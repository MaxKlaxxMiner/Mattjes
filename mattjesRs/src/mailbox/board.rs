use std::fmt;

use crate::chess::{Castling, Piece, Pos, Setup, ALL_CASTLING, FIELD_COUNT, START_FEN};

/// A complete position. `Copy` makes "copy-make" a plain assignment.
#[derive(Clone, Copy, Debug)]
pub struct Board {
    pub fields: [Piece; FIELD_COUNT],
    pub white_king: Pos,
    pub black_king: Pos,
    /// Target square of a possible en passant capture, or `Pos::NONE`.
    pub en_passant: Pos,
    pub castling: Castling,
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

    /// The start position.
    pub fn new() -> Board {
        Board::from_fen(START_FEN).expect("start FEN is valid")
    }

    /// Parses a FEN string via `Setup::parse`.
    pub fn from_fen(fen: &str) -> Result<Board, String> {
        Ok(Board::from_setup(&Setup::parse(fen)?))
    }

    /// Builds a board from a validated setup.
    pub fn from_setup(s: &Setup) -> Board {
        let mut b = Board::empty();
        for (i, &p) in s.squares.iter().enumerate() {
            if p != Piece::NONE {
                b.set_field(Pos(i as i8), p);
            }
        }
        b.white_move = s.white_move;
        b.castling = s.castling;
        b.en_passant = s.en_passant;
        b.halfmove_clock = s.halfmove_clock;
        b.move_number = s.move_number;
        b
    }

    /// Converts the board back to the representation-independent form.
    pub fn setup(&self) -> Setup {
        Setup {
            squares: self.fields,
            white_move: self.white_move,
            castling: self.castling,
            en_passant: self.en_passant,
            halfmove_clock: self.halfmove_clock,
            move_number: self.move_number,
        }
    }

    pub fn fen(&self) -> String {
        self.setup().fen()
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

impl Default for Board {
    fn default() -> Board {
        Board::new()
    }
}

/// ASCII diagram followed by the FEN.
impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.setup())
    }
}
