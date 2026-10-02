package mailbox

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// Move and MoveBuffer are shared by all generators, see package chess.
type Move = chess.Move
type MoveBuffer = chess.MoveBuffer

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
