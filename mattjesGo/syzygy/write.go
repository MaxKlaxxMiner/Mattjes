package syzygy

// Our own WDL tables in Syzygy's structure (docs/m5-endgame-tables.md,
// "Eigene WDL-Tabellen"): the same position index (encode), one stream per
// side to move (and per file of the leading pawn), the piece order of every
// stream chosen by trying the candidates like tbgen does, dead, illegal and
// capture-decided positions as don't care, the streams packed with package
// pairs. The values come from an exact egtb table (DTM), so there are no
// fifty-move shades: 0 loss, 1 draw, 2 win for the side to move.

import (
	"bufio"
	"encoding/binary"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"sync"
	"sync/atomic"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/pairs"
)

// codeOf maps a coloured piece to its Syzygy code (pawn 1 .. king 6, black
// + 8).
func codeOf(p chess.Piece) uint8 {
	var c uint8
	switch p.Type() {
	case chess.King:
		c = 6
	case chess.Queen:
		c = 5
	case chess.Rook:
		c = 4
	case chess.Bishop:
		c = 3
	case chess.Knight:
		c = 2
	case chess.Pawn:
		c = 1
	}
	if p.Color() == chess.Black {
		c += 8
	}
	return c
}

// kindOf is the bitboard kind (K 0 .. P 5) of a piece.
func kindOf(p chess.Piece) int { return 6 - int(codeOf(p)&7) }

// codeName renders a Syzygy piece code ("K" white, "k" black).
func codeName(c uint8) string {
	s := "?PNBRQK"[c&7 : c&7+1]
	if c&8 != 0 {
		s = string(s[0] + 'a' - 'A')
	}
	return s
}

// countsOf gives the piece counts of a material per side (our white is the
// first side), in bitboard kind order.
func countsOf(m egtb.Material) (first, second [6]uint8) {
	first[0], second[0] = 1, 1
	for _, p := range m.White {
		first[kindOf(p)]++
	}
	for _, p := range m.Black {
		second[kindOf(p)]++
	}
	return
}

// piecesOf lists the coloured pieces of an egtb table in its square order
// (white king, black king, then the slots).
func piecesOf(m egtb.Material) []chess.Piece {
	ps := []chess.Piece{chess.WhiteKing, chess.BlackKing}
	for _, p := range m.White {
		ps = append(ps, chess.White|p)
	}
	for _, p := range m.Black {
		ps = append(ps, chess.Black|p)
	}
	return ps
}

// DontCare marks a position whose stored value does not matter; a symbol
// with the boundedFlag marks one whose stored value may be anything up to
// the symbol below the flag.
const (
	DontCare    = 0xff
	boundedFlag = 0x80
)

// Source is the value stream of a material taken from an exact egtb table,
// per index a symbol: for WDL 0 loss, 1 draw, 2 win for the side to move,
// DontCare for dead and illegal indices, or boundedFlag|symbol for
// positions whose best capture already achieves their value (the probe
// takes the maximum of the best capture and the stored value, so the
// stored value may be anything up to the real one, anything at all when a
// capture wins). For DTM (NewSourceDTM) the symbol is the quantised
// distance bucket, draws free when a WDL table carries them.
type Source struct {
	Table    *egtb.Table
	Set      *egtb.Set // the tables the captures lead into (the sampling looks positions up here)
	Symbols  []byte
	Alphabet int // symbols the stream uses
	Legal    int // legal, canonical positions
	Decided  int // of them decided by a capture (WDL) or free as draws (DTM)
	dtm      bool
	step     int  // DTM: moves per bucket
	signed   bool // DTM: wins and losses apart (without, the WDL table tells)
	drawFree bool // DTM: draws as don't care
}

func symbolOf(v egtb.Value) byte {
	switch {
	case v.IsWin():
		return 2
	case v.IsLoss():
		return 0
	}
	return 1
}

// symbolAt is the stream symbol of a legal position with the table value v.
func (src *Source) symbolAt(b *bitboard.Board, buf *chess.MoveBuffer, v egtb.Value) byte {
	if !src.dtm {
		return captureBound(src.Set, b, buf, symbolOf(v))
	}
	q := egtb.Quantize(v, src.step)
	switch {
	case q == egtb.Draw:
		if src.drawFree {
			return DontCare
		}
		return 0
	case q.IsWin():
		return byte((int(q) + src.step - 1) / src.step) // 1 .. 127/step
	}
	m := int(q) - 128 // loss: bucket 0 = mated now
	bucket := byte((m + src.step - 1) / src.step)
	if src.signed {
		return byte(127/src.step) + 1 + bucket
	}
	return bucket
}

// NewSource derives the WDL symbols from a table with exact values in
// memory, in parallel.
func NewSource(set *egtb.Set, t *egtb.Table, workers int) *Source {
	return newSource(set, t, workers, &Source{Alphabet: 3})
}

// NewSourceDTM derives the quantised distance symbols (buckets of step
// moves; signed = wins and losses apart, else the bucket alone; drawFree =
// draws as don't care) from a table with exact values in memory.
func NewSourceDTM(set *egtb.Set, t *egtb.Table, step int, signed, drawFree bool, workers int) *Source {
	buckets := (127+step-1)/step + 1 // 0 .. ceil(127/step): a win in 127 moves rounds up to bucket 16 at step 8
	alphabet := buckets
	if signed {
		alphabet = 2*buckets + 1
	}
	return newSource(set, t, workers, &Source{Alphabet: alphabet, dtm: true, step: step, signed: signed, drawFree: drawFree})
}

func newSource(set *egtb.Set, t *egtb.Table, workers int, src *Source) *Source {
	if t.Values == nil || !t.Exact() {
		panic("syzygy: NewSource needs an exact table in memory: " + t.Mat.Name())
	}
	pieces := piecesOf(t.Mat)
	n := t.Size
	src.Table, src.Set, src.Symbols = t, set, make([]byte, n)
	var mu sync.Mutex
	var wg sync.WaitGroup
	chunk := (n + workers - 1) / workers
	for w := 0; w < workers; w++ {
		lo, hi := w*chunk, min(n, (w+1)*chunk)
		wg.Add(1)
		go func() {
			defer wg.Done()
			sq := make([]chess.Pos, len(pieces))
			var buf chess.MoveBuffer
			legal, decided := 0, 0
			for idx := lo; idx < hi; idx++ {
				src.Symbols[idx] = DontCare
				whiteMove := t.Decode(idx, sq)
				if !distinctSquares(sq) || t.Index(whiteMove, sq) != idx {
					continue
				}
				b := bitboard.FromPieces(whiteMove, sq, pieces)
				if b.OpponentInCheck() {
					continue
				}
				legal++
				own := src.symbolAt(&b, &buf, t.Values[idx])
				if own&boundedFlag != 0 { // bounded, or DontCare (a free draw)
					decided++
				}
				src.Symbols[idx] = own
			}
			mu.Lock()
			src.Legal += legal
			src.Decided += decided
			mu.Unlock()
		}()
	}
	wg.Wait()
	return src
}

func distinctSquares(sq []chess.Pos) bool {
	var seen uint64
	for _, s := range sq {
		if seen&(1<<uint(s)) != 0 {
			return false
		}
		seen |= 1 << uint(s)
	}
	return true
}

// Order is the piece order of one stream (Syzygy codes, the leading group
// first) with the chain positions of the leading group and of the other
// colour's pawns (0x0f without).
type Order struct {
	Pieces []uint8
	Order  int
	Order2 int
}

func (o Order) String() string {
	s := ""
	for _, c := range o.Pieces {
		s += codeName(c)
	}
	s += fmt.Sprintf("/%d", o.Order)
	if o.Order2 != 0x0f {
		s += fmt.Sprintf("/%d", o.Order2)
	}
	return s
}

type group struct {
	code  uint8
	count int
}

func permutations(n int) [][]int {
	if n == 0 {
		return [][]int{{}}
	}
	var out [][]int
	for _, rest := range permutations(n - 1) {
		for pos := 0; pos <= n-1; pos++ {
			p := make([]int, 0, n)
			p = append(p, rest[:pos]...)
			p = append(p, n-1)
			p = append(p, rest[pos:]...)
			out = append(out, p)
		}
	}
	return out
}

// Candidates lists the piece orders worth trying for a stream of the
// material: pawnless with the two kings as the only singles, the kings in
// either order and every order of the other groups; otherwise every ordered
// triple of singles as the leading group and every order of the rest; pawn
// tables lead with the pawns of the leading colour, then the other pawns,
// then every order of the pieces. The chain position of the leading group
// (and of the other pawns) runs over every slot.
func Candidates(be *baseEntry, first, second [6]uint8) []Order {
	var groups []group
	for colour := 0; colour < 2; colour++ {
		counts := first
		if colour == 1 {
			counts = second
		}
		for kind := 0; kind < 6; kind++ {
			if counts[kind] > 0 {
				groups = append(groups, group{uint8(6-kind) | uint8(colour)<<3, int(counts[kind])})
			}
		}
	}
	expand := func(lead []uint8, rest []group, perm []int) []uint8 {
		pieces := append([]uint8(nil), lead...)
		for _, i := range perm {
			for k := 0; k < rest[i].count; k++ {
				pieces = append(pieces, rest[i].code)
			}
		}
		return pieces
	}
	var out []Order
	if !be.hasPawns {
		var singles []uint8
		var multis []group
		for _, g := range groups {
			if g.count == 1 {
				singles = append(singles, g.code)
			} else {
				multis = append(multis, g)
			}
		}
		var leads [][]uint8
		var rests [][]group
		if be.kkEnc {
			wk, bk := uint8(6), uint8(14)
			leads = [][]uint8{{wk, bk}, {bk, wk}}
			rests = [][]group{multis, multis}
		} else {
			for _, a := range singles {
				for _, b := range singles {
					for _, c := range singles {
						if a == b || a == c || b == c {
							continue
						}
						leads = append(leads, []uint8{a, b, c})
						rest := append([]group(nil), multis...)
						for _, s := range singles {
							if s != a && s != b && s != c {
								rest = append(rest, group{s, 1})
							}
						}
						rests = append(rests, rest)
					}
				}
			}
		}
		for i, lead := range leads {
			rest := rests[i]
			for _, perm := range permutations(len(rest)) {
				pieces := expand(lead, rest, perm)
				for order := 0; order <= len(rest); order++ {
					out = append(out, Order{pieces, order, 0x0f})
				}
			}
		}
		return out
	}
	leadColour := 0
	if second[5] != 0 && (first[5] == 0 || first[5] > second[5]) {
		leadColour = 1
	}
	var lead []uint8
	var rest []group
	other := false
	for _, g := range groups {
		switch {
		case g.code&7 == 1 && int(g.code>>3) == leadColour:
			for k := 0; k < g.count; k++ {
				lead = append(lead, g.code)
			}
		case g.code&7 == 1:
			other = true
		default:
			rest = append(rest, g)
		}
	}
	if other {
		code := uint8(1) | uint8(1-leadColour)<<3
		for k := 0; k < int(first[5]+second[5])-len(lead); k++ {
			lead = append(lead, code)
		}
	}
	chain := len(rest) + 1
	if other {
		chain++
	}
	for _, perm := range permutations(len(rest)) {
		pieces := expand(lead, rest, perm)
		for order := 0; order < chain; order++ {
			if !other {
				out = append(out, Order{pieces, order, 0x0f})
				continue
			}
			for order2 := 0; order2 < chain; order2++ {
				if order2 != order {
					out = append(out, Order{pieces, order, order2})
				}
			}
		}
	}
	return out
}

// Stream is one packed value stream: a side to move (and a pawn file).
type Stream struct {
	Order  Order
	Size   uint64 // positions
	Packed []byte
	Stats  pairs.Stats
}

// Built is a WDL table ready to be written: its material and its streams
// (per side to move, pawn files innermost; one side for symmetric
// materials).
type Built struct {
	Name          string
	First, Second [6]uint8
	Sides, Files  int
	Streams       []Stream
	Candidates    int
	// DTM: the streams hold distance buckets of Step moves instead of WDL
	// symbols (Signed: wins and losses apart; DrawFree: draws are don't care,
	// the WDL table tells them)
	DTM              bool
	Step             int
	Signed, DrawFree bool
	Alphabet         int
}

// Bytes is the size of all packed streams.
func (b *Built) Bytes() int {
	n := 0
	for _, s := range b.Streams {
		n += len(s.Packed)
	}
	return n
}

// BuildOptions control the candidate preselection: a large stream is not
// built for every candidate order; instead windows spread over the stream
// are enumerated with the decoder, looked up and packed, and only the best
// candidates of that sample are built completely.
type BuildOptions struct {
	// SampleTarget: positions sampled per stream (0 = 2 Mi); a stream with
	// fewer than twice that many positions is built completely for every
	// candidate.
	SampleTarget int
	// Finalists: candidates built completely per stream after the sampling (0 = 3).
	Finalists int
	// Window: positions per sample window (0 = 4096).
	Window int
	// DictAbove: a stream with more positions than this is packed with a
	// dictionary from a sample of DictSample values (pairs.Options.DictSample)
	// instead of Re-Pair over the whole stream (0 = 512 Mi; a five-piece
	// stream of 115 Mi packs completely in half a minute, the sample would
	// cost 14 % size for no time; a six-piece stream of billions would take
	// hours).
	DictAbove int
	// DictSample: the sample of the dictionary above DictAbove (0 = 16 Mi).
	DictSample int
}

func (bo BuildOptions) withDefaults() BuildOptions {
	if bo.SampleTarget <= 0 {
		bo.SampleTarget = 2 << 20
	}
	if bo.Finalists <= 0 {
		bo.Finalists = 3
	}
	if bo.Window <= 0 {
		bo.Window = 4096
	}
	if bo.DictAbove <= 0 {
		bo.DictAbove = 512 << 20
	}
	if bo.DictSample <= 0 {
		bo.DictSample = 16 << 20
	}
	return bo
}

// Build packs the WDL of a source: every candidate order per side on small
// streams, the finalists of the sampled windows on large ones (BuildOptions),
// keeping the smallest stream per side and file. progress (optional) gets a
// line per candidate.
func Build(src *Source, opt pairs.Options, bo BuildOptions, workers int, progress func(string)) *Built {
	bo = bo.withDefaults()
	first, second := countsOf(src.Table.Mat)
	be := newEntry(Name(first, second), first, second)
	if be == nil {
		panic("syzygy: too many pieces for " + src.Table.Mat.Name())
	}
	b := &Built{Name: be.name, First: first, Second: second, Sides: 2, Files: 1, DTM: src.dtm, Step: src.step, Signed: src.signed, DrawFree: src.drawFree, Alphabet: src.Alphabet}
	if be.symmetric {
		b.Sides = 1
	}
	if be.hasPawns {
		b.Files = 4
	}
	b.Streams = make([]Stream, b.Sides*b.Files)
	cands := Candidates(be, first, second)
	b.Candidates = len(cands)
	set := src.Set
	for side := 0; side < b.Sides; side++ {
		chosen := cands
		var ei encInfo
		copy(ei.pieces[:], cands[0].Pieces)
		enc := encPiece
		if be.hasPawns {
			enc = encFile
		}
		if size := ei.setup(be, cands[0].Order, cands[0].Order2, 0, enc); set != nil && size > 2*uint64(bo.SampleTarget) && len(cands) > bo.Finalists {
			// the sample: per candidate and file the packed size of the windows
			scores := make([][]int, len(cands))
			for c, o := range cands {
				scores[c] = sampleSide(src, be, side, o, b.Files, bo, opt, workers)
				if progress != nil {
					progress(fmt.Sprintf("%s side %d order %-12s sample %v bytes", be.name, side, o, scores[c]))
				}
			}
			// the finalists: the best of every file, as a set
			picked := map[int]bool{}
			for f := 0; f < b.Files; f++ {
				order := make([]int, len(cands))
				for c := range order {
					order[c] = c
				}
				sort.SliceStable(order, func(x, y int) bool { return scores[order[x]][f] < scores[order[y]][f] })
				for _, c := range order[:bo.Finalists] {
					picked[c] = true
				}
			}
			chosen = nil
			for c, o := range cands {
				if picked[c] {
					chosen = append(chosen, o)
				}
			}
		}
		for _, o := range chosen {
			values, sizes := buildSide(src, be, side, o, b.Files, workers)
			for f := 0; f < b.Files; f++ {
				po := opt
				if sizes[f] > uint64(bo.DictAbove) && po.DictSample == 0 {
					po.DictSample = bo.DictSample
				}
				packed, st := pairs.PackStats(values[f], src.Alphabet, po)
				s := &b.Streams[side*b.Files+f]
				if s.Packed == nil || len(packed) < len(s.Packed) {
					*s = Stream{Order: o, Size: sizes[f], Packed: packed, Stats: st}
				}
				if progress != nil {
					progress(fmt.Sprintf("%s side %d file %d order %-12s %8d bytes (%d tokens, %d symbols)", be.name, side, f, o, len(packed), st.Tokens, st.Symbols))
				}
			}
		}
	}
	return b
}

// pieceOfCode is the coloured piece of a Syzygy code.
func pieceOfCode(c uint8) chess.Piece {
	var p chess.Piece
	switch c & 7 {
	case 1:
		p = chess.Pawn
	case 2:
		p = chess.Knight
	case 3:
		p = chess.Bishop
	case 4:
		p = chess.Rook
	case 5:
		p = chess.Queen
	default:
		p = chess.King
	}
	if c&8 != 0 {
		return chess.Black | p
	}
	return chess.White | p
}

// captureBound returns the symbol as the stream stores it: own, or
// bounded+own when the best capture already achieves it (DontCare never).
func captureBound(set *egtb.Set, b *bitboard.Board, buf *chess.MoveBuffer, own byte) byte {
	best, hasCapture := byte(0), false
	for _, m := range buf[:b.GenMoves(buf)] {
		if m.Capture == chess.None {
			continue
		}
		c := *b
		c.DoMove(m)
		v, ok := set.Lookup(&c)
		if !ok {
			panic("syzygy: no table after " + c.FEN())
		}
		hasCapture = true
		best = max(best, 2-symbolOf(v))
	}
	if hasCapture && best == own {
		return own | boundedFlag
	}
	return own
}

// sampleSide judges a candidate order of one side: windows spread over
// every file's stream are enumerated with the decoder, looked up in the
// tables, filled like the real stream and packed; returns the packed bytes
// per file.
func sampleSide(src *Source, be *baseEntry, side int, o Order, files int, bo BuildOptions, opt pairs.Options, workers int) []int {
	set := src.Set
	enc := encPiece
	if be.hasPawns {
		enc = encFile
	}
	pieces := make([]chess.Piece, be.num)
	for i, c := range o.Pieces {
		pieces[i] = pieceOfCode(c)
	}
	scores := make([]int, files)
	for f := 0; f < files; f++ {
		var ei encInfo
		copy(ei.pieces[:], o.Pieces)
		size := ei.setup(be, o.Order, o.Order2, f, enc)
		d := newDecoder(be, &ei, enc, f)
		window := uint64(bo.Window)
		numWindows := uint64(max(1, bo.SampleTarget/bo.Window))
		if window*numWindows >= size {
			window, numWindows = size, 1
		}
		sample := make([]byte, window*numWindows)
		var next atomic.Int64
		var wg sync.WaitGroup
		for w := 0; w < max(1, min(workers, int(numWindows))); w++ {
			wg.Add(1)
			go func() {
				defer wg.Done()
				var p [maxPieces]int
				sq := make([]chess.Pos, be.num)
				var buf chess.MoveBuffer
				for {
					wi := uint64(next.Add(1) - 1)
					if wi >= numWindows {
						return
					}
					start := uint64(0)
					if numWindows > 1 {
						start = wi * (size - window) / (numWindows - 1)
					}
					out := sample[wi*window : (wi+1)*window]
					for i := range out {
						d.squares(start+uint64(i), p[:be.num])
						for j := 0; j < int(be.num); j++ {
							sq[j] = chess.Pos(p[j] ^ 0x38)
						}
						b := bitboard.FromPieces(side == 0, sq, pieces)
						out[i] = DontCare
						if b.OpponentInCheck() {
							continue
						}
						v, ok := set.Lookup(&b)
						if !ok {
							continue // adjacent kings
						}
						out[i] = src.symbolAt(&b, &buf, v)
					}
				}
			}()
		}
		wg.Wait()
		fillDontCare(sample)
		_, st := pairs.PackStats(sample, src.Alphabet, opt)
		scores[f] = st.Bytes
	}
	return scores
}

// fillDontCare replaces the don't-care symbols by their predecessor
// (bounded ones by the predecessor when it fits, else the bound).
func fillDontCare(values []byte) {
	prev := byte(1)
	for i, v := range values {
		switch {
		case v == DontCare:
			values[i] = prev
		case v&boundedFlag != 0:
			if bound := v &^ boundedFlag; prev > bound {
				prev = bound
			}
			values[i] = prev
		default:
			prev = v
		}
	}
}

// buildSide lays the symbols of one side to move out in the index order of
// a candidate (one stream per pawn file), don't-care positions filled with
// their predecessor (bounded ones with the predecessor when it fits, else
// the bound).
func buildSide(src *Source, be *baseEntry, side int, o Order, files int, workers int) (values [][]byte, sizes []uint64) {
	enc := encPiece
	if be.hasPawns {
		enc = encFile
	}
	eis := make([]encInfo, files)
	sizes = make([]uint64, files)
	values = make([][]byte, files)
	for f := range eis {
		copy(eis[f].pieces[:], o.Pieces)
		sizes[f] = eis[f].setup(be, o.Order, o.Order2, f, enc)
		values[f] = make([]byte, sizes[f])
		for i := range values[f] {
			values[f][i] = DontCare
		}
	}
	t := src.Table
	pieces := piecesOf(t.Mat)
	n := t.Size
	var wg sync.WaitGroup
	chunk := (n + workers - 1) / workers
	for w := 0; w < workers; w++ {
		lo, hi := w*chunk, min(n, (w+1)*chunk)
		wg.Add(1)
		go func() {
			defer wg.Done()
			sq := make([]chess.Pos, len(pieces))
			var p [maxPieces]int
			for idx := lo; idx < hi; idx++ {
				s := src.Symbols[idx]
				if s == DontCare {
					continue
				}
				if t.Decode(idx, sq) != (side == 0) {
					continue // the other side's stream (a symmetric material: the colour-swapped twin)
				}
				for i := 0; i < len(o.Pieces); {
					i = squaresOf(o.Pieces[i], pieces, sq, p[:], i)
				}
				f := 0
				if be.hasPawns {
					f = leadingPawn(p[:], be, encFile)
				}
				q := p
				values[f][encode(q[:], &eis[f], be, enc)] = s
				// our index folds the diagonal transposition always, Syzygy's
				// only by the first off-diagonal piece of the leading group:
				// with the whole group on the diagonal the transposed twin is
				// a position of its own in the stream
				if !be.hasPawns {
					// the squares as encode sees them before its diagonal step:
					// mirrored into the a1-d4 quadrant by the first piece
					q = p
					if q[0]&4 != 0 {
						for i := range q[:be.num] {
							q[i] ^= 7
						}
					}
					if q[0]&0x20 != 0 {
						for i := range q[:be.num] {
							q[i] ^= 0x38
						}
					}
					onDiagonal := true
					for i := 0; i < int(eis[0].norm[0]); i++ {
						if offDiag[q[i]] != 0 {
							onDiagonal = false
							break
						}
					}
					if onDiagonal {
						for i := range q[:be.num] {
							q[i] = int(flipDiag[q[i]])
						}
						values[f][encode(q[:], &eis[f], be, enc)] = s
					}
				}
			}
		}()
	}
	wg.Wait()
	for f := range values {
		fillDontCare(values[f])
	}
	return values, sizes
}

// --- the file ---

// The own WDL file: a 32-byte header ("MWDL", version, pieces, sides, files,
// the piece counts of both sides), one 32-byte descriptor per stream (piece
// codes, chain positions, offset and length of its container) and the
// containers of package pairs, each 8-aligned. Streams are ordered side by
// side, pawn files innermost.
const (
	ownMagic      = "MWDL"
	dtmMagic      = "MDTM" // the same layout with byte 20 = step, 21 = flags, 22 = alphabet
	ownVersion    = 1
	ownHeader     = 32
	ownDescriptor = 32
	dtmSigned     = 1
	dtmDrawFree   = 2
)

// WriteDTM derives, builds and writes the own DTM table of an exact egtb
// table to path: the distance buckets of egtb.FileStep moves, wins and
// losses as one bucket (the WDL table tells which), draws free. Read
// through Tables.ProbeDTM after a WDL verdict.
func WriteDTM(set *egtb.Set, t *egtb.Table, path string, opt pairs.Options, bo BuildOptions, workers int, progress func(string)) (*Built, error) {
	src := NewSourceDTM(set, t, egtb.FileStep, false, true, workers)
	b := Build(src, opt, bo, workers, progress)
	return b, b.Write(path)
}

// Write writes the table to path (the folder is created).
func (b *Built) Write(path string) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	n := len(b.Streams)
	head := make([]byte, ownHeader+ownDescriptor*n)
	copy(head, ownMagic)
	head[4] = ownVersion
	pieces := 0
	for _, c := range b.First {
		pieces += int(c)
	}
	for _, c := range b.Second {
		pieces += int(c)
	}
	head[5], head[6], head[7] = byte(pieces), byte(b.Sides), byte(b.Files)
	copy(head[8:], b.First[:])
	copy(head[14:], b.Second[:])
	if b.DTM {
		copy(head, dtmMagic)
		head[20] = byte(b.Step)
		if b.Signed {
			head[21] |= dtmSigned
		}
		if b.DrawFree {
			head[21] |= dtmDrawFree
		}
		head[22] = byte(b.Alphabet)
	}
	offset := len(head)
	for i, s := range b.Streams {
		d := head[ownHeader+ownDescriptor*i:]
		copy(d, s.Order.Pieces)
		d[7], d[8] = byte(s.Order.Order), byte(s.Order.Order2)
		binary.LittleEndian.PutUint64(d[16:], uint64(offset))
		binary.LittleEndian.PutUint64(d[24:], uint64(len(s.Packed)))
		offset += (len(s.Packed) + 7) &^ 7
	}
	// written beside the target and renamed when complete: a reader never
	// sees a partial file (a sweep may be killed while writing)
	tmp := egtb.TmpPath(path)
	f, err := os.Create(tmp)
	if err != nil {
		return err
	}
	w := bufio.NewWriterSize(f, 1<<20)
	w.Write(head)
	for _, s := range b.Streams {
		w.Write(s.Packed)
		for k := len(s.Packed); k&7 != 0; k++ {
			w.WriteByte(0)
		}
	}
	if err := w.Flush(); err != nil {
		f.Close()
		return err
	}
	if err := f.Close(); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

// WriteWDL derives, builds and writes the own WDL table of an exact egtb
// table to path; returns what was built.
func WriteWDL(set *egtb.Set, t *egtb.Table, path string, opt pairs.Options, bo BuildOptions, workers int, progress func(string)) (*Built, error) {
	src := NewSource(set, t, workers)
	b := Build(src, opt, bo, workers, progress)
	return b, b.Write(path)
}

// squaresOf appends the Syzygy squares of every piece with the code c (in
// ascending bitboard square order, as fillSquares lists them) to p at i and
// returns the next index.
func squaresOf(c uint8, pieces []chess.Piece, sq []chess.Pos, p []int, i int) int {
	start := i
	for k, pc := range pieces {
		if codeOf(pc) == c {
			p[i] = int(sq[k])
			i++
		}
	}
	sort.Ints(p[start:i])
	for k := start; k < i; k++ {
		p[k] ^= 0x38
	}
	return i
}
