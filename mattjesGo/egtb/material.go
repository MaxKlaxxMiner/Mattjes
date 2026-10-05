// Package egtb holds the own endgame tables of milestone 5 (working title,
// "endgame tablebase"): one byte per position for every material with up to
// four pieces, computed by retrograde analysis and kept in RAM. The design is
// explained in docs/m5-endgame-tables-design.md.
package egtb

import (
	"math/bits"
	"sort"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// Material is the set of pieces besides the two kings. By convention White is
// the stronger side (more pieces, then the higher piece), so KQKR exists and
// KRKQ is looked up with swapped colors. Types are sorted queen first.
type Material struct {
	White []chess.Piece // piece types without color
	Black []chess.Piece
}

// pieceOrder ranks the types for sorting and for the "stronger side" rule.
func pieceOrder(p chess.Piece) int {
	switch p.Type() {
	case chess.Queen:
		return 5
	case chess.Rook:
		return 4
	case chess.Bishop:
		return 3
	case chess.Knight:
		return 2
	case chess.Pawn:
		return 1
	}
	return 0
}

func sortTypes(ts []chess.Piece) {
	sort.Slice(ts, func(i, j int) bool { return pieceOrder(ts[i]) > pieceOrder(ts[j]) })
}

// compareSides returns >0 if side a is stronger than side b, 0 if equal.
func compareSides(a, b []chess.Piece) int {
	if len(a) != len(b) {
		return len(a) - len(b)
	}
	for i := range a {
		if d := pieceOrder(a[i]) - pieceOrder(b[i]); d != 0 {
			return d
		}
	}
	return 0
}

// Name is the usual notation, e.g. "KQKR" or "KBNK".
func (m Material) Name() string {
	s := "K"
	for _, p := range m.White {
		s += string(p.Char() &^ 0x20)
	}
	s += "K"
	for _, p := range m.Black {
		s += string(p.Char() &^ 0x20)
	}
	return s
}

// Pieces is the total number of pieces including the kings.
func (m Material) Pieces() int { return 2 + len(m.White) + len(m.Black) }

// Pawns counts the pawns of both sides.
func (m Material) Pawns() int {
	n := 0
	for _, p := range m.White {
		if p == chess.Pawn {
			n++
		}
	}
	for _, p := range m.Black {
		if p == chess.Pawn {
			n++
		}
	}
	return n
}

// slots lists the pieces besides the kings with their color, white first. The
// order of the slots is the order of the index digits.
func (m Material) slots() []chess.Piece {
	s := make([]chess.Piece, 0, len(m.White)+len(m.Black))
	for _, p := range m.White {
		s = append(s, chess.White|p)
	}
	for _, p := range m.Black {
		s = append(s, chess.Black|p)
	}
	return s
}

// All returns the 35 materials with up to four pieces in generation order:
// fewer pawns first, then fewer pieces. Every material a table depends on
// (captures lead to fewer pieces, promotions to fewer pawns) comes earlier.
// The order is also the layout of the cache file, so it must not change.
func All() []Material {
	types := []chess.Piece{chess.Queen, chess.Rook, chess.Bishop, chess.Knight, chess.Pawn}
	var all []Material
	for _, x := range types {
		all = append(all, Material{White: []chess.Piece{x}})
	}
	for i, x := range types {
		for _, y := range types[i:] {
			all = append(all, Material{White: []chess.Piece{x, y}})
		}
	}
	for i, x := range types {
		for _, y := range types[i:] {
			all = append(all, Material{White: []chess.Piece{x}, Black: []chess.Piece{y}})
		}
	}
	sort.SliceStable(all, func(i, j int) bool {
		if all[i].Pawns() != all[j].Pawns() {
			return all[i].Pawns() < all[j].Pawns()
		}
		return all[i].Pieces() < all[j].Pieces()
	})
	return all
}

// Signature identifies a material by piece counts: two bits per type and
// color, white in the low ten bits. A board's signature and the signature with
// swapped colors are both cheap to compute, which is all a lookup needs.
type Signature uint32

const signatureSpace = 1 << 20

func typeBit(p chess.Piece) uint {
	return 2 * uint(pieceOrder(p)-1) // pawn 0, knight 2, bishop 4, rook 6, queen 8
}

func (m Material) signature() Signature {
	var s Signature
	for _, p := range m.White {
		s += 1 << typeBit(p)
	}
	for _, p := range m.Black {
		s += 1 << (10 + typeBit(p))
	}
	return s
}

// boardSignature returns the signature of the board and the one with swapped
// colors. The kings are not counted.
func boardSignature(b *bitboard.Board) (Signature, Signature) {
	var w, bl Signature
	for _, p := range []chess.Piece{chess.Pawn, chess.Knight, chess.Bishop, chess.Rook, chess.Queen} {
		w += Signature(bits.OnesCount64(b.PieceBB(chess.White|p))) << typeBit(p)
		bl += Signature(bits.OnesCount64(b.PieceBB(chess.Black|p))) << typeBit(p)
	}
	return w | bl<<10, bl | w<<10
}
