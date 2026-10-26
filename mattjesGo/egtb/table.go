package egtb

import (
	"fmt"
	"math"
	"math/bits"
	"sync"
	"unsafe"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// Value is the table entry of a position from the point of view of the side
// to move, as a move count: 0 draw, 1..0x7fff mates in 2v-1 plies, 0x8000..
// gets mated in 2(v-0x8000) plies (0x8000 = is mated right now). A win always
// takes an odd number of plies and a loss an even one, so storing the move
// count instead of the ply count doubles the range for free. A table stores
// its values in one byte (ToByte: wins up to 253 plies, losses up to 254,
// which covers every five-piece ending, KPPKP needs a loss in 254) or, when
// the generator reaches a longer distance, in two bytes (the table is widened
// in place, Table.widen; wins up to 65,533 plies). Dead and illegal indices
// have no value of their own: the generator keeps them in a bitset, stores 0
// and fills them with their predecessor before compression; they are never
// looked up.
type Value uint16

const (
	Draw     Value = 0
	lossBase       = 0x8000
	// byteLossBase is the loss base of the one-byte form (format v3).
	byteLossBase = 128
	// MaxPlies is the longest distance a one-byte table can express (wins 253, losses 254).
	MaxPlies = 254
	// MaxPliesWide is the longest distance a two-byte table can express (wins 65,533, losses 65,534).
	MaxPliesWide = 65534
)

// WinIn encodes a win in plies (odd); LossIn a loss in plies (even).
func WinIn(plies int) Value  { return Value((plies + 1) / 2) }
func LossIn(plies int) Value { return Value(lossBase + plies/2) }

// ValueOfByte is the value of a one-byte entry.
func ValueOfByte(b byte) Value {
	v := Value(b)
	return v + (v&0x80)*0xff // 128.. moves up to 0x8000..
}

// ToByte is the one-byte entry of a value that fits (FitsByte).
func (v Value) ToByte() byte {
	if v >= lossBase {
		return byte(byteLossBase + int(v) - lossBase)
	}
	return byte(v)
}

// FitsByte reports whether the value fits a one-byte table (at most 127 moves).
func (v Value) FitsByte() bool { return v.Moves() < byteLossBase }

// ToPlanes is the file form of a wide table's entry as two planes: the low
// byte is the one-byte form for every value that fits one (loss bit 0x80 plus
// the move count modulo 128), the high byte the move count divided by 128.
// Positions beyond 127 moves are rare, so the high plane is almost all zeros
// and packs to nearly nothing, and the low plane packs like a one-byte table.
func (v Value) ToPlanes() (lo, hi byte) {
	m := v.Moves()
	if v.IsLoss() {
		lo = byteLossBase
	}
	return lo | byte(m&0x7f), byte(m >> 7)
}

// ValueOfPlanes is the inverse of ToPlanes.
func ValueOfPlanes(lo, hi byte) Value {
	m := int(lo&0x7f) | int(hi)<<7
	if lo&byteLossBase != 0 {
		return ValueOfMoves(false, m)
	}
	return Value(m) // a win, or 0 for a draw
}

func (v Value) IsWin() bool  { return v >= 1 && v < lossBase }
func (v Value) IsLoss() bool { return v >= lossBase }

// Moves is the distance in moves: v for a win in 2v-1 plies, m for a loss in
// 2m plies, 0 for a draw.
func (v Value) Moves() int {
	if v.IsLoss() {
		return int(v) - lossBase
	}
	return int(v)
}

// ValueOfMoves is a win (or loss) in moves moves, the inverse of Moves.
func ValueOfMoves(win bool, moves int) Value {
	if win {
		return Value(moves)
	}
	return Value(lossBase + moves)
}

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

// Bounds is the interval of plies the true distance lies in when the value
// comes from a table with the given step (moves per bucket, see Quantize):
// an exact table (step 1) gives lo == hi, a quantised table the whole bucket.
// Draws and mates in 0 give 0, 0.
func (v Value) Bounds(step int) (lo, hi int) {
	if step <= 1 || v == Draw || v == lossBase {
		p := v.Plies()
		return p, p
	}
	if v.IsWin() {
		return 2*max(1, int(v)-step+1) - 1, 2*int(v) - 1
	}
	m := int(v) - lossBase
	return 2 * max(1, m-step+1), 2 * m
}

// Quantize rounds a value up to the next multiple of step moves ("mate in at
// most 8" instead of the exact count), the upper end of its bucket, at most
// 127 moves (the quantised files are one-byte tables). Draws and mates in 0
// stay; wins and losses stay apart.
func Quantize(v Value, step int) Value { return quantizeTo(v, step, byteLossBase-1) }

// quantizeTo is Quantize with the largest move count top.
func quantizeTo(v Value, step, top int) Value {
	if step <= 1 || v == Draw || v == lossBase {
		return v
	}
	return ValueOfMoves(v.IsWin(), min((v.Moves()+step-1)/step*step, top))
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
	Values    []byte // nil until generated or loaded as a whole (the base); a table beyond the base loads packed, see packed; one byte per entry (Value.ToByte) unless the table is wide
	// Wide holds the entries of a wide table (distances beyond MaxPlies),
	// Values is nil then; nil for every other table. The generator widens a
	// table in place when it reaches level MaxPlies+1 (widen).
	Wide []uint16
	// packed serves a table that stays compressed in its cache file (every
	// material beyond the base once loaded): a block cache behind value().
	// Values is nil then; the generator never uses such a table.
	packed *blockStore
	// Step is the resolution of the values in moves: 1 = exact distances,
	// n = every value is the upper end of a bucket of n moves (Quantize), so
	// a lookup only bounds the distance (Value.Bounds) and the search has to
	// resolve the rest. The generator always produces exact values.
	Step int
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
	t := &Table{Mat: m, slots: m.slots(), Step: 1}
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
	baseCount int     // the first baseCount tables are the four-piece base
	bySig     []int16 // signature -> table index, -1 none
	// CacheDir holds the cache files (BaseFileName and one file per larger
	// material); next to the binary unless changed.
	CacheDir string
	// missing counts the lookups of materials without a table (up to
	// MaxPieces pieces), by canonical signature: the demand a search creates
	// for tables that could be generated (MissingTables). Written by the
	// search thread, read by the main loop while the search runs, hence the
	// mutex (taken on misses only).
	missing   map[Signature]uint64
	missingMu sync.Mutex
	// WidenLevel is the first level the generator stores in two bytes per
	// entry: it widens a table before this level (MaxPlies+1, the first that
	// does not fit a byte; tests lower it to exercise the widening).
	WidenLevel int
}

// NewSet allocates the base tables without values; Generate or Load fills them.
func NewSet() *Set {
	s := &Set{bySig: make([]int16, signatureSpace), CacheDir: DefaultCacheDir(), missing: map[Signature]uint64{}, WidenLevel: MaxPlies + 1}
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
	n.WidenLevel = s.WidenLevel
	for _, t := range s.Tables {
		nt := n.AddMaterial(t.Mat)
		if t.Generated() {
			nt.Values = append([]byte(nil), t.Values...)
			nt.Wide = append([]uint16(nil), t.Wide...)
			nt.RawChecksum = t.RawChecksum
			nt.filled, nt.Step = t.filled, t.Step
		}
	}
	return n
}

// Adopt moves every generated table this set lacks over from another set
// (the result of a forked generation).
func (s *Set) Adopt(from *Set) {
	for _, ft := range from.Tables {
		if !ft.Generated() {
			continue
		}
		t := s.AddMaterial(ft.Mat)
		if !t.Generated() {
			t.Values, t.Wide, t.RawChecksum, t.filled, t.invalid, t.Step = ft.Values, ft.Wide, ft.RawChecksum, ft.filled, ft.invalid, ft.Step
			ft.Values, ft.Wide, ft.invalid = nil, nil, nil
		}
	}
}

// AddMaterial registers a table for a material beyond the base (up to
// MaxPieces), without values. Returns the existing table if already present.
func (s *Set) AddMaterial(m Material) *Table {
	if t := s.Find(m.Name()); t != nil {
		return t
	}
	if m.Pieces() > MaxPieces || len(s.Tables) >= math.MaxInt16 {
		panic("egtb: cannot add material " + m.Name())
	}
	t := newTable(m)
	s.bySig[m.signature()] = int16(len(s.Tables))
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
	_, v, ok = s.lookup(b)
	return v, ok
}

// LookupBounds is Lookup with the interval of plies the distance lies in
// (Value.Bounds with the table's Step): lo == hi for exact tables.
func (s *Set) LookupBounds(b *bitboard.Board) (v Value, lo, hi int, ok bool) {
	t, v, ok := s.lookup(b)
	if !ok {
		return v, 0, 0, false
	}
	step := 1
	if t != nil {
		step = t.Step
	}
	lo, hi = v.Bounds(step)
	return v, lo, hi, true
}

// lookup finds the value and its table (nil for king against king).
func (s *Set) lookup(b *bitboard.Board) (t *Table, v Value, ok bool) {
	occ := b.ByColor[0] | b.ByColor[1]
	if bits.OnesCount64(occ) == 2 && b.Castling == 0 {
		return nil, Draw, true
	}
	t, flip, ok := s.findTable(b)
	if !ok {
		return nil, Draw, false
	}
	if !t.available() { // registered but not generated: no index for nothing (a search asks millions of times), but counted as demand
		s.countMissing(t.Mat.signature())
		return nil, Draw, false
	}
	idx := t.indexOf(b, flip)
	if idx < 0 { // adjacent kings, never reached from a legal position
		return nil, Draw, false
	}
	return t, t.Value(idx), true
}

// Value reads one entry: from the array, or from the block cache of a
// packed table.
func (t *Table) Value(idx int) Value {
	if t.Values != nil {
		return ValueOfByte(t.Values[idx])
	}
	if t.Wide != nil {
		return Value(t.Wide[idx])
	}
	return t.packed.value(idx)
}

// SetValue stores one entry; a one-byte table only takes values that fit
// (Value.FitsByte), the generator widens the table before.
func (t *Table) SetValue(idx int, v Value) {
	if t.Values != nil {
		t.Values[idx] = v.ToByte()
	} else {
		t.Wide[idx] = uint16(v)
	}
}

// Generated reports whether the values are in memory as an array (generated,
// or the loaded base).
func (t *Table) Generated() bool { return t.Values != nil || t.Wide != nil }

// IsWide reports whether the entries take two bytes (distances beyond MaxPlies).
func (t *Table) IsWide() bool { return t.Wide != nil }

// allocate allocates the entries of a table (all draws), one byte or two each.
func (t *Table) allocate(wide bool) {
	t.Values, t.Wide = nil, nil
	if wide {
		t.Wide = make([]uint16, t.Size)
	} else {
		t.Values = make([]byte, t.Size)
	}
}

// widen turns a one-byte table into a two-byte table with the same values
// (the generator calls it between two levels, when the next level exceeds
// MaxPlies): for a moment both arrays exist, three bytes per entry.
func (t *Table) widen() {
	if t.Wide != nil || t.Values == nil {
		return
	}
	t.Wide = make([]uint16, len(t.Values))
	for i, b := range t.Values {
		t.Wide[i] = uint16(ValueOfByte(b))
	}
	t.Values = nil
}

// bytes is the content the checksum covers: the one-byte entries, or the
// two-byte entries little endian (as they lie in memory on every target).
func (t *Table) bytes() []byte {
	if t.Wide != nil {
		return unsafe.Slice((*byte)(unsafe.Pointer(unsafe.SliceData(t.Wide))), 2*len(t.Wide))
	}
	return t.Values
}

// available reports whether the table answers lookups: generated or loaded
// as a whole, or loaded packed.
func (t *Table) available() bool { return t.Generated() || t.packed != nil }

// Available is available for other packages (the UCI decides with it
// whether a table has to be generated).
func (t *Table) Available() bool { return t.available() }

// Packed reports whether the table stays compressed in its file.
func (t *Table) Packed() bool { return t.packed != nil }

// Quantize coarsens the values in memory to buckets of step moves (see
// Value.Bounds): what the cache files beyond the base hold
// (docs/m5-endgame-tables.md). Exact values cannot be recovered, so the
// table can no longer serve the generator as a dependency. A packed table is
// already quantised in its file.
func (t *Table) Quantize(step int) {
	if step <= 1 || !t.Generated() {
		return
	}
	for i, b := range t.Values {
		t.Values[i] = Quantize(ValueOfByte(b), step).ToByte()
	}
	for i, w := range t.Wide {
		t.Wide[i] = uint16(quantizeTo(Value(w), step, lossBase-1))
	}
	t.Step = step
}

// Exact reports whether the values are exact distances (Step 1).
func (t *Table) Exact() bool { return t.Step <= 1 }

// Locate finds the table and the index of a position with three or four
// pieces, no castling rights and no en passant square. idx is -1 for adjacent
// kings. ok is false when no table covers the position.
func (s *Set) Locate(b *bitboard.Board) (t *Table, idx int, ok bool) {
	t, flip, ok := s.findTable(b)
	if !ok {
		return nil, -1, false
	}
	return t, t.indexOf(b, flip), true
}

// findTable finds the table of a position's material (registered, generated
// or not) and whether the colors have to be swapped for it.
func (s *Set) findTable(b *bitboard.Board) (t *Table, flip bool, ok bool) {
	if b.Castling != 0 || b.EnPassant.Valid() {
		return nil, false, false
	}
	occ := b.ByColor[0] | b.ByColor[1]
	if n := bits.OnesCount64(occ); n > MaxPieces || n < 3 {
		return nil, false, false
	}
	sig, flipped := boardSignature(b)
	ti := s.bySig[sig]
	if ti < 0 {
		ti = s.bySig[flipped]
		flip = true
	}
	if ti < 0 {
		// a material without a table: counted, so that the demand of a
		// search can drive which table is generated next (MissingTables);
		// four of a kind (the sentinel signature) never has a table
		if sig != signatureSpace-1 {
			s.countMissing(min(sig, flipped))
		}
		return nil, false, false
	}
	return s.Tables[ti], flip, true
}

// indexOf is the index of a position in this table (-1 for adjacent kings).
func (t *Table) indexOf(b *bitboard.Board, flip bool) int {
	var sq [6]chess.Pos
	n := t.squares(b, flip, sq[:])
	return t.Index(b.WhiteMove != flip, sq[:n])
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
