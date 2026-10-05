package bitboard

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// FromPieces builds a position from single pieces, without castling rights and
// without an en passant square (the endgame tables of milestone 5 need exactly
// that). The squares must be distinct and both kings must be present; the
// caller checks legality with OpponentInCheck. Move counters start at 1.
func FromPieces(whiteMove bool, squares []chess.Pos, pieces []chess.Piece) Board {
	b := Board{EnPassant: chess.NoPos, WhiteMove: whiteMove, MoveNumber: 1}
	for i, sq := range squares {
		b.put(sq, pieces[i])
	}
	b.finishKey()
	return b
}

// PieceAttacks returns the squares a king, queen, rook, bishop or knight on sq
// attacks with the given occupancy. Because these moves are symmetric, the
// same set lists the squares such a piece can have come from (the un-moves
// of the endgame table generator). Pawns are not handled here.
func PieceAttacks(p chess.Piece, sq chess.Pos, occ uint64) uint64 {
	switch p.Type() {
	case chess.King:
		return kingAttacks[sq]
	case chess.Knight:
		return knightAttacks[sq]
	case chess.Bishop:
		return bishopAttacks(sq, occ)
	case chess.Rook:
		return rookAttacks(sq, occ)
	case chess.Queen:
		return queenAttacks(sq, occ)
	}
	return 0
}

// OpponentInCheck reports whether the side NOT to move is in check, which makes
// the position illegal (the side to move could capture the king). Adjacent
// kings are included, because each king attacks the other.
func (b *Board) OpponentInCheck() bool {
	them := b.us() ^ 1
	return b.attackersTo(b.KingPos(them), b.occupied(), them^1) != 0
}
