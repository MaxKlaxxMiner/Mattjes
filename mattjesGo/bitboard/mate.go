package bitboard

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// The questions a mate search asks that perft never did: is there any legal
// move at all (mate, stalemate), and does a move give check (the last attacker
// ply only needs checking moves). Everything here reuses the legality logic of
// GenMoves (danger map, checkers, pins) but stops early or never builds a list.

// HasMoves reports whether the side to move has at least one legal move. It is
// GenMoves with early exit: king moves first (the only moves in double check),
// then knights, sliders, pawns and en passant. Castling is never needed, because
// a legal castling implies a legal plain king step onto the crossed square.
func (b *Board) HasMoves() bool {
	us := b.us()
	them := us ^ 1
	ours, theirs := b.ByColor[us], b.ByColor[them]
	occ := ours | theirs
	ksq := lsb(b.Pieces[us][kKing])
	tp := &b.Pieces[them]
	theirRQ := tp[kRook] | tp[kQueen]
	theirBQ := tp[kBishop] | tp[kQueen]

	// --- danger map (as in GenMoves) ---
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
	if kingAttacks[ksq]&^ours&^danger != 0 {
		return true
	}

	// --- checkers ---
	checkers := (pawnAttacks[us][ksq] & tp[kPawn]) | (knightAttacks[ksq] & tp[kKnight]) |
		(bishopAttacks(ksq, occ) & theirBQ) | (rookAttacks(ksq, occ) & theirRQ)
	if checkers&(checkers-1) != 0 {
		return false // double check and the king cannot move
	}
	target := ^ours
	if checkers != 0 {
		target = between[ksq][lsb(checkers)] | checkers
	}

	// --- pins ---
	var pinned uint64
	snipers := (rookAttacks(ksq, 0) & theirRQ) | (bishopAttacks(ksq, 0) & theirBQ)
	for s := snipers; s != 0; {
		blockers := between[ksq][popLSB(&s)] & occ
		if blockers != 0 && blockers&(blockers-1) == 0 && blockers&ours != 0 {
			pinned |= blockers
		}
	}

	// --- knights, sliders ---
	up := &b.Pieces[us]
	for bb := up[kKnight] &^ pinned; bb != 0; {
		if knightAttacks[popLSB(&bb)]&target != 0 {
			return true
		}
	}
	for bb := up[kBishop] | up[kQueen]; bb != 0; {
		from := popLSB(&bb)
		t := bishopAttacks(from, occ) & target
		if pinned&bit(from) != 0 {
			t &= line[ksq][from]
		}
		if t != 0 {
			return true
		}
	}
	for bb := up[kRook] | up[kQueen]; bb != 0; {
		from := popLSB(&bb)
		t := rookAttacks(from, occ) & target
		if pinned&bit(from) != 0 {
			t &= line[ksq][from]
		}
		if t != 0 {
			return true
		}
	}

	// --- pawns: all unpinned pawns at once, pinned pawns one by one ---
	pawns := up[kPawn]
	empty := ^occ
	pawnTargets := func(p uint64) uint64 {
		if us == 0 {
			single := north(p) & empty
			return single | (north(single&rankMask(5)) & empty) | (northWest(p) & theirs) | (northEast(p) & theirs)
		}
		single := south(p) & empty
		return single | (south(single&rankMask(2)) & empty) | (southWest(p) & theirs) | (southEast(p) & theirs)
	}
	if pawnTargets(pawns&^pinned)&target != 0 {
		return true
	}
	for bb := pawns & pinned; bb != 0; {
		from := popLSB(&bb)
		if pawnTargets(bit(from))&target&line[ksq][from] != 0 {
			return true
		}
	}

	// --- en passant ---
	if b.EnPassant.Valid() {
		ep := b.EnPassant
		capSq := ep + chess.Width
		if us == 1 {
			capSq = ep - chess.Width
		}
		for c := pawnAttacks[us^1][ep] & pawns; c != 0; {
			if b.enPassantLegal(popLSB(&c), ep, capSq, ksq, us, occ) {
				return true
			}
		}
	}
	return false
}

// IsMate reports whether the side to move is checkmated.
func (b *Board) IsMate() bool { return b.InCheck() && !b.HasMoves() }

// IsStalemate reports whether the side to move has no legal move and is not in check.
func (b *Board) IsStalemate() bool { return !b.InCheck() && !b.HasMoves() }

// checkersAfter returns the squares of our pieces that would attack the enemy
// king after the legal move m, without playing it, and whether the moved piece
// itself is among them (direct check). Checkers other than the moved piece are
// discovered checks: the moved piece left a line, en passant removed a second
// pawn from a line, or the castling rook landed on one. For castling the rook
// counts as the moved piece.
func (b *Board) checkersAfter(m chess.Move) (checkers uint64, direct bool) {
	us := b.us()
	them := us ^ 1
	ksq := lsb(b.Pieces[them][kKing])
	from, to := bit(m.From), bit(m.To)
	occ := (b.occupied() &^ from) | to
	up := &b.Pieces[us]
	rq := (up[kRook] | up[kQueen]) &^ from
	bq := (up[kBishop] | up[kQueen]) &^ from
	kn := up[kKnight] &^ from
	pw := up[kPawn] &^ from
	moved := b.Squares[m.From]
	if m.Promo != chess.None {
		moved = m.Promo
	}
	mover := to
	switch kindIdx(moved) {
	case kPawn:
		if m.To == b.EnPassant && b.EnPassant.Valid() {
			capSq := m.To + chess.Width
			if us == 1 {
				capSq = m.To - chess.Width
			}
			occ &^= bit(capSq)
		}
		pw |= to
	case kKnight:
		kn |= to
	case kBishop:
		bq |= to
	case kRook:
		rq |= to
	case kQueen:
		rq |= to
		bq |= to
	case kKing:
		if d := m.To - m.From; d == 2 || d == -2 {
			rookFrom, rookTo := m.From+3, m.From+1
			if d < 0 {
				rookFrom, rookTo = m.From-4, m.From-1
			}
			occ = occ&^bit(rookFrom) | bit(rookTo)
			rq = rq&^bit(rookFrom) | bit(rookTo)
			mover = bit(rookTo)
		}
	}
	checkers = (rookAttacks(ksq, occ) & rq) | (bishopAttacks(ksq, occ) & bq) |
		(knightAttacks[ksq] & kn) | (pawnAttacks[them][ksq] & pw)
	return checkers, checkers&mover != 0
}

// GivesCheck reports whether the legal move m gives check, without playing it.
func (b *Board) GivesCheck(m chess.Move) bool {
	c, _ := b.checkersAfter(m)
	return c != 0
}

// GenChecks writes only the legal moves that give check into buf and returns
// their count. It is GenMoves followed by a GivesCheck filter; a dedicated
// generator with target masks is an optimisation to be measured, not a
// correctness requirement.
func (b *Board) GenChecks(buf *chess.MoveBuffer) int {
	n := b.GenMoves(buf)
	k := 0
	for i := 0; i < n; i++ {
		if b.GivesCheck(buf[i]) {
			buf[k] = buf[i]
			k++
		}
	}
	return k
}
