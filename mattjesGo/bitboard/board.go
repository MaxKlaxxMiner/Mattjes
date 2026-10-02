package bitboard

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// Board is a complete position. Pieces holds one bit set per color and kind,
// ByColor the union per color, and Squares the piece per square for O(1)
// lookups (what was captured?). All three are kept in sync by put/remove.
type Board struct {
	Pieces        [2][kindCount]uint64
	ByColor       [2]uint64
	Squares       [chess.FieldCount]chess.Piece
	EnPassant     chess.Pos
	Castling      chess.Castling
	WhiteMove     bool
	HalfmoveClock uint16
	MoveNumber    uint16
	Key           Key // incremental Zobrist key, see zobrist.go
}

// State is the irreversible part of a position for UndoMove.
type State uint32

func (b *Board) State() State {
	return State(uint8(b.EnPassant)) | State(b.Castling)<<8 | State(b.HalfmoveClock)<<16
}

func (b *Board) restoreState(s State) {
	b.EnPassant = chess.Pos(int8(uint8(s)))
	b.Castling = chess.Castling(s>>8) & chess.AllCastling
	b.HalfmoveClock = uint16(s >> 16)
}

// New returns the start position.
func New() Board {
	b, _ := FromFEN(chess.StartFEN)
	return b
}

// FromFEN parses a FEN string via chess.ParseFEN.
func FromFEN(fen string) (Board, error) {
	s, err := chess.ParseFEN(fen)
	if err != nil {
		return Board{}, err
	}
	return FromSetup(&s), nil
}

// FromSetup builds a board from a validated setup.
func FromSetup(s *chess.Setup) Board {
	b := Board{EnPassant: s.EnPassant, Castling: s.Castling, WhiteMove: s.WhiteMove, HalfmoveClock: s.HalfmoveClock, MoveNumber: s.MoveNumber}
	for sq := chess.Pos(0); sq < chess.FieldCount; sq++ {
		if p := s.Squares[sq]; p != chess.None {
			b.put(sq, p)
		}
	}
	// canonical en passant: only if a legal capture exists (Setup only checks for an adjacent pawn)
	if b.EnPassant.Valid() && !b.hasLegalEnPassant(b.EnPassant, b.us()) {
		b.EnPassant = chess.NoPos
	}
	b.finishKey()
	return b
}

// Setup converts the board back to the representation-independent form.
func (b *Board) Setup() chess.Setup {
	return chess.Setup{
		Squares:       b.Squares,
		WhiteMove:     b.WhiteMove,
		Castling:      b.Castling,
		EnPassant:     b.EnPassant,
		HalfmoveClock: b.HalfmoveClock,
		MoveNumber:    b.MoveNumber,
	}
}

func (b *Board) FEN() string {
	s := b.Setup()
	return s.FEN()
}

func (b *Board) String() string {
	s := b.Setup()
	return s.String()
}

func (b *Board) put(sq chess.Pos, p chess.Piece) {
	bb := bit(sq)
	c := colorIdx(p)
	b.Pieces[c][kindIdx(p)] |= bb
	b.ByColor[c] |= bb
	b.Squares[sq] = p
	b.xorPiece(p, sq)
}

func (b *Board) remove(sq chess.Pos) {
	p := b.Squares[sq]
	bb := bit(sq)
	c := colorIdx(p)
	b.Pieces[c][kindIdx(p)] &^= bb
	b.ByColor[c] &^= bb
	b.Squares[sq] = chess.None
	b.xorPiece(p, sq)
}

// us returns the color index of the side to move.
func (b *Board) us() int {
	if b.WhiteMove {
		return 0
	}
	return 1
}

func (b *Board) occupied() uint64 { return b.ByColor[0] | b.ByColor[1] }

// KingPos returns the king square of the color index.
func (b *Board) KingPos(c int) chess.Pos { return lsb(b.Pieces[c][kKing]) }

// attackersTo returns all pieces of color index `by` attacking sq, given occupancy occ.
func (b *Board) attackersTo(sq chess.Pos, occ uint64, by int) uint64 {
	p := &b.Pieces[by]
	return (pawnAttacks[by^1][sq] & p[kPawn]) |
		(knightAttacks[sq] & p[kKnight]) |
		(kingAttacks[sq] & p[kKing]) |
		(bishopAttacks(sq, occ) & (p[kBishop] | p[kQueen])) |
		(rookAttacks(sq, occ) & (p[kRook] | p[kQueen]))
}

// InCheck reports whether the side to move is in check.
func (b *Board) InCheck() bool {
	us := b.us()
	return b.attackersTo(b.KingPos(us), b.occupied(), us^1) != 0
}

// Moves returns all legal moves as a freshly allocated slice (convenience, not for hot loops).
func (b *Board) Moves() []chess.Move {
	var buf chess.MoveBuffer
	n := b.GenMoves(&buf)
	return append([]chess.Move(nil), buf[:n]...)
}
