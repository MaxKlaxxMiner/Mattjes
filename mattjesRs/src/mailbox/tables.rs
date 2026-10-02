//! Lookup tables, computed at compile time with `const fn`. Rust evaluates these
//! functions once during compilation, so the tables are plain static data in the
//! binary and cost nothing at startup (Go builds the same tables in `init()`).

use crate::chess::{Pos, FIELD_COUNT, HEIGHT, WIDTH};
use crate::chess::{BLACK_KINGSIDE, BLACK_QUEENSIDE, WHITE_KINGSIDE, WHITE_QUEENSIDE};

/// Directions. The first four are orthogonal (rook), the last four diagonal (bishop).
pub const DIR_N: usize = 0; // towards rank 8 (smaller index)
pub const DIR_S: usize = 1;
pub const DIR_W: usize = 2;
pub const DIR_E: usize = 3;
pub const DIR_NW: usize = 4;
pub const DIR_NE: usize = 5;
pub const DIR_SW: usize = 6;
pub const DIR_SE: usize = 7;

pub const DIR_DELTA: [i8; 8] = [-WIDTH, WIDTH, -1, 1, -WIDTH - 1, -WIDTH + 1, WIDTH - 1, WIDTH + 1];

/// `EDGE_DIST[sq][dir]` is the number of squares from sq to the board edge in that direction.
pub static EDGE_DIST: [[i8; 8]; FIELD_COUNT] = build_edge_dist();

/// A short list of destination squares (knight or king), at most 8.
#[derive(Clone, Copy)]
pub struct Targets {
    n: u8,
    sq: [Pos; 8],
}

impl Targets {
    const EMPTY: Targets = Targets { n: 0, sq: [Pos::NONE; 8] };

    const fn push(mut self, p: Pos) -> Targets {
        self.sq[self.n as usize] = p;
        self.n += 1;
        self
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[Pos] {
        &self.sq[..self.n as usize]
    }
}

pub static KNIGHT_TARGETS: [Targets; FIELD_COUNT] = build_knight_targets();
pub static KING_TARGETS: [Targets; FIELD_COUNT] = build_king_targets();

/// `CASTLE_CLEAR[sq]` holds the castling rights that are lost when a piece moves from or to sq.
pub static CASTLE_CLEAR: [u8; FIELD_COUNT] = build_castle_clear();

const fn min(a: i8, b: i8) -> i8 {
    if a < b {
        a
    } else {
        b
    }
}

const fn build_edge_dist() -> [[i8; 8]; FIELD_COUNT] {
    let mut t = [[0i8; 8]; FIELD_COUNT];
    let mut sq = 0;
    while sq < FIELD_COUNT {
        let x = (sq as i8) % WIDTH;
        let y = (sq as i8) / WIDTH;
        let (n, s, w, e) = (y, HEIGHT - 1 - y, x, WIDTH - 1 - x);
        t[sq] = [n, s, w, e, min(n, w), min(n, e), min(s, w), min(s, e)];
        sq += 1;
    }
    t
}

const fn build_knight_targets() -> [Targets; FIELD_COUNT] {
    const JUMPS: [(i32, i32); 8] = [(1, 2), (2, 1), (2, -1), (1, -2), (-1, -2), (-2, -1), (-2, 1), (-1, 2)];
    let mut t = [Targets::EMPTY; FIELD_COUNT];
    let mut sq = 0;
    while sq < FIELD_COUNT {
        let x = (sq % WIDTH as usize) as i32;
        let y = (sq / WIDTH as usize) as i32;
        let mut j = 0;
        while j < JUMPS.len() {
            let to = Pos::from_xy(x + JUMPS[j].0, y + JUMPS[j].1);
            if to.valid() {
                t[sq] = t[sq].push(to);
            }
            j += 1;
        }
        sq += 1;
    }
    t
}

const fn build_king_targets() -> [Targets; FIELD_COUNT] {
    let mut t = [Targets::EMPTY; FIELD_COUNT];
    let mut sq = 0;
    while sq < FIELD_COUNT {
        let mut d = 0;
        while d < 8 {
            if EDGE_DIST[sq][d] > 0 {
                t[sq] = t[sq].push(Pos(sq as i8 + DIR_DELTA[d]));
            }
            d += 1;
        }
        sq += 1;
    }
    t
}

const fn build_castle_clear() -> [u8; FIELD_COUNT] {
    let mut t = [0u8; FIELD_COUNT];
    t[0] = BLACK_QUEENSIDE;
    t[4] = BLACK_KINGSIDE | BLACK_QUEENSIDE;
    t[7] = BLACK_KINGSIDE;
    t[56] = WHITE_QUEENSIDE;
    t[60] = WHITE_KINGSIDE | WHITE_QUEENSIDE;
    t[63] = WHITE_KINGSIDE;
    t
}
