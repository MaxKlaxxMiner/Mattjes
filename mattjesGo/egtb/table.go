package egtb

import (
	"fmt"
	"math/bits"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// Value is the table entry of a position from the point of view of the side
// to move: 0 draw, 1..127 mates in n plies, 128 invalid position, 129..255
// gets mated in n-129 plies (129 = is mated right now).
type Value uint8

const (
	Draw     Value = 0
	Invalid  Value = 128
	lossBase       = 129
	// MaxPlies is the longest distance either side can express.
	MaxPlies = 126
)

func WinIn(plies int) Value  { return Value(plies) }
func LossIn(plies int) Value { return Value(lossBase + plies) }

func (v Value) IsWin() bool  { return v >= 1 && v < Invalid }
func (v Value) IsLoss() bool { return v > Invalid }

// Plies is the distance to mate for a win or a loss, 0 otherwise.
func (v Value) Plies() int {
	switch {
	case v.IsWin():
		return int(v)
	case v.IsLoss():
		return int(v) - lossBase
	}
	return 0
}

func (v Value) String() string {
	switch {
	case v == Draw:
		return "draw"
	case v == Invalid:
		return "invalid"
	case v.IsWin():
		return fmt.Sprintf("win in %d", v.Plies())
	}
	return fmt.Sprintf("loss in %d", v.Plies())
}

// Table is the endgame table of one material: a dense array indexed by the
// position. Squares are listed as white king, black king, then the slots of
// the material in order.
type Table struct {
	Mat       Material
	slots     []chess.Piece // colored pieces after the kings
	sizes     []int         // index digits: 64 per piece, 48 per pawn
	equalPrev []bool        // slot holds the same piece as the previous slot: squares are kept ascending
	kk        *kkTable
	Size      int
	Values    []Value // nil until generated or loaded
}

func newTable(m Material) *Table {
	t := &Table{Mat: m, slots: m.slots()}
	for i, p := range t.slots {
		t.equalPrev = append(t.equalPrev, i > 0 && t.slots[i-1] == p)
	}
	if m.Pawns() > 0 {
		t.kk = &kkPawns
		t.Size = 2 * KKPawns
	} else {
		t.kk = &kkPawnless
		t.Size = 2 * KKPawnless
	}
	for _, p := range t.slots {
		size := pieceSquares
		if p.Is(chess.Pawn) {
			size = pawnSquares
		}
		t.sizes = append(t.sizes, size)
		t.Size *= size
	}
	return t
}

// Index computes the position index; -1 if the kings are adjacent or equal.
// A square occupied twice still gives an index (marked Invalid in the table).
// Two equal pieces are ordered by square, so both orders give the same index;
// the indices with the other order are dead (Decode does not know that, the
// generator marks them Invalid).
func (t *Table) Index(whiteMove bool, sq []chess.Pos) int {
	kk := int(t.kk.index[sq[0]][sq[1]])
	if kk < 0 {
		return -1
	}
	tf := int(t.kk.xform[sq[0]][sq[1]])
	base := kk
	if !whiteMove {
		base += len(t.kk.squares)
	}
	idx := t.digits(base, tf, sq)
	if t.kk.diagonal[kk] {
		// both kings on the diagonal: the kings do not fix the transposition,
		// so the smaller of the two indices is the canonical one (independent
		// of the order in which equal pieces are listed)
		idx = min(idx, t.digits(base, tf|4, sq))
	}
	return idx
}

// digits appends the piece squares, transformed by tf, to the king pair index.
func (t *Table) digits(idx, tf int, sq []chess.Pos) int {
	var s [4]int
	for i := range t.sizes {
		s[i] = int(xform[tf][sq[2+i]])
		if t.equalPrev[i] && s[i] < s[i-1] {
			s[i], s[i-1] = s[i-1], s[i]
		}
	}
	for i, size := range t.sizes {
		d := s[i]
		if size == pawnSquares {
			d -= pawnOffset
		}
		idx = idx*size + d
	}
	return idx
}

// Decode is the inverse of Index: it fills sq (len 2 + slots) and returns the
// side to move. Decoded positions are always in canonical form, but an index
// with two equal pieces in descending square order is dead: Index(Decode(idx))
// then differs from idx, which is how the generator recognizes it.
func (t *Table) Decode(idx int, sq []chess.Pos) (whiteMove bool) {
	for i := len(t.sizes) - 1; i >= 0; i-- {
		size := t.sizes[i]
		s := idx % size
		idx /= size
		if size == pawnSquares {
			s += pawnOffset
		}
		sq[2+i] = chess.Pos(s)
	}
	kk := len(t.kk.squares)
	whiteMove = idx < kk
	if !whiteMove {
		idx -= kk
	}
	sq[0], sq[1] = t.kk.squares[idx][0], t.kk.squares[idx][1]
	return whiteMove
}

// Board builds the position of an index (a convenience for tests and output;
// the generator does the same inline).
func (t *Table) Board(idx int) bitboard.Board {
	sq := make([]chess.Pos, 2+len(t.slots))
	whiteMove := t.Decode(idx, sq)
	pieces := append([]chess.Piece{chess.WhiteKing, chess.BlackKing}, t.slots...)
	return bitboard.FromPieces(whiteMove, sq, pieces)
}

// Set is the complete collection of tables for all materials up to four pieces.
type Set struct {
	Tables []*Table
	bySig  []int8 // signature -> table index, -1 none
}

// NewSet allocates the tables without values; Generate or Load fills them.
func NewSet() *Set {
	s := &Set{bySig: make([]int8, signatureSpace)}
	for i := range s.bySig {
		s.bySig[i] = -1
	}
	for i, m := range All() {
		s.Tables = append(s.Tables, newTable(m))
		s.bySig[m.signature()] = int8(i)
	}
	return s
}

// Find returns the table of a material by name, e.g. "KBNK", or nil.
func (s *Set) Find(name string) *Table {
	for _, t := range s.Tables {
		if t.Mat.Name() == name {
			return t
		}
	}
	return nil
}

// TotalSize is the number of bytes of all tables.
func (s *Set) TotalSize() int {
	n := 0
	for _, t := range s.Tables {
		n += t.Size
	}
	return n
}

// Lookup returns the value of a position with up to four pieces, without
// castling rights and without an en passant square. ok is false when the
// position is not covered: more pieces, castling or en passant (the search
// plays one more ply) or a table that is not computed yet. King against king
// is a draw without a table.
func (s *Set) Lookup(b *bitboard.Board) (v Value, ok bool) {
	occ := b.ByColor[0] | b.ByColor[1]
	if bits.OnesCount64(occ) == 2 && b.Castling == 0 {
		return Draw, true
	}
	t, idx, ok := s.Locate(b)
	if !ok || t.Values == nil {
		return Draw, false
	}
	if idx < 0 {
		return Invalid, true
	}
	return t.Values[idx], true
}

// Locate finds the table and the index of a position with three or four
// pieces, no castling rights and no en passant square. idx is -1 for adjacent
// kings. ok is false when no table covers the position.
func (s *Set) Locate(b *bitboard.Board) (t *Table, idx int, ok bool) {
	if b.Castling != 0 || b.EnPassant.Valid() {
		return nil, -1, false
	}
	occ := b.ByColor[0] | b.ByColor[1]
	if n := bits.OnesCount64(occ); n > 4 || n < 3 {
		return nil, -1, false
	}
	sig, flipped := boardSignature(b)
	flip := false
	ti := s.bySig[sig]
	if ti < 0 {
		ti = s.bySig[flipped]
		flip = true
	}
	if ti < 0 {
		return nil, -1, false
	}
	t = s.Tables[ti]
	var sq [6]chess.Pos
	n := t.squares(b, flip, sq[:])
	return t, t.Index(b.WhiteMove != flip, sq[:n]), true
}

// squares extracts the squares of the board in slot order, with the colors
// swapped and the board mirrored vertically when flip is set.
func (t *Table) squares(b *bitboard.Board, flip bool, sq []chess.Pos) int {
	var m chess.Pos
	if flip {
		m = 56
	}
	us, them := 0, 1
	if flip {
		us, them = 1, 0
	}
	sq[0] = b.KingPos(us) ^ m
	sq[1] = b.KingPos(them) ^ m
	var taken uint64
	for i, p := range t.slots {
		if flip {
			p ^= chess.ColorMask
		}
		bb := b.PieceBB(p) &^ taken
		s := chess.Pos(bits.TrailingZeros64(bb))
		taken |= 1 << uint(s)
		sq[2+i] = s ^ m
	}
	return 2 + len(t.slots)
}
