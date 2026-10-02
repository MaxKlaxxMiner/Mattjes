use std::sync::LazyLock;
use std::time::{Duration, Instant};

use super::bits::*;
use super::magic::Magics;
use super::zobrist::Zobrist;
use crate::chess::{Castling, Pos, BLACK_KINGSIDE, BLACK_QUEENSIDE, FIELD_COUNT, WHITE_KINGSIDE, WHITE_QUEENSIDE};

/// All lookup tables. Built once on first use (`LazyLock`), because the magic
/// search is far too slow for compile-time `const fn` evaluation. Hot code
/// fetches `&TABLES` once per call and then indexes plain arrays.
pub struct Tables {
    pub knight: [u64; FIELD_COUNT],
    pub king: [u64; FIELD_COUNT],
    /// `pawn[color][sq]` are the squares a pawn of that color on sq attacks.
    /// Mirror property: a pawn of color c on `from` attacks `to` iff from ∈ pawn[!c][to].
    pub pawn: [[u64; FIELD_COUNT]; 2],
    /// Squares strictly between a and b if they share a line, else 0.
    pub between: [[u64; FIELD_COUNT]; FIELD_COUNT],
    /// The whole line through a and b (edge to edge) if they share one, else 0.
    pub line: [[u64; FIELD_COUNT]; FIELD_COUNT],
    /// Castling rights lost when a piece moves from or to sq.
    pub castle_clear: [Castling; FIELD_COUNT],
    pub magics: Magics,
    pub zobrist: Zobrist,
    pub init_duration: Duration,
}

pub static TABLES: LazyLock<Tables> = LazyLock::new(Tables::build);

/// Forces table construction and returns how long it took.
pub fn init_duration() -> Duration {
    TABLES.init_duration
}

impl Tables {
    fn build() -> Tables {
        let start = Instant::now();

        let mut knight = [0u64; FIELD_COUNT];
        let mut king = [0u64; FIELD_COUNT];
        let mut pawn = [[0u64; FIELD_COUNT]; 2];
        for sq in 0..FIELD_COUNT {
            let b = bit(Pos(sq as i8));
            knight[sq] = north(north(east(b)))
                | north(east(east(b)))
                | south(east(east(b)))
                | south(south(east(b)))
                | south(south(west(b)))
                | south(west(west(b)))
                | north(west(west(b)))
                | north(north(west(b)));
            king[sq] = north(b) | south(b) | west(b) | east(b) | north_west(b) | north_east(b) | south_west(b) | south_east(b);
            pawn[0][sq] = north_west(b) | north_east(b);
            pawn[1][sq] = south_west(b) | south_east(b);
        }

        let magics = Magics::new();

        let mut between = [[0u64; FIELD_COUNT]; FIELD_COUNT];
        let mut line = [[0u64; FIELD_COUNT]; FIELD_COUNT];
        for a in 0..FIELD_COUNT {
            for b in 0..FIELD_COUNT {
                if a == b {
                    continue;
                }
                let (pa, pb) = (Pos(a as i8), Pos(b as i8));
                if magics.rook_attacks(pa, 0) & bit(pb) != 0 {
                    between[a][b] = magics.rook_attacks(pa, bit(pb)) & magics.rook_attacks(pb, bit(pa));
                    line[a][b] = (magics.rook_attacks(pa, 0) & magics.rook_attacks(pb, 0)) | bit(pa) | bit(pb);
                } else if magics.bishop_attacks(pa, 0) & bit(pb) != 0 {
                    between[a][b] = magics.bishop_attacks(pa, bit(pb)) & magics.bishop_attacks(pb, bit(pa));
                    line[a][b] = (magics.bishop_attacks(pa, 0) & magics.bishop_attacks(pb, 0)) | bit(pa) | bit(pb);
                }
            }
        }

        let mut castle_clear = [0 as Castling; FIELD_COUNT];
        castle_clear[0] = BLACK_QUEENSIDE;
        castle_clear[4] = BLACK_KINGSIDE | BLACK_QUEENSIDE;
        castle_clear[7] = BLACK_KINGSIDE;
        castle_clear[56] = WHITE_QUEENSIDE;
        castle_clear[60] = WHITE_KINGSIDE | WHITE_QUEENSIDE;
        castle_clear[63] = WHITE_KINGSIDE;

        let zobrist = Zobrist::new();

        Tables { knight, king, pawn, between, line, castle_clear, magics, zobrist, init_duration: start.elapsed() }
    }
}
