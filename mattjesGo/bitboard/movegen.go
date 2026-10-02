package bitboard

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// GenMoves writes all legal moves of the side to move into buf and returns their count.
//
// No move is ever made and taken back to test legality (the classic mailbox
// approach). Legality comes from three bit sets computed once per position:
//   - danger: every square the opponent attacks (with our king removed, so a
//     slider's attack continues "through" the king). King moves avoid it.
//   - checkers: opponent pieces attacking our king. Double check allows only
//     king moves; single check restricts other pieces to capturing the checker
//     or blocking the line between checker and king.
//   - pinned: our pieces on a line between our king and an enemy slider with
//     nothing else in between. They may only move along that line.
//
// En passant is the one case that still needs an explicit test, because it
// removes two pawns from a rank at once and can uncover a rook attack.
func (b *Board) GenMoves(buf *chess.MoveBuffer) int {
	us := b.us()
	them := us ^ 1
	ours, theirs := b.ByColor[us], b.ByColor[them]
	occ := ours | theirs
	ksq := lsb(b.Pieces[us][kKing])
	tp := &b.Pieces[them]
	theirRQ := tp[kRook] | tp[kQueen]
	theirBQ := tp[kBishop] | tp[kQueen]

	// --- danger map ---
	occNoKing := occ &^ bit(ksq)
	var danger uint64
	if them == 0 {
		danger = northWest(tp[kPawn]) | northEast(tp[kPawn])
	} else {
		danger = southWest(tp[kPawn]) | southEast(tp[kPawn])
	}
	for bb := tp[kKnight]; bb != 0; {
		danger |= knightAttacks[popLSB(&bb)]
	}
	danger |= kingAttacks[lsb(tp[kKing])]
	for bb := theirBQ; bb != 0; {
		danger |= bishopAttacks(popLSB(&bb), occNoKing)
	}
	for bb := theirRQ; bb != 0; {
		danger |= rookAttacks(popLSB(&bb), occNoKing)
	}

	// --- king moves ---
	n := b.emit(buf, 0, ksq, kingAttacks[ksq]&^ours&^danger)

	// --- checkers ---
	checkers := (pawnAttacks[us][ksq] & tp[kPawn]) | (knightAttacks[ksq] & tp[kKnight]) |
		(bishopAttacks(ksq, occ) & theirBQ) | (rookAttacks(ksq, occ) & theirRQ)
	if checkers&(checkers-1) != 0 {
		return n // double check: only the king may move
	}

	target := ^ours
	if checkers != 0 {
		target = between[ksq][lsb(checkers)] | checkers
	} else {
		n = b.addCastling(buf, n, us, occ, danger)
	}

	// --- pins: enemy sliders aligned with our king with exactly one of our pieces in between ---
	var pinned uint64
	snipers := (rookAttacks(ksq, 0) & theirRQ) | (bishopAttacks(ksq, 0) & theirBQ)
	for s := snipers; s != 0; {
		blockers := between[ksq][popLSB(&s)] & occ
		if blockers != 0 && blockers&(blockers-1) == 0 && blockers&ours != 0 {
			pinned |= blockers
		}
	}

	// --- knights (a pinned knight can never move) ---
	up := &b.Pieces[us]
	for bb := up[kKnight] &^ pinned; bb != 0; {
		from := popLSB(&bb)
		n = b.emit(buf, n, from, knightAttacks[from]&target)
	}

	// --- sliders ---
	for bb := up[kBishop] | up[kQueen]; bb != 0; {
		from := popLSB(&bb)
		t := bishopAttacks(from, occ) & target
		if pinned&bit(from) != 0 {
			t &= line[ksq][from]
		}
		n = b.emit(buf, n, from, t)
	}
	for bb := up[kRook] | up[kQueen]; bb != 0; {
		from := popLSB(&bb)
		t := rookAttacks(from, occ) & target
		if pinned&bit(from) != 0 {
			t &= line[ksq][from]
		}
		n = b.emit(buf, n, from, t)
	}

	// --- pawns ---
	return b.genPawns(buf, n, us, target, pinned, ksq, occ, theirs)
}

// emit appends one move per set bit in targets.
func (b *Board) emit(buf *chess.MoveBuffer, n int, from chess.Pos, targets uint64) int {
	for targets != 0 {
		to := popLSB(&targets)
		buf[n] = chess.Move{From: from, To: to, Capture: b.Squares[to]}
		n++
	}
	return n
}

// addCastling is only called when not in check. The two squares the king
// crosses must be empty and safe; queenside additionally needs b1/b8 empty.
func (b *Board) addCastling(buf *chess.MoveBuffer, n int, us int, occ, danger uint64) int {
	if us == 0 {
		if b.Castling&chess.WhiteKingside != 0 && (occ|danger)&(bit(61)|bit(62)) == 0 {
			buf[n] = chess.Move{From: 60, To: 62}
			n++
		}
		if b.Castling&chess.WhiteQueenside != 0 && occ&(bit(57)|bit(58)|bit(59)) == 0 && danger&(bit(58)|bit(59)) == 0 {
			buf[n] = chess.Move{From: 60, To: 58}
			n++
		}
	} else {
		if b.Castling&chess.BlackKingside != 0 && (occ|danger)&(bit(5)|bit(6)) == 0 {
			buf[n] = chess.Move{From: 4, To: 6}
			n++
		}
		if b.Castling&chess.BlackQueenside != 0 && occ&(bit(1)|bit(2)|bit(3)) == 0 && danger&(bit(2)|bit(3)) == 0 {
			buf[n] = chess.Move{From: 4, To: 2}
			n++
		}
	}
	return n
}

// genPawns generates pushes and captures for all pawns at once with shifts,
// then en passant per candidate pawn.
func (b *Board) genPawns(buf *chess.MoveBuffer, n int, us int, target, pinned uint64, ksq chess.Pos, occ, theirs uint64) int {
	pawns := b.Pieces[us][kPawn]
	empty := ^occ
	usColor := colorPiece[us]

	var single, double, capL, capR, promoRank uint64
	var dPush, dCapL, dCapR, dEP chess.Pos
	if us == 0 {
		single = north(pawns) & empty
		double = north(single&rankMask(5)) & empty // from rank 2 (y=6) via y=5 to y=4
		capL = northWest(pawns) & theirs
		capR = northEast(pawns) & theirs
		dPush, dCapL, dCapR, dEP = -8, -9, -7, 8
		promoRank = Rank8
	} else {
		single = south(pawns) & empty
		double = south(single&rankMask(2)) & empty
		capL = southWest(pawns) & theirs
		capR = southEast(pawns) & theirs
		dPush, dCapL, dCapR, dEP = 8, 7, 9, -8
		promoRank = Rank1
	}

	n = b.emitPawns(buf, n, single&target, dPush, pinned, ksq, promoRank, usColor)
	n = b.emitPawns(buf, n, double&target, 2*dPush, pinned, ksq, 0, usColor)
	n = b.emitPawns(buf, n, capL&target, dCapL, pinned, ksq, promoRank, usColor)
	n = b.emitPawns(buf, n, capR&target, dCapR, pinned, ksq, promoRank, usColor)

	if b.EnPassant.Valid() {
		ep := b.EnPassant
		capSq := ep + dEP // the pawn that just double-pushed
		for c := pawnAttacks[us^1][ep] & pawns; c != 0; {
			from := popLSB(&c)
			if b.enPassantLegal(from, ep, capSq, ksq, us, occ) {
				buf[n] = chess.Move{From: from, To: ep}
				n++
			}
		}
	}
	return n
}

// emitPawns appends one move per target square; from = to - delta. Pinned pawns
// may only move along the pin line. Targets on promoRank produce four moves.
func (b *Board) emitPawns(buf *chess.MoveBuffer, n int, targets uint64, delta chess.Pos, pinned uint64, ksq chess.Pos, promoRank uint64, usColor chess.Piece) int {
	for targets != 0 {
		to := popLSB(&targets)
		from := to - delta
		if pinned&bit(from) != 0 && line[ksq][from]&bit(to) == 0 {
			continue
		}
		m := chess.Move{From: from, To: to, Capture: b.Squares[to]}
		if bit(to)&promoRank != 0 {
			for _, pt := range [4]chess.Piece{chess.Queen, chess.Rook, chess.Bishop, chess.Knight} {
				m.Promo = usColor | pt
				buf[n] = m
				n++
			}
		} else {
			buf[n] = m
			n++
		}
	}
	return n
}

// hasLegalEnPassant reports whether `side` has at least one legal en passant
// capture onto ep. Used by DoMove and FromSetup to store the square canonically.
func (b *Board) hasLegalEnPassant(ep chess.Pos, side int) bool {
	cands := pawnAttacks[side^1][ep] & b.Pieces[side][kPawn]
	if cands == 0 {
		return false
	}
	capSq := ep + chess.Width
	if side == 1 {
		capSq = ep - chess.Width
	}
	ksq := lsb(b.Pieces[side][kKing])
	occ := b.occupied()
	for c := cands; c != 0; {
		if b.enPassantLegal(popLSB(&c), ep, capSq, ksq, side, occ) {
			return true
		}
	}
	return false
}

// enPassantLegal applies the capture to the occupancy and checks whether our
// king is attacked afterwards. Covers pins, discovered rank attacks and the
// case where the double-pushed pawn is the checker.
func (b *Board) enPassantLegal(from, ep, capSq, ksq chess.Pos, us int, occ uint64) bool {
	newOcc := (occ &^ bit(from) &^ bit(capSq)) | bit(ep)
	tp := &b.Pieces[us^1]
	attackers := (rookAttacks(ksq, newOcc) & (tp[kRook] | tp[kQueen])) |
		(bishopAttacks(ksq, newOcc) & (tp[kBishop] | tp[kQueen])) |
		(knightAttacks[ksq] & tp[kKnight]) |
		(pawnAttacks[us][ksq] & tp[kPawn] &^ bit(capSq))
	return attackers == 0
}
