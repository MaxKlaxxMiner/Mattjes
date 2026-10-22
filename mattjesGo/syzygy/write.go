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
// bounded+v marks one whose stored value may be anything up to v.
const (
	DontCare = 0xff
	bounded  = 3
)

// Source is the WDL of a material taken from an exact egtb table: per
// index the symbol (0 loss, 1 draw, 2 win for the side to move), DontCare
// for dead and illegal indices, or bounded+symbol for positions whose best
// capture already achieves their value: the probe takes the maximum of the
// best capture and the stored value, so the stored value may be anything up
// to the real one (anything at all when a capture wins).
type Source struct {
	Table   *egtb.Table
	Symbols []byte
	Legal   int // legal, canonical positions
	Decided int // of them decided by a capture
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

// NewSource derives the symbols from a table with exact values in memory,
// in parallel.
func NewSource(set *egtb.Set, t *egtb.Table, workers int) *Source {
	if t.Values == nil || !t.Exact() {
		panic("syzygy: NewSource needs an exact table in memory: " + t.Mat.Name())
	}
	pieces := piecesOf(t.Mat)
	n := t.Size
	src := &Source{Table: t, Symbols: make([]byte, n)}
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
				own := symbolOf(t.Values[idx])
				best, hasCapture := byte(0), false
				for _, m := range buf[:b.GenMoves(&buf)] {
					if m.Capture == chess.None {
						continue
					}
					c := b
					c.DoMove(m)
					v, ok := set.Lookup(&c)
					if !ok {
						panic("syzygy: no table after " + c.FEN())
					}
					hasCapture = true
					best = max(best, 2-symbolOf(v)) // the child's value is the opponent's
				}
				if hasCapture && best == own {
					decided++
					own += bounded
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
}

// Bytes is the size of all packed streams.
func (b *Built) Bytes() int {
	n := 0
	for _, s := range b.Streams {
		n += len(s.Packed)
	}
	return n
}

// Build packs the WDL of a source, trying every candidate order per side
// and keeping the smallest stream per side and file. progress (optional)
// gets a line per candidate.
func Build(src *Source, opt pairs.Options, workers int, progress func(string)) *Built {
	first, second := countsOf(src.Table.Mat)
	be := newEntry(Name(first, second), first, second)
	if be == nil {
		panic("syzygy: too many pieces for " + src.Table.Mat.Name())
	}
	b := &Built{Name: be.name, First: first, Second: second, Sides: 2, Files: 1}
	if be.symmetric {
		b.Sides = 1
	}
	if be.hasPawns {
		b.Files = 4
	}
	b.Streams = make([]Stream, b.Sides*b.Files)
	cands := Candidates(be, first, second)
	b.Candidates = len(cands)
	for side := 0; side < b.Sides; side++ {
		for _, o := range cands {
			values, sizes := buildSide(src, be, side, o, b.Files, workers)
			for f := 0; f < b.Files; f++ {
				packed, st := pairs.PackStats(values[f], 3, opt)
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
		prev := byte(1)
		for i, v := range values[f] {
			switch {
			case v == DontCare:
				values[f][i] = prev
			case v >= bounded: // the predecessor when it fits the bound, else the bound
				if prev > v-bounded {
					prev = v - bounded
				}
				values[f][i] = prev
			default:
				prev = v
			}
		}
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
	ownVersion    = 1
	ownHeader     = 32
	ownDescriptor = 32
)

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
	offset := len(head)
	for i, s := range b.Streams {
		d := head[ownHeader+ownDescriptor*i:]
		copy(d, s.Order.Pieces)
		d[7], d[8] = byte(s.Order.Order), byte(s.Order.Order2)
		binary.LittleEndian.PutUint64(d[16:], uint64(offset))
		binary.LittleEndian.PutUint64(d[24:], uint64(len(s.Packed)))
		offset += (len(s.Packed) + 7) &^ 7
	}
	f, err := os.Create(path)
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
	return f.Close()
}

// WriteWDL derives, builds and writes the own WDL table of an exact egtb
// table to path; returns what was built.
func WriteWDL(set *egtb.Set, t *egtb.Table, path string, opt pairs.Options, workers int, progress func(string)) (*Built, error) {
	src := NewSource(set, t, workers)
	b := Build(src, opt, workers, progress)
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
