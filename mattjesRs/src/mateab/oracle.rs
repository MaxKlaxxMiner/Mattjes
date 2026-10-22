use crate::bitboard::Board;
use crate::chess::Piece;
use crate::egtb;

/// What an `Oracle` knows about a position, from the point of view of the side
/// to move.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    Unknown,
    Draw,
    /// The side to move mates.
    Win,
    /// The side to move gets mated.
    Loss,
}

/// The upper bound of a win or loss whose distance the oracle does not know at
/// all (a WDL table): lo is 1, hi is `UNBOUNDED`.
pub const UNBOUNDED: u32 = 1 << 20;

/// Answers terminal questions the search cannot answer by itself. For `Win` and
/// `Loss` the true distance in plies lies in `[lo, hi]`: an exact endgame table
/// gives lo == hi, a quantised one its bucket, a WDL table 1..UNBOUNDED. The
/// search treats a position as proven when hi fits into the remaining depth, as
/// refuted when lo exceeds it, and expands it otherwise, so coarse tables cost
/// search instead of being wrong. The material check is the first
/// implementation; endgame tables (milestone 5) plug in here without touching
/// the search.
pub trait Oracle {
    fn probe(&self, b: &Board) -> (Verdict, u32, u32);
}

/// Answers from the endgame tables (milestone 5) for every position with up to
/// four pieces (and every larger material that is loaded), with the distance as
/// exactly as the table stores it, and falls back to the material check for
/// everything else. Positions with an en passant right are not in the tables;
/// the search simply continues there.
#[derive(Clone, Copy)]
pub struct Tables<'a> {
    pub set: &'a egtb::Set,
    /// Syzygy WDL tables (optional): win or loss with the distance
    /// 1..UNBOUNDED, a cursed win counts as a win and a blessed loss as a
    /// loss (the fifty-move rule is ignored throughout, the author's
    /// decision); they also resolve positions with an en passant right.
    pub wdl: Option<&'a crate::syzygy::Tables>,
}

impl Oracle for Tables<'_> {
    fn probe(&self, b: &Board) -> (Verdict, u32, u32) {
        match self.set.lookup_bounds(b) {
            Some((v, lo, hi)) if v.is_win() => (Verdict::Win, lo, hi),
            Some((v, lo, hi)) if v.is_loss() => (Verdict::Loss, lo, hi),
            Some((egtb::Value::DRAW, _, _)) => (Verdict::Draw, 0, 0),
            Some(_) => (Verdict::Unknown, 0, 0), // invalid position, cannot happen for legal input
            None => {
                if let Some(wdl) = self.wdl {
                    if let Some(v) = wdl.probe_wdl(b) {
                        return if v >= crate::syzygy::CURSED_WIN {
                            (Verdict::Win, 1, UNBOUNDED)
                        } else if v <= crate::syzygy::BLESSED_LOSS {
                            (Verdict::Loss, 1, UNBOUNDED)
                        } else {
                            (Verdict::Draw, 0, 0)
                        };
                    }
                }
                Material.probe(b)
            }
        }
    }
}

/// a8 (bit 0) is a light square.
const LIGHT_SQUARES: u64 = 0xAA55AA55AA55AA55;

/// Answers `Draw` for dead positions, in which no sequence of legal moves can
/// lead to mate (FIDE 5.2.2): king against king, king and one minor piece against
/// king, and king and bishop against king and bishop with both bishops on the
/// same square colour. Everything else is `Unknown`; note that two knights
/// against a bare king can still mate with the defender's help, so it is not dead.
#[derive(Clone, Copy)]
pub struct Material;

impl Oracle for Material {
    fn probe(&self, b: &Board) -> (Verdict, u32, u32) {
        let heavy = b.piece_bb(Piece::WHITE_PAWN)
            | b.piece_bb(Piece::BLACK_PAWN)
            | b.piece_bb(Piece::WHITE_ROOK)
            | b.piece_bb(Piece::BLACK_ROOK)
            | b.piece_bb(Piece::WHITE_QUEEN)
            | b.piece_bb(Piece::BLACK_QUEEN);
        if heavy != 0 {
            return (Verdict::Unknown, 0, 0);
        }
        let knights = b.piece_bb(Piece::WHITE_KNIGHT) | b.piece_bb(Piece::BLACK_KNIGHT);
        let bishops = b.piece_bb(Piece::WHITE_BISHOP) | b.piece_bb(Piece::BLACK_BISHOP);
        match (knights | bishops).count_ones() {
            0 | 1 => (Verdict::Draw, 0, 0),
            // two bishops on the same colour, necessarily one per side
            2 if knights == 0 && (bishops & LIGHT_SQUARES == 0 || bishops & !LIGHT_SQUARES == 0) => (Verdict::Draw, 0, 0),
            _ => (Verdict::Unknown, 0, 0),
        }
    }
}
