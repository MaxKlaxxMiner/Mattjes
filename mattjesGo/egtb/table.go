package egtb

import (
	"fmt"
	"math/bits"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// Value is the table entry of a position from the point of view of the side
// to move: 0 draw, 1..127 mates in 2v-1 plies, 128..255 gets mated in
// 2(v-128) plies (128 = is mated right now). A win always takes an odd number
// of plies and a loss an even one, so storing the move count instead of the
// ply count doubles the range for free: wins up to 253 plies, losses up to
// 254, which covers every five-piece ending (KPPKP needs a loss in 254). Dead
// and illegal indices have no value of their own: the generator keeps them in
// a bitset, stores 0 and fills them with their predecessor before
// compression; they are never looked up.
type Value uint8

const (
	Draw     Value = 0
	lossBase       = 128
	// MaxPlies is the longest distance a table can express (wins 253, losses 254).
	MaxPlies = 254
)

// WinIn encodes a win in plies (odd); LossIn a loss in plies (even).
func WinIn(plies int) Value  { return Value((plies + 1) / 2) }
func LossIn(plies int) Value { return Value(lossBase + plies/2) }

func (v Value) IsWin() bool  { return v >= 1 && v < lossBase }
func (v Value) IsLoss() bool { return v >= lossBase }

// Plies is the distance to mate for a win or a loss, 0 otherwise.
func (v Value) Plies() int {
	switch {
	case v.IsWin():
		return 2*int(v) - 1
	case v.IsLoss():
		return 2 * (int(v) - lossBase)
	}
	return 0
}

func (v Value) String() string {
	switch {
	case v == Draw:
		return "draw"
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
	// RawChecksum is the checksum of the values as generated (dead and illegal
	// entries as 0); set after generation or read from the cache file header.
	// Checksum constants in the code refer to this.
	RawChecksum uint64
	// invalid marks the dead and illegal indices after generation (nil for a
	// loaded table); filled: those entries were overwritten with their
	// predecessor for compression ("don't care"), Values no longer hash to
	// RawChecksum.
	invalid bitset
	filled  bool
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
		// keep every run of equal pieces sorted by square (insertion sort: a
		// single neighbour swap is a full sort for pairs, but not for triples)
		for j := i; j > 0 && t.equalPrev[j] && s[j] < s[j-1]; j-- {
			s[j], s[j-1] = s[j-1], s[j]
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

// MaxPieces is the largest material a table can hold (two kings + four pieces:
// the square arrays and index digits are sized for it).
const MaxPieces = 6

// Set is the collection of tables: the base of all materials up to four
// pieces (always complete, one cache file) plus any larger materials added on
// demand (one file each).
type Set struct {
	Tables    []*Table
	baseCount int    // the first baseCount tables are the four-piece base
	bySig     []int8 // signature -> table index, -1 none
	// CacheDir holds the cache files (BaseFileName and one file per larger
	// material); next to the binary unless changed.
	CacheDir string
}

// NewSet allocates the base tables without values; Generate or Load fills them.
func NewSet() *Set {
	s := &Set{bySig: make([]int8, signatureSpace), CacheDir: DefaultCacheDir()}
	for i := range s.bySig {
		s.bySig[i] = -1
	}
	for _, m := range All() {
		s.AddMaterial(m)
	}
	s.baseCount = len(s.Tables)
	return s
}

// Base returns the four-piece tables (the content of the base cache file).
func (s *Set) Base() []*Table { return s.Tables[:s.baseCount] }

// Fork returns a new set with copies of every generated table, so that a
// background generation can work on it while this set stays untouched (the
// copies cost the size of the tables once, 173 MB for the base).
func (s *Set) Fork() *Set {
	n := NewSet()
	n.CacheDir = s.CacheDir
	for _, t := range s.Tables {
		nt := n.AddMaterial(t.Mat)
		if t.Values != nil {
			nt.Values = append([]Value(nil), t.Values...)
			nt.RawChecksum = t.RawChecksum
			nt.filled = t.filled
		}
	}
	return n
}

// Adopt moves every generated table this set lacks over from another set
// (the result of a forked generation).
func (s *Set) Adopt(from *Set) {
	for _, ft := range from.Tables {
		if ft.Values == nil {
			continue
		}
		t := s.AddMaterial(ft.Mat)
		if t.Values == nil {
			t.Values, t.RawChecksum, t.filled, t.invalid = ft.Values, ft.RawChecksum, ft.filled, ft.invalid
			ft.Values, ft.invalid = nil, nil
		}
	}
}

// GenerationBytes estimates the RAM a generation of t needs: the table, four
// bitsets of 1/8 each and the direct dependencies that are not loaded yet.
func (s *Set) GenerationBytes(t *Table) int {
	n := t.Size + t.Size/2
	for _, d := range t.Mat.dependencies() {
		if dt := s.Find(d.Name()); dt == nil || dt.Values == nil {
			n += newTable(d).Size
		}
	}
	return n
}

// AddMaterial registers a table for a material beyond the base (up to
// MaxPieces), without values. Returns the existing table if already present.
func (s *Set) AddMaterial(m Material) *Table {
	if t := s.Find(m.Name()); t != nil {
		return t
	}
	if m.Pieces() > MaxPieces || len(s.Tables) >= 127 {
		panic("egtb: cannot add material " + m.Name())
	}
	t := newTable(m)
	s.bySig[m.signature()] = int8(len(s.Tables))
	s.Tables = append(s.Tables, t)
	return t
}

// Parse reads a material name like "KQKBN" (white pieces, then black pieces,
// each side led by its king) into canonical form.
func Parse(name string) (Material, error) {
	var sides [2][]chess.Piece
	side := -1
	for i := 0; i < len(name); i++ {
		c := name[i]
		if c == 'K' {
			side++
			if side > 1 {
				return Material{}, fmt.Errorf("egtb: %q has more than two kings", name)
			}
			continue
		}
		p := chess.PieceFromChar(c).Type()
		if side < 0 || p == chess.None || p == chess.King {
			return Material{}, fmt.Errorf("egtb: bad material %q", name)
		}
		sides[side] = append(sides[side], p)
	}
	if side != 1 {
		return Material{}, fmt.Errorf("egtb: %q needs two kings", name)
	}
	m := canonicalMaterial(sides[0], sides[1])
	if m.Pieces() > MaxPieces {
		return Material{}, fmt.Errorf("egtb: %q has more than %d pieces", name, MaxPieces)
	}
	return m, nil
}

// RawPositions estimates the positions of a pawnless material without any
// symmetry: 3612 legal king pairs, the other pieces on the remaining squares,
// both sides to move (before removing positions with the opponent in check).
func (m Material) RawPositions() uint64 {
	n := uint64(3612) * 2
	free := uint64(62)
	for range m.White {
		n *= free
		free--
	}
	for range m.Black {
		n *= free
		free--
	}
	return n
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
	if !ok || t.Values == nil || idx < 0 { // idx < 0: adjacent kings, never reached from a legal position
		return Draw, false
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
	if n := bits.OnesCount64(occ); n > MaxPieces || n < 3 {
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
