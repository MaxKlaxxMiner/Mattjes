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

// Enumerate returns all materials with exactly pieces pieces (kings included)
// in dependency order: fewer pawns first (promotions lead to fewer pawns),
// then by name. Captures lead to fewer pieces, which are always available.
func Enumerate(pieces int) []Material {
	types := []chess.Piece{chess.Queen, chess.Rook, chess.Bishop, chess.Knight, chess.Pawn}
	var multisets func(k, start int, cur []chess.Piece, out *[][]chess.Piece)
	multisets = func(k, start int, cur []chess.Piece, out *[][]chess.Piece) {
		if len(cur) == k {
			*out = append(*out, append([]chess.Piece{}, cur...))
			return
		}
		for i := start; i < len(types); i++ {
			multisets(k, i, append(cur, types[i]), out)
		}
	}
	n := pieces - 2
	seen := map[string]bool{}
	var out []Material
	for k := 0; k <= n; k++ {
		var whites, blacks [][]chess.Piece
		multisets(k, 0, nil, &whites)
		multisets(n-k, 0, nil, &blacks)
		for _, w := range whites {
			for _, b := range blacks {
				m := canonicalMaterial(w, b)
				if !seen[m.Name()] {
					seen[m.Name()] = true
					out = append(out, m)
				}
			}
		}
	}
	sort.SliceStable(out, func(i, j int) bool {
		if out[i].Pawns() != out[j].Pawns() {
			return out[i].Pawns() < out[j].Pawns()
		}
		return out[i].Name() < out[j].Name()
	})
	return out
}

// Signature identifies a material by piece counts: two bits per type and
// color, white in the low ten bits. A board's signature and the signature with
// swapped colors are both cheap to compute, which is all a lookup needs.
type Signature uint32

const signatureSpace = 1 << 20

// MaterialOf reads the material of a board (kings excluded) in canonical form.
// ok is false when a side has no king or the material exceeds MaxPieces.
func MaterialOf(b *bitboard.Board) (m Material, ok bool) {
	var w, bl []chess.Piece
	kings := 0
	for _, p := range b.Squares {
		switch {
		case p == chess.None:
		case p.Is(chess.King):
			kings++
		case p.Color() == chess.White:
			w = append(w, p.Type())
		default:
			bl = append(bl, p.Type())
		}
	}
	if kings != 2 || 2+len(w)+len(bl) > MaxPieces {
		return Material{}, false
	}
	return canonicalMaterial(w, bl), true
}

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
