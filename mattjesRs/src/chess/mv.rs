use std::fmt;

use super::{Piece, Pos};

/// Castling rights as a bit set.
pub type Castling = u8;
pub const WHITE_KINGSIDE: Castling = 1;
pub const WHITE_QUEENSIDE: Castling = 2;
pub const BLACK_KINGSIDE: Castling = 4;
pub const BLACK_QUEENSIDE: Castling = 8;
pub const ALL_CASTLING: Castling = 15;

/// A compact 4-byte move shared by all generators. Castling is a king move of
/// two squares, en passant is a diagonal pawn move with `capture == NONE`.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Move {
    pub from: Pos,
    pub to: Pos,
    /// Promotion piece (with color) or NONE.
    pub promo: Piece,
    /// Captured piece or NONE (also NONE for en passant).
    pub capture: Piece,
}

impl Move {
    pub const NONE: Move = Move { from: Pos::NONE, to: Pos::NONE, promo: Piece::NONE, capture: Piece::NONE };

    pub const fn new(from: Pos, to: Pos, capture: Piece) -> Move {
        Move { from, to, promo: Piece::NONE, capture }
    }

    /// The move in UCI notation like "e2e4" or "e7e8q".
    pub fn uci(&self) -> String {
        let mut s = format!("{}{}", self.from, self.to);
        if self.promo != Piece::NONE {
            s.push(self.promo.kind().to_char() as char);
        }
        s
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.uci())
    }
}

/// Capacity of a move buffer. No legal position has more than 218 moves.
pub const MAX_MOVES: usize = 256;

/// A fixed-size, allocation-free move list.
pub type MoveBuffer = [Move; MAX_MOVES];

pub fn new_buffer() -> MoveBuffer {
    [Move::NONE; MAX_MOVES]
}
