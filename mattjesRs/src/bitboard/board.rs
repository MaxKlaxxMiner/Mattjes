use std::fmt;

use super::bits::*;
use super::tables::{Tables, TABLES};
use crate::chess::{new_buffer, Castling, Move, Piece, Pos, Setup, ALL_CASTLING, FIELD_COUNT, START_FEN};

/// A complete position. `pieces` holds one bit set per color and kind,
/// `by_color` the union per color, and `squares` the piece per square for O(1)
/// lookups (what was captured?). All three are kept in sync by put/remove.
#[derive(Clone, Copy, Debug)]
pub struct Board {
    pub pieces: [[u64; KIND_COUNT]; 2],
    pub by_color: [u64; 2],
    pub squares: [Piece; FIELD_COUNT],
    pub en_passant: Pos,
    pub castling: Castling,
    pub white_move: bool,
    pub halfmove_clock: u16,
    pub move_number: u16,
}

/// The irreversible part of a position for `undo_move`.
pub type State = u32;

impl Board {
    pub fn empty() -> Board {
        Board {
            pieces: [[0; KIND_COUNT]; 2],
            by_color: [0; 2],
            squares: [Piece::NONE; FIELD_COUNT],
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
                b.put(Pos(i as i8), p);
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
            squares: self.squares,
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

    #[inline(always)]
    pub(super) fn put(&mut self, sq: Pos, p: Piece) {
        let bb = bit(sq);
        let c = color_idx(p);
        self.pieces[c][kind_idx(p)] |= bb;
        self.by_color[c] |= bb;
        self.squares[sq.idx()] = p;
    }

    #[inline(always)]
    pub(super) fn remove(&mut self, sq: Pos) {
        let p = self.squares[sq.idx()];
        let bb = bit(sq);
        let c = color_idx(p);
        self.pieces[c][kind_idx(p)] &= !bb;
        self.by_color[c] &= !bb;
        self.squares[sq.idx()] = Piece::NONE;
    }

    /// The color index of the side to move.
    #[inline(always)]
    pub fn us(&self) -> usize {
        if self.white_move {
            0
        } else {
            1
        }
    }

    #[inline(always)]
    pub fn occupied(&self) -> u64 {
        self.by_color[0] | self.by_color[1]
    }

    /// The king square of the color index.
    #[inline(always)]
    pub fn king_pos(&self, c: usize) -> Pos {
        lsb(self.pieces[c][K_KING])
    }

    /// All pieces of color index `by` attacking sq, given occupancy occ.
    pub fn attackers_to(&self, t: &Tables, sq: Pos, occ: u64, by: usize) -> u64 {
        let p = &self.pieces[by];
        (t.pawn[by ^ 1][sq.idx()] & p[K_PAWN])
            | (t.knight[sq.idx()] & p[K_KNIGHT])
            | (t.king[sq.idx()] & p[K_KING])
            | (t.magics.bishop_attacks(sq, occ) & (p[K_BISHOP] | p[K_QUEEN]))
            | (t.magics.rook_attacks(sq, occ) & (p[K_ROOK] | p[K_QUEEN]))
    }

    /// Reports whether the side to move is in check.
    pub fn in_check(&self) -> bool {
        let us = self.us();
        self.attackers_to(&TABLES, self.king_pos(us), self.occupied(), us ^ 1) != 0
    }

    /// All legal moves as a freshly allocated vector (convenience, not for hot loops).
    pub fn moves(&self) -> Vec<Move> {
        let mut buf = new_buffer();
        let n = self.gen_moves(&mut buf);
        buf[..n].to_vec()
    }
}

impl Default for Board {
    fn default() -> Board {
        Board::new()
    }
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.setup())
    }
}
