package egtb

import (
	"fmt"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// Board symmetry. A transform id has three bits: 1 = mirror files (a<->h),
// 2 = mirror ranks (1<->8), 4 = transpose (a1-h8 diagonal). xform[t][sq] is
// the transformed square. Without pawns all eight are available, with pawns
// only the file mirror.
var xform [8][chess.FieldCount]chess.Pos

func transformSquare(t int, sq chess.Pos) chess.Pos {
	f, r := sq.X(), chess.Height-1-sq.Y() // file and rank, both 0..7 with a1 = (0,0)
	if t&1 != 0 {
		f = chess.Width - 1 - f
	}
	if t&2 != 0 {
		r = chess.Height - 1 - r
	}
	if t&4 != 0 {
		f, r = r, f
	}
	return chess.PosFromXY(f, chess.Height-1-r)
}

// King pair tables. The index of a king pair is dense (only legal, canonical
// pairs count), the transform tells how the other pieces must be moved along.
const (
	KKPawnless = 462  // white king in the triangle a1-d1-d4, black king below or on the diagonal when white's is on it
	KKPawns    = 1806 // white king on the files a-d
)

type kkTable struct {
	index   [chess.FieldCount][chess.FieldCount]int16 // [white king][black king] -> dense index, -1 illegal
	xform   [chess.FieldCount][chess.FieldCount]uint8 // transform that produces the canonical pair
	squares [][2]chess.Pos                            // dense index -> canonical (white king, black king)
	// diagonal marks pairs with both kings on the a1-h8 diagonal: the kings do
	// not fix the transposition, the first other piece off the diagonal does
	// (it is moved below). Without that rule a position and its transposed
	// twin would get two indices whose children coincide, and the un-moves
	// from a child would reach only one of the twins.
	diagonal []bool
}

var kkPawnless, kkPawns kkTable

func kingsAdjacent(a, b chess.Pos) bool {
	dx, dy := a.X()-b.X(), a.Y()-b.Y()
	return dx >= -1 && dx <= 1 && dy >= -1 && dy <= 1
}

// inTriangle reports whether sq is one of the ten squares a1, b1, b2, c1, c2,
// c3, d1, d2, d3, d4 (file <= 3, rank <= file).
func inTriangle(sq chess.Pos) bool {
	f, r := sq.X(), chess.Height-1-sq.Y()
	return f <= 3 && r <= f
}

func belowOrOnDiagonal(sq chess.Pos) bool {
	f, r := sq.X(), chess.Height-1-sq.Y()
	return r <= f
}

func onDiagonal(sq chess.Pos) bool { return sq.X() == chess.Height-1-sq.Y() }

// buildKK fills a king pair table. canonical reports whether (wk, bk) is in
// canonical form; the transform with the lowest id that makes a pair canonical
// is chosen, so a pair that already is canonical keeps the identity.
func buildKK(t *kkTable, transforms int, canonical func(wk, bk chess.Pos) bool) {
	for wk := chess.Pos(0); wk < chess.FieldCount; wk++ {
		for bk := chess.Pos(0); bk < chess.FieldCount; bk++ {
			t.index[wk][bk] = -1
			if wk == bk || kingsAdjacent(wk, bk) {
				continue
			}
			for tf := 0; tf < transforms; tf++ {
				w, b := xform[tf][wk], xform[tf][bk]
				if !canonical(w, b) {
					continue
				}
				t.xform[wk][bk] = uint8(tf)
				if tf == 0 {
					t.index[wk][bk] = int16(len(t.squares))
					t.squares = append(t.squares, [2]chess.Pos{wk, bk})
					t.diagonal = append(t.diagonal, transforms == 8 && onDiagonal(wk) && onDiagonal(bk))
				}
				break
			}
		}
	}
	// second pass: non-canonical pairs point to the index of their canonical form
	for wk := chess.Pos(0); wk < chess.FieldCount; wk++ {
		for bk := chess.Pos(0); bk < chess.FieldCount; bk++ {
			if wk == bk || kingsAdjacent(wk, bk) || t.index[wk][bk] >= 0 {
				continue
			}
			tf := t.xform[wk][bk]
			t.index[wk][bk] = t.index[xform[tf][wk]][xform[tf][bk]]
		}
	}
}

func init() {
	for t := 0; t < 8; t++ {
		for sq := chess.Pos(0); sq < chess.FieldCount; sq++ {
			xform[t][sq] = transformSquare(t, sq)
		}
	}
	buildKK(&kkPawnless, 8, func(wk, bk chess.Pos) bool {
		if !inTriangle(wk) {
			return false
		}
		if onDiagonal(wk) { // white king on the diagonal: the black king decides
			return belowOrOnDiagonal(bk)
		}
		return true
	})
	buildKK(&kkPawns, 2, func(wk, bk chess.Pos) bool { return wk.X() <= 3 })
	if len(kkPawnless.squares) != KKPawnless || len(kkPawns.squares) != KKPawns {
		panic(fmt.Sprintf("egtb: king pairs %d/%d, expected %d/%d", len(kkPawnless.squares), len(kkPawns.squares), KKPawnless, KKPawns))
	}
}

// Pawns live on the ranks 2 to 7: 48 squares, index = square - 8.
const (
	pawnSquares  = 48
	pieceSquares = chess.FieldCount
	pawnOffset   = chess.Width
)
