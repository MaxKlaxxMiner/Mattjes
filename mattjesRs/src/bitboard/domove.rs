use super::bits::*;
use super::board::{Board, State};
use super::tables::TABLES;
use crate::chess::{Castling, Move, Piece, Pos, ALL_CASTLING, WIDTH};

impl Board {
    /// Plays a legal move. Save `state()` beforehand if you want to `undo_move`.
    pub fn do_move(&mut self, m: Move) {
        let t = &*TABLES;
        let us = self.us();
        let them = us ^ 1;
        let p = self.squares[m.from.idx()];

        if m.capture != Piece::NONE {
            self.remove(t, m.to);
        }
        self.remove(t, m.from);
        if m.promo != Piece::NONE {
            self.put(t, m.to, m.promo);
        } else {
            self.put(t, m.to, p);
        }

        let mut new_en_passant = Pos::NONE;
        if p.is(Piece::PAWN) {
            if m.to == self.en_passant && m.from.x() != m.to.x() {
                // en passant: remove the pawn behind the target square
                let captured = if us == 0 { m.to + WIDTH } else { m.to - WIDTH };
                self.remove(t, captured);
            } else if (m.to - m.from).abs() == 2 * WIDTH {
                // Double push. Only remember the square if the opponent can legally
                // capture en passant (pinned pawns excluded). This is the canonical rule
                // used for counting distinct positions (OEIS A083276) and keeps equal
                // positions equal for hashing.
                let ep = Pos((m.from.0 + m.to.0) / 2);
                if self.has_legal_en_passant(t, ep, them) {
                    new_en_passant = ep;
                }
            }
        } else if p.is(Piece::KING) {
            match m.to - m.from {
                2 => {
                    // kingside: rook from the corner to the square the king passed
                    let rook = self.squares[(m.to + 1).idx()];
                    self.remove(t, m.to + 1);
                    self.put(t, m.to - 1, rook);
                }
                -2 => {
                    // queenside
                    let rook = self.squares[(m.to - 2).idx()];
                    self.remove(t, m.to - 2);
                    self.put(t, m.to + 1, rook);
                }
                _ => {}
            }
        }
        let (old_castling, old_en_passant) = (self.castling, self.en_passant);
        self.en_passant = new_en_passant;
        self.castling &= !(t.castle_clear[m.from.idx()] | t.castle_clear[m.to.idx()]);
        self.xor_state(t, old_castling, old_en_passant);

        if p.is(Piece::PAWN) || m.capture != Piece::NONE {
            self.halfmove_clock = 0;
        } else {
            self.halfmove_clock += 1;
        }
        if us == 1 {
            self.move_number += 1;
        }
        self.white_move = !self.white_move;
    }

    /// Takes back the last move m; `s` must be the `state()` from before `do_move`.
    pub fn undo_move(&mut self, m: Move, s: State) {
        let t = &*TABLES;
        self.white_move = !self.white_move;
        let us = self.us(); // the side that made the move
        let them = us ^ 1;
        if us == 1 {
            self.move_number -= 1;
        }

        let mut p = self.squares[m.to.idx()];
        self.remove(t, m.to);
        if m.promo != Piece::NONE {
            p = COLOR_PIECE[us] | Piece::PAWN;
        }
        self.put(t, m.from, p);
        if m.capture != Piece::NONE {
            self.put(t, m.to, m.capture);
        }

        if p.is(Piece::PAWN) {
            if m.capture == Piece::NONE && m.from.x() != m.to.x() {
                // en passant: put the captured pawn back
                let captured = if us == 0 { m.to + WIDTH } else { m.to - WIDTH };
                self.put(t, captured, COLOR_PIECE[them] | Piece::PAWN);
            }
        } else if p.is(Piece::KING) {
            match m.to - m.from {
                2 => {
                    let rook = self.squares[(m.to - 1).idx()];
                    self.remove(t, m.to - 1);
                    self.put(t, m.to + 1, rook);
                }
                -2 => {
                    let rook = self.squares[(m.to + 1).idx()];
                    self.remove(t, m.to + 1);
                    self.put(t, m.to - 2, rook);
                }
                _ => {}
            }
        }

        self.xor_state(t, (s >> 8) as Castling & ALL_CASTLING, Pos(s as u8 as i8));
        self.restore_state(s);
    }
}
