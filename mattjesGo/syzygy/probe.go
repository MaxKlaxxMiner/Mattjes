package syzygy

import (
	"math/bits"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// WDL values for the side to move.
const (
	Loss        = -2
	BlessedLoss = -1 // loss, but a draw under the fifty-move rule
	Draw        = 0
	CursedWin   = 1 // win, but a draw under the fifty-move rule
	Win         = 2
)

// counts returns the piece counts per kind of both colours.
func counts(b *bitboard.Board) (white, black [6]uint8) {
	for kind := 0; kind < 6; kind++ {
		white[kind] = uint8(bits.OnesCount64(b.Pieces[0][kind]))
		black[kind] = uint8(bits.OnesCount64(b.Pieces[1][kind]))
	}
	return
}

// Has reports whether a table covers the board's material.
func (t *Tables) Has(b *bitboard.Board) bool {
	white, black := counts(b)
	_, ok := t.find(materialKey(white, black))
	return ok
}

// ProbeWDL returns the WDL value of the position for the side to move
// (Loss .. Win) and false when no table covers the material. The tables
// store "don't care" values where a capture decides, so captures (and
// en passant) are resolved by a small search first, as in Fathom's
// probe_wdl. Castling rights are not supported (no table has them).
func (t *Tables) ProbeWDL(b *bitboard.Board) (int, bool) {
	t.Probes.Add(1)
	white, black := counts(b)
	if _, ok := t.find(materialKey(white, black)); !ok {
		if t.base != nil {
			return t.baseWDL(b)
		}
		return 0, false
	}
	var buf chess.MoveBuffer
	n := b.GenMoves(&buf)
	bestCap, bestEp := -3, -3
	for _, m := range buf[:n] {
		ep := isEnPassant(b, m)
		if m.Capture == chess.None && !ep {
			continue
		}
		c := *b
		c.DoMove(m)
		v, ok := t.probeAB(&c, -2, -bestCap)
		if !ok {
			return 0, false
		}
		v = -v
		if v > bestCap {
			if v == Win {
				t.Hits.Add(1)
				return Win, true
			}
			if !ep {
				bestCap = v
			} else if v > bestEp {
				bestEp = v
			}
		}
	}
	v, ok := t.probeTable(b)
	if !ok {
		return 0, false
	}
	t.Hits.Add(1)
	// max(v, bestCap) is the value without en passant rights; an en passant
	// capture can only improve on it, except in the stalemate case below
	if bestEp > bestCap {
		if bestEp > v {
			return bestEp, true
		}
		bestCap = bestEp
	}
	if bestCap >= v {
		return bestCap, true
	}
	if bestEp > -3 && v == Draw {
		// the table says draw because without the en passant capture the
		// side to move would be stalemated: then the capture is forced
		stalemate := !b.InCheck()
		for _, m := range buf[:n] {
			if !isEnPassant(b, m) {
				stalemate = false
				break
			}
		}
		if stalemate {
			return bestEp, true
		}
	}
	return v, true
}

// probeAB resolves captures below a capture (never called with en passant
// rights: a capture clears them).
func (t *Tables) probeAB(b *bitboard.Board, alpha, beta int) (int, bool) {
	var buf chess.MoveBuffer
	n := b.GenMoves(&buf)
	for _, m := range buf[:n] {
		if m.Capture == chess.None {
			continue
		}
		c := *b
		c.DoMove(m)
		v, ok := t.probeAB(&c, -beta, -alpha)
		if !ok {
			return 0, false
		}
		v = -v
		if v > alpha {
			if v >= beta {
				return v, true
			}
			alpha = v
		}
	}
	v, ok := t.probeTable(b)
	if !ok {
		return 0, false
	}
	return max(alpha, v), true
}

func isEnPassant(b *bitboard.Board, m chess.Move) bool {
	return m.Capture == chess.None && m.To == b.EnPassant && b.Squares[m.From].Type() == chess.Pawn
}

// probeTable looks the position up in its table without capture
// resolution (Fathom's probe_table for WDL).
func (t *Tables) probeTable(b *bitboard.Board) (int, bool) {
	white, black := counts(b)
	if bits.OnesCount64(b.ByColor[0]|b.ByColor[1]) == 2 {
		return Draw, true // KvK has no table
	}
	l, ok := t.find(materialKey(white, black))
	if !ok {
		if t.base != nil {
			return t.baseWDL(b) // exact, captures included
		}
		return 0, false
	}
	be := l.be
	if !t.prepare(be) {
		return 0, false
	}
	var flip, bside bool
	if !be.symmetric {
		flip = l.flip
		bside = b.WhiteMove == flip
	} else {
		flip = !b.WhiteMove
		bside = false
	}
	var p [maxPieces]int
	var ei *encInfo
	var idx uint64
	if !be.hasPawns {
		if bside {
			ei = &be.ei[1]
		} else {
			ei = &be.ei[0]
		}
		for i := 0; i < int(be.num); {
			i = fillSquares(b, ei.pieces[:], flip, 0, p[:], i)
		}
		idx = encode(p[:], ei, be, encPiece)
	} else {
		mirror := 0
		if flip {
			mirror = 0x38
		}
		i := fillSquares(b, be.ei[0].pieces[:], flip, mirror, p[:], 0)
		f := leadingPawn(p[:], be, encFile)
		if bside {
			ei = &be.ei[f+4]
		} else {
			ei = &be.ei[f]
		}
		for i < int(be.num) {
			i = fillSquares(b, ei.pieces[:], flip, mirror, p[:], i)
		}
		idx = encode(p[:], ei, be, encFile)
	}
	if ei.own != nil {
		return int(ei.own.Value(int(idx)))*2 - 2, true // our tables store 0 loss, 1 draw, 2 win
	}
	w := ei.precomp.decompress(idx)
	return int(w[0]) - 2, true
}

// SetBase gives the prober the exact base tables for the materials without
// a WDL file: our own WDL tables (write.go) cover one material each, and a
// probe resolves captures into smaller materials first.
func (t *Tables) SetBase(set *egtb.Set) { t.base = set }

// baseWDL answers from the base tables (no WDL file for the material).
func (t *Tables) baseWDL(b *bitboard.Board) (int, bool) {
	v, ok := t.base.Lookup(b)
	if !ok {
		return 0, false
	}
	switch {
	case v.IsWin():
		return Win, true
	case v.IsLoss():
		return Loss, true
	}
	return Draw, true
}

// fillSquares appends the squares of the pieces with the Syzygy code pc[i]
// (1 = white pawn .. 6 = white king, 9 .. 14 black; colours swapped with
// flip) to p, as Syzygy squares (a1 = 0) XOR mirror, and returns the next
// index. Pieces of one kind are consecutive in the table's order.
func fillSquares(b *bitboard.Board, pc []uint8, flip bool, mirror int, p []int, i int) int {
	color := int(pc[i] >> 3)
	if flip {
		color ^= 1
	}
	kind := 6 - int(pc[i]&7) // Syzygy pawn 1 .. king 6, bitboard king 0 .. pawn 5
	bb := b.Pieces[color][kind]
	for bb != 0 {
		sq := bits.TrailingZeros64(bb)
		p[i] = (sq ^ 0x38) ^ mirror // a8 = 0 in the bitboard, a1 = 0 in Syzygy
		i++
		bb &= bb - 1
	}
	return i
}
