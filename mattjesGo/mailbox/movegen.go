package mailbox

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// GenMoves writes all legal moves of the side to move into buf and returns their count.
//
// Like yacboard, moves are generated pseudo-legally and each candidate is
// verified by making it on the board, testing whether the own king is attacked
// and taking it back (see isLegal). Castling is validated separately.
func (b *Board) GenMoves(buf *MoveBuffer) int {
	us := b.SideToMove()
	them := chess.Opponent(us)
	n := 0

	// pawn geometry depends on color
	fwd := chess.Pos(-chess.Width) // white pawns move towards rank 8 = smaller index
	startY, promoY := 6, 1
	capL, capR := dirNW, dirNE
	if us == chess.Black {
		fwd = +chess.Width
		startY, promoY = 1, 6
		capL, capR = dirSW, dirSE
	}

	for sq := chess.Pos(0); sq < chess.FieldCount; sq++ {
		p := b.Fields[sq]
		if p&us == 0 {
			continue
		}
		dist := &edgeDist[sq]

		switch p.Type() {
		case chess.Pawn:
			y := sq.Y()
			// single and double push
			if to := sq + fwd; b.Fields[to] == chess.None {
				m := Move{From: sq, To: to}
				if y == promoY {
					n = b.addPromotions(buf, n, m, us)
				} else {
					if b.isLegal(m) {
						buf[n] = m
						n++
					}
					if to2 := to + fwd; y == startY && b.Fields[to2] == chess.None {
						m.To = to2
						if b.isLegal(m) {
							buf[n] = m
							n++
						}
					}
				}
			}
			// captures and en passant
			for _, d := range [2]int{capL, capR} {
				if dist[d] == 0 {
					continue
				}
				to := sq + dirDelta[d]
				target := b.Fields[to]
				if target&them != 0 {
					m := Move{From: sq, To: to, Capture: target}
					if y == promoY {
						n = b.addPromotions(buf, n, m, us)
					} else if b.isLegal(m) {
						buf[n] = m
						n++
					}
				} else if to == b.EnPassant {
					m := Move{From: sq, To: to}
					if b.isLegal(m) {
						buf[n] = m
						n++
					}
				}
			}

		case chess.Knight:
			for _, to := range knightTargets[sq] {
				if target := b.Fields[to]; target&us == 0 {
					m := Move{From: sq, To: to, Capture: target}
					if b.isLegal(m) {
						buf[n] = m
						n++
					}
				}
			}

		case chess.King:
			for _, to := range kingTargets[sq] {
				if target := b.Fields[to]; target&us == 0 {
					m := Move{From: sq, To: to, Capture: target}
					if b.isLegal(m) {
						buf[n] = m
						n++
					}
				}
			}
			n = b.addCastling(buf, n, us, them)

		default: // sliders
			first, last := dirN, dirSE
			if p.Type() == chess.Rook {
				last = dirE
			} else if p.Type() == chess.Bishop {
				first = dirNW
			}
			for d := first; d <= last; d++ {
				to := sq
				for steps := dist[d]; steps > 0; steps-- {
					to += dirDelta[d]
					target := b.Fields[to]
					if target&us != 0 {
						break
					}
					m := Move{From: sq, To: to, Capture: target}
					if b.isLegal(m) {
						buf[n] = m
						n++
					}
					if target != chess.None {
						break
					}
				}
			}
		}
	}
	return n
}

// addPromotions adds the four promotion variants of m. Legality is identical for all of them.
func (b *Board) addPromotions(buf *MoveBuffer, n int, m Move, us chess.Piece) int {
	if !b.isLegal(m) {
		return n
	}
	for _, pt := range [4]chess.Piece{chess.Queen, chess.Rook, chess.Bishop, chess.Knight} {
		m.Promo = us | pt
		buf[n] = m
		n++
	}
	return n
}

// addCastling adds legal castling moves. The king may not be in check, pass through
// or land on an attacked square, and the squares between king and rook must be empty.
func (b *Board) addCastling(buf *MoveBuffer, n int, us, them chess.Piece) int {
	kingside, queenside := WhiteKingside, WhiteQueenside
	king := chess.Pos(60)
	if us == chess.Black {
		kingside, queenside = BlackKingside, BlackQueenside
		king = 4
	}
	if b.Castling&(kingside|queenside) == 0 || b.IsAttacked(king, them) {
		return n
	}
	if b.Castling&kingside != 0 &&
		b.Fields[king+1] == chess.None && b.Fields[king+2] == chess.None &&
		!b.IsAttacked(king+1, them) && !b.IsAttacked(king+2, them) {
		buf[n] = Move{From: king, To: king + 2}
		n++
	}
	if b.Castling&queenside != 0 &&
		b.Fields[king-1] == chess.None && b.Fields[king-2] == chess.None && b.Fields[king-3] == chess.None &&
		!b.IsAttacked(king-1, them) && !b.IsAttacked(king-2, them) {
		buf[n] = Move{From: king, To: king - 2}
		n++
	}
	return n
}

// isLegal makes the pseudo-legal move m, checks whether the own king is attacked
// and takes the move back. Promotion piece and castling are irrelevant here.
func (b *Board) isLegal(m Move) bool {
	us := b.SideToMove()
	them := chess.Opponent(us)
	p := b.Fields[m.From]

	b.Fields[m.To] = p
	b.Fields[m.From] = chess.None

	epCaptured := chess.NoPos
	if p.Is(chess.Pawn) && m.To == b.EnPassant && m.From.X() != m.To.X() {
		if us == chess.White {
			epCaptured = m.To + chess.Width
		} else {
			epCaptured = m.To - chess.Width
		}
		b.Fields[epCaptured] = chess.None
	}

	kingSq := b.KingPos(us)
	if p.Is(chess.King) {
		kingSq = m.To
	}
	attacked := b.IsAttacked(kingSq, them)

	b.Fields[m.From] = p
	b.Fields[m.To] = m.Capture
	if epCaptured != chess.NoPos {
		b.Fields[epCaptured] = them | chess.Pawn
	}
	return !attacked
}
