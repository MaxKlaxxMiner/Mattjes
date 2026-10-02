use super::board::*;
use super::tables::CASTLE_CLEAR;
use crate::chess::{Move, Piece, Pos, WIDTH};

impl Board {
    /// Plays a legal move. Save `state()` beforehand if you want to `undo_move`.
    pub fn do_move(&mut self, m: Move) {
        let us = self.side_to_move();
        let p = self.fields[m.from.idx()];

        self.fields[m.to.idx()] = p;
        self.fields[m.from.idx()] = Piece::NONE;
        if m.promo != Piece::NONE {
            self.fields[m.to.idx()] = m.promo;
        }

        let mut new_en_passant = Pos::NONE;
        if p.is(Piece::PAWN) {
            if m.to == self.en_passant && m.from.x() != m.to.x() {
                // en passant capture: remove the pawn behind the target square
                let captured = if us == Piece::WHITE { m.to + WIDTH } else { m.to - WIDTH };
                self.fields[captured.idx()] = Piece::NONE;
            } else if (m.to - m.from).abs() == 2 * WIDTH {
                // double push. Only remember the en passant square if an enemy pawn can
                // actually use it. This keeps equal positions equal, which matters for hashing later.
                if self.pawn_can_capture_en_passant(m.to) {
                    new_en_passant = Pos((m.from.0 + m.to.0) / 2);
                }
            }
        } else if p.is(Piece::KING) {
            if us == Piece::WHITE {
                self.white_king = m.to;
            } else {
                self.black_king = m.to;
            }
            match m.to - m.from {
                2 => {
                    // kingside: the rook jumps from the corner to the square the king passed
                    self.fields[(m.to - 1).idx()] = self.fields[(m.to + 1).idx()];
                    self.fields[(m.to + 1).idx()] = Piece::NONE;
                }
                -2 => {
                    // queenside
                    self.fields[(m.to + 1).idx()] = self.fields[(m.to - 2).idx()];
                    self.fields[(m.to - 2).idx()] = Piece::NONE;
                }
                _ => {}
            }
        }
        self.en_passant = new_en_passant;
        self.castling &= !(CASTLE_CLEAR[m.from.idx()] | CASTLE_CLEAR[m.to.idx()]);

        if p.is(Piece::PAWN) || m.capture != Piece::NONE {
            self.halfmove_clock = 0;
        } else {
            self.halfmove_clock += 1;
        }
        if us == Piece::BLACK {
            self.move_number += 1;
        }
        self.white_move = !self.white_move;
    }

    /// Takes back the last move m; `s` must be the `state()` from before `do_move`.
    pub fn undo_move(&mut self, m: Move, s: State) {
        self.white_move = !self.white_move;
        let us = self.side_to_move(); // the side that made the move
        let them = us.opponent();
        if us == Piece::BLACK {
            self.move_number -= 1;
        }

        let mut p = self.fields[m.to.idx()];
        if m.promo != Piece::NONE {
            p = us | Piece::PAWN;
        }
        self.fields[m.from.idx()] = p;
        self.fields[m.to.idx()] = m.capture;

        if p.is(Piece::PAWN) {
            if m.capture == Piece::NONE && m.from.x() != m.to.x() {
                // en passant: put the captured pawn back
                let captured = if us == Piece::WHITE { m.to + WIDTH } else { m.to - WIDTH };
                self.fields[captured.idx()] = them | Piece::PAWN;
            }
        } else if p.is(Piece::KING) {
            if us == Piece::WHITE {
                self.white_king = m.from;
            } else {
                self.black_king = m.from;
            }
            match m.to - m.from {
                2 => {
                    self.fields[(m.to + 1).idx()] = self.fields[(m.to - 1).idx()];
                    self.fields[(m.to - 1).idx()] = Piece::NONE;
                }
                -2 => {
                    self.fields[(m.to - 2).idx()] = self.fields[(m.to + 1).idx()];
                    self.fields[(m.to + 1).idx()] = Piece::NONE;
                }
                _ => {}
            }
        }

        self.restore_state(s);
    }

    /// Reports whether an enemy pawn stands directly left or right of `pawn_sq`,
    /// i.e. whether a double push to `pawn_sq` creates a usable en passant square.
    pub(super) fn pawn_can_capture_en_passant(&self, pawn_sq: Pos) -> bool {
        let enemy_pawn = self.fields[pawn_sq.idx()].color().opponent() | Piece::PAWN;
        (pawn_sq.x() > 0 && self.fields[(pawn_sq - 1).idx()] == enemy_pawn)
            || (pawn_sq.x() < WIDTH - 1 && self.fields[(pawn_sq + 1).idx()] == enemy_pawn)
    }
}
