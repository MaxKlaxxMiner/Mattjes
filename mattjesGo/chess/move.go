package chess

// Castling rights as a bit set.
type Castling uint8

const (
	WhiteKingside Castling = 1 << iota
	WhiteQueenside
	BlackKingside
	BlackQueenside
	AllCastling = WhiteKingside | WhiteQueenside | BlackKingside | BlackQueenside
)

// Move is a compact 4-byte move shared by all generators. Castling is a king
// move of two squares, en passant is a diagonal pawn move with Capture == None.
type Move struct {
	From    Pos
	To      Pos
	Promo   Piece // promotion piece (with color) or None
	Capture Piece // captured piece or None (also None for en passant)
}

// MaxMoves is the capacity of a move buffer. No legal position has more than 218 moves.
const MaxMoves = 256

// MoveBuffer is a fixed-size, allocation-free move list.
type MoveBuffer [MaxMoves]Move

// UCI returns the move in UCI notation like "e2e4" or "e7e8q".
func (m Move) UCI() string {
	s := m.From.String() + m.To.String()
	if m.Promo != None {
		s += string(m.Promo.Type().Char() | 0x20) // lowercase
	}
	return s
}

func (m Move) String() string { return m.UCI() }
