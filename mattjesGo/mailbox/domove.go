package mailbox

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// DoMove plays a legal move. Save State() beforehand if you want to UndoMove.
func (b *Board) DoMove(m Move) {
	us := b.SideToMove()
	them := chess.Opponent(us)
	p := b.Fields[m.From]

	b.Fields[m.To] = p
	b.Fields[m.From] = chess.None
	if m.Promo != chess.None {
		b.Fields[m.To] = m.Promo
	}

	newEnPassant := chess.NoPos
	if p.Is(chess.Pawn) {
		if m.To == b.EnPassant && m.From.X() != m.To.X() { // en passant capture: remove the pawn behind the target square
			if us == chess.White {
				b.Fields[m.To+chess.Width] = chess.None
			} else {
				b.Fields[m.To-chess.Width] = chess.None
			}
		} else if d := m.To - m.From; d == 2*chess.Width || d == -2*chess.Width { // double push
			// Only remember the en passant square if an enemy pawn can actually use it.
			// This keeps equal positions equal, which matters for hashing later.
			if b.pawnCanCaptureEnPassant(m.To) {
				newEnPassant = (m.From + m.To) / 2
			}
		}
	} else if p.Is(chess.King) {
		if us == chess.White {
			b.WhiteKing = m.To
		} else {
			b.BlackKing = m.To
		}
		switch m.To - m.From {
		case 2: // kingside: rook jumps from the corner to the square the king passed
			b.Fields[m.To-1] = b.Fields[m.To+1]
			b.Fields[m.To+1] = chess.None
		case -2: // queenside
			b.Fields[m.To+1] = b.Fields[m.To-2]
			b.Fields[m.To-2] = chess.None
		}
	}
	b.EnPassant = newEnPassant
	b.Castling &^= castleClear[m.From] | castleClear[m.To]

	if p.Is(chess.Pawn) || m.Capture != chess.None {
		b.HalfmoveClock = 0
	} else {
		b.HalfmoveClock++
	}
	if us == chess.Black {
		b.MoveNumber++
	}
	b.WhiteMove = !b.WhiteMove
	_ = them
}

// UndoMove takes back the last move m; s must be the State() from before DoMove.
func (b *Board) UndoMove(m Move, s State) {
	b.WhiteMove = !b.WhiteMove
	us := b.SideToMove() // the side that made the move
	them := chess.Opponent(us)
	if us == chess.Black {
		b.MoveNumber--
	}

	p := b.Fields[m.To]
	if m.Promo != chess.None {
		p = us | chess.Pawn
	}
	b.Fields[m.From] = p
	b.Fields[m.To] = m.Capture

	if p.Is(chess.Pawn) {
		if m.Capture == chess.None && m.From.X() != m.To.X() { // en passant: put the captured pawn back
			if us == chess.White {
				b.Fields[m.To+chess.Width] = them | chess.Pawn
			} else {
				b.Fields[m.To-chess.Width] = them | chess.Pawn
			}
		}
	} else if p.Is(chess.King) {
		if us == chess.White {
			b.WhiteKing = m.From
		} else {
			b.BlackKing = m.From
		}
		switch m.To - m.From {
		case 2:
			b.Fields[m.To+1] = b.Fields[m.To-1]
			b.Fields[m.To-1] = chess.None
		case -2:
			b.Fields[m.To-2] = b.Fields[m.To+1]
			b.Fields[m.To+1] = chess.None
		}
	}

	b.restoreState(s)
}

// pawnCanCaptureEnPassant reports whether an enemy pawn stands directly left or right
// of pawnSq, i.e. whether a double push to pawnSq creates a usable en passant square.
func (b *Board) pawnCanCaptureEnPassant(pawnSq chess.Pos) bool {
	enemyPawn := chess.Opponent(b.Fields[pawnSq].Color()) | chess.Pawn
	return (pawnSq.X() > 0 && b.Fields[pawnSq-1] == enemyPawn) ||
		(pawnSq.X() < chess.Width-1 && b.Fields[pawnSq+1] == enemyPawn)
}
