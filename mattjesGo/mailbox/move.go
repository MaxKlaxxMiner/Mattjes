package mailbox

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// Move is a compact 4-byte move. Castling is a king move of two squares,
// en passant is a diagonal pawn move with Capture == None.
type Move struct {
	From    chess.Pos
	To      chess.Pos
	Promo   chess.Piece // promotion piece (with color) or None
	Capture chess.Piece // captured piece or None (also None for en passant)
}

// MaxMoves is the capacity of a move buffer. No legal position has more than 218 moves.
const MaxMoves = 256

// MoveBuffer is a fixed-size, allocation-free move list.
type MoveBuffer [MaxMoves]Move

// UCI returns the move in UCI notation like "e2e4" or "e7e8q".
func (m Move) UCI() string {
	s := m.From.String() + m.To.String()
	if m.Promo != chess.None {
		s += string(m.Promo.Type().Char() | 0x20) // lowercase
	}
	return s
}

func (m Move) String() string { return m.UCI() }

// Moves returns all legal moves as a freshly allocated slice (convenience, not for hot loops).
func (b *Board) Moves() []Move {
	var buf MoveBuffer
	n := b.GenMoves(&buf)
	return append([]Move(nil), buf[:n]...)
}

// FindUCI looks up a legal move by its UCI string. Returns false if the move is not legal.
func (b *Board) FindUCI(uci string) (Move, bool) {
	for _, m := range b.Moves() {
		if m.UCI() == uci {
			return m, true
		}
	}
	return Move{}, false
}
