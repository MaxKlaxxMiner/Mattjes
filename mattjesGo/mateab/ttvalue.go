package mateab

import (
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/tt"
)

// Table is what the search needs from a transposition table; tt.Table and
// tt.Buckets both qualify. A nil Table means no table.
type Table interface {
	Probe(k tt.Key) (uint64, bool)
	Store(k tt.Key, value uint64)
}

// TT value layout, 21 bits, so it fits the 22 value bits of 256 MB buckets:
//
//	bits 20..19  type: 1 = no mate / escape within depth, 2 = mate / mated in depth
//	bits 18..12  depth in plies (7 bits, up to 127; mate in 39 is 77 plies)
//	bits 11..6   move from
//	bits  5..0   move to
//
// The type sits in the high bits, so bucket replacement evicts "no mate" entries
// before proven mates and shallow entries before deep ones. The move carries no
// promotion piece (queen is assumed when matching); it only orders moves, so a
// lost underpromotion costs ordering, never correctness.
//
// Semantics per node kind:
//   - attacker, type 2: mate in depth plies, exact, valid for any remaining depth >= depth
//   - attacker, type 1: no mate within depth plies, valid for any remaining depth <= depth
//   - defender, type 2: mated in depth plies (every move loses), move = the longest defence
//   - defender, type 1: an escape exists within depth plies, move = the escape
//
// Unlike perftTT the key carries no depth salt: a proven mate does not depend
// on the search depth.
const (
	ttNoMate = 1
	ttMate   = 2
)

// ValueBits is the width of a packed value; a table must offer at least that.
const ValueBits = 21

func packValue(typ, depth int, m chess.Move) uint64 {
	return uint64(typ)<<19 | uint64(depth)<<12 | uint64(m.From)<<6 | uint64(m.To)
}

func unpackValue(v uint64) (typ, depth int, from, to chess.Pos) {
	return int(v >> 19), int(v>>12) & 127, chess.Pos(v>>6) & 63, chess.Pos(v) & 63
}

// sameMove matches a generated move against the from/to pair of a TT entry;
// among promotions only the queen matches.
func sameMove(m chess.Move, from, to chess.Pos) bool {
	return m.From == from && m.To == to && (m.Promo == chess.None || m.Promo&chess.Queen != 0)
}
