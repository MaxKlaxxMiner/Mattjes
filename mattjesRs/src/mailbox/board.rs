use std::fmt;

use crate::chess::{Piece, Pos, FIELD_COUNT, HEIGHT, WIDTH};

/// Castling rights as a bit set.
pub const WHITE_KINGSIDE: u8 = 1;
pub const WHITE_QUEENSIDE: u8 = 2;
pub const BLACK_KINGSIDE: u8 = 4;
pub const BLACK_QUEENSIDE: u8 = 8;
pub const ALL_CASTLING: u8 = 15;

/// A complete position. `Copy` makes "copy-make" a plain assignment.
#[derive(Clone, Copy, Debug)]
pub struct Board {
    pub fields: [Piece; FIELD_COUNT],
    pub white_king: Pos,
    pub black_king: Pos,
    /// Target square of a possible en passant capture, or `Pos::NONE`.
    pub en_passant: Pos,
    pub castling: u8,
    pub white_move: bool,
    pub halfmove_clock: u16,
    pub move_number: u16,
}

/// The irreversible part of a position that `undo_move` cannot reconstruct
/// from the move alone: en passant square, castling rights and halfmove clock.
pub type State = u32;

impl Board {
    /// An empty board. White to move, no castling, no en passant.
    pub fn empty() -> Board {
        Board {
            fields: [Piece::NONE; FIELD_COUNT],
            white_king: Pos::NONE,
            black_king: Pos::NONE,
            en_passant: Pos::NONE,
            castling: 0,
            white_move: true,
            halfmove_clock: 0,
            move_number: 1,
        }
    }

    pub fn state(&self) -> State {
        (self.en_passant.0 as u8 as u32) | (self.castling as u32) << 8 | (self.halfmove_clock as u32) << 16
    }

    pub(super) fn restore_state(&mut self, s: State) {
        self.en_passant = Pos(s as u8 as i8);
        self.castling = (s >> 8) as u8 & ALL_CASTLING;
        self.halfmove_clock = (s >> 16) as u16;
    }

    /// Places a piece and keeps the king positions in sync.
    pub fn set_field(&mut self, pos: Pos, p: Piece) {
        self.fields[pos.idx()] = p;
        match p {
            Piece::WHITE_KING => self.white_king = pos,
            Piece::BLACK_KING => self.black_king = pos,
            _ => {}
        }
    }

    #[inline(always)]
    pub fn side_to_move(&self) -> Piece {
        if self.white_move {
            Piece::WHITE
        } else {
            Piece::BLACK
        }
    }

    #[inline(always)]
    pub fn king_pos(&self, color: Piece) -> Pos {
        if color == Piece::WHITE {
            self.white_king
        } else {
            self.black_king
        }
    }
}

/// Renders the board as ASCII diagram followed by the FEN.
impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for y in 0..HEIGHT as i32 {
            write!(f, "    ")?;
            for x in 0..WIDTH as i32 {
                write!(f, "{}", self.fields[Pos::from_xy(x, y).idx()])?;
            }
            writeln!(f)?;
        }
        write!(f, "\nFEN: {}", self.fen())
    }
}
