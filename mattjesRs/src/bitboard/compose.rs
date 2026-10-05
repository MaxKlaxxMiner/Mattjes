use super::board::Board;
use super::tables::{Tables, TABLES};
use crate::chess::{Piece, Pos};

impl Board {
    /// Builds a position from single pieces, without castling rights and without
    /// an en passant square (the endgame tables of milestone 5 need exactly
    /// that). The squares must be distinct and both kings must be present; the
    /// caller checks legality with `opponent_in_check`. Move counters start at 1.
    pub fn from_pieces(white_move: bool, squares: &[Pos], pieces: &[Piece]) -> Board {
        let t: &Tables = &TABLES;
        let mut b = Board::empty();
        b.white_move = white_move;
        for (i, &sq) in squares.iter().enumerate() {
            b.put(t, sq, pieces[i]);
        }
        b.finish_key(t);
        b
    }

    /// Reports whether the side NOT to move is in check, which makes the
    /// position illegal (the side to move could capture the king). Adjacent
    /// kings are included, because each king attacks the other.
    pub fn opponent_in_check(&self) -> bool {
        let them = self.us() ^ 1;
        self.attackers_to(&TABLES, self.king_pos(them), self.occupied(), them ^ 1) != 0
    }
}

/// The squares a king, queen, rook, bishop or knight on sq attacks with the
/// given occupancy. Because these moves are symmetric, the same set lists the
/// squares such a piece can have come from (the un-moves of the endgame table
/// generator). Pawns are not handled here.
pub fn piece_attacks(p: Piece, sq: Pos, occ: u64) -> u64 {
    let t: &Tables = &TABLES;
    match p.kind() {
        Piece::KING => t.king[sq.idx()],
        Piece::KNIGHT => t.knight[sq.idx()],
        Piece::BISHOP => t.magics.bishop_attacks(sq, occ),
        Piece::ROOK => t.magics.rook_attacks(sq, occ),
        Piece::QUEEN => t.magics.queen_attacks(sq, occ),
        _ => 0,
    }
}
