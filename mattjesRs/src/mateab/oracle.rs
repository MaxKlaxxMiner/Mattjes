use crate::bitboard::Board;
use crate::chess::Piece;

/// What an `Oracle` knows about a position, from the point of view of the side
/// to move.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    Unknown,
    Draw,
    /// The side to move mates; the distance in plies is given when known, else 0.
    Win,
    /// The side to move gets mated; distance as above.
    Loss,
}

/// Answers terminal questions the search cannot answer by itself. The material
/// check is the first implementation; endgame tables (milestone 5) plug in here
/// without touching the search.
pub trait Oracle {
    fn probe(&self, b: &Board) -> (Verdict, u32);
}

/// a8 (bit 0) is a light square.
const LIGHT_SQUARES: u64 = 0xAA55AA55AA55AA55;

/// Answers `Draw` for dead positions, in which no sequence of legal moves can
/// lead to mate (FIDE 5.2.2): king against king, king and one minor piece against
/// king, and king and bishop against king and bishop with both bishops on the
/// same square colour. Everything else is `Unknown`; note that two knights
/// against a bare king can still mate with the defender's help, so it is not dead.
pub struct Material;

impl Oracle for Material {
    fn probe(&self, b: &Board) -> (Verdict, u32) {
        let heavy = b.piece_bb(Piece::WHITE_PAWN)
            | b.piece_bb(Piece::BLACK_PAWN)
            | b.piece_bb(Piece::WHITE_ROOK)
            | b.piece_bb(Piece::BLACK_ROOK)
            | b.piece_bb(Piece::WHITE_QUEEN)
            | b.piece_bb(Piece::BLACK_QUEEN);
        if heavy != 0 {
            return (Verdict::Unknown, 0);
        }
        let knights = b.piece_bb(Piece::WHITE_KNIGHT) | b.piece_bb(Piece::BLACK_KNIGHT);
        let bishops = b.piece_bb(Piece::WHITE_BISHOP) | b.piece_bb(Piece::BLACK_BISHOP);
        match (knights | bishops).count_ones() {
            0 | 1 => (Verdict::Draw, 0),
            // two bishops on the same colour, necessarily one per side
            2 if knights == 0 && (bishops & LIGHT_SQUARES == 0 || bishops & !LIGHT_SQUARES == 0) => (Verdict::Draw, 0),
            _ => (Verdict::Unknown, 0),
        }
    }
}
