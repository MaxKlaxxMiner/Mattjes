package bitboard

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// DoMove plays a legal move. Save State() beforehand if you want to UndoMove.
func (b *Board) DoMove(m chess.Move) {
	us := b.us()
	them := us ^ 1
	p := b.Squares[m.From]

	if m.Capture != chess.None {
		b.remove(m.To)
	}
	b.remove(m.From)
	if m.Promo != chess.None {
		b.put(m.To, m.Promo)
	} else {
		b.put(m.To, p)
	}

	newEnPassant := chess.NoPos
	if p.Is(chess.Pawn) {
		if m.To == b.EnPassant && m.From.X() != m.To.X() { // en passant: remove the pawn behind the target square
			if us == 0 {
				b.remove(m.To + chess.Width)
			} else {
				b.remove(m.To - chess.Width)
			}
		} else if d := m.To - m.From; d == 2*chess.Width || d == -2*chess.Width { // double push
			// Only remember the square if the opponent can legally capture en passant
			// (pinned pawns excluded). This is the canonical rule used for counting
			// distinct positions (OEIS A083276) and keeps equal positions equal for hashing.
			ep := (m.From + m.To) / 2
			if b.hasLegalEnPassant(ep, them) {
				newEnPassant = ep
			}
		}
	} else if p.Is(chess.King) {
		switch m.To - m.From {
		case 2: // kingside: rook from the corner to the square the king passed
			rook := b.Squares[m.To+1]
			b.remove(m.To + 1)
			b.put(m.To-1, rook)
		case -2: // queenside
			rook := b.Squares[m.To-2]
			b.remove(m.To - 2)
			b.put(m.To+1, rook)
		}
	}
	oldCastling, oldEnPassant := b.Castling, b.EnPassant
	b.EnPassant = newEnPassant
	b.Castling &^= castleClear[m.From] | castleClear[m.To]
	b.xorState(oldCastling, oldEnPassant)

	if p.Is(chess.Pawn) || m.Capture != chess.None {
		b.HalfmoveClock = 0
	} else {
		b.HalfmoveClock++
	}
	if us == 1 {
		b.MoveNumber++
	}
	b.WhiteMove = !b.WhiteMove
}

// UndoMove takes back the last move m; s must be the State() from before DoMove.
func (b *Board) UndoMove(m chess.Move, s State) {
	b.WhiteMove = !b.WhiteMove
	us := b.us() // the side that made the move
	them := us ^ 1
	if us == 1 {
		b.MoveNumber--
	}

	p := b.Squares[m.To]
	b.remove(m.To)
	if m.Promo != chess.None {
		p = colorPiece[us] | chess.Pawn
	}
	b.put(m.From, p)
	if m.Capture != chess.None {
		b.put(m.To, m.Capture)
	}

	if p.Is(chess.Pawn) {
		if m.Capture == chess.None && m.From.X() != m.To.X() { // en passant: put the captured pawn back
			if us == 0 {
				b.put(m.To+chess.Width, colorPiece[them]|chess.Pawn)
			} else {
				b.put(m.To-chess.Width, colorPiece[them]|chess.Pawn)
			}
		}
	} else if p.Is(chess.King) {
		switch m.To - m.From {
		case 2:
			rook := b.Squares[m.To-1]
			b.remove(m.To - 1)
			b.put(m.To+1, rook)
		case -2:
			rook := b.Squares[m.To+1]
			b.remove(m.To + 1)
			b.put(m.To-2, rook)
		}
	}

	b.xorState(chess.Castling(s>>8)&chess.AllCastling, chess.Pos(int8(uint8(s))))
	b.restoreState(s)
}
