use super::board::*;
use super::tables::*;
use crate::chess::{new_buffer, Move, MoveBuffer, Piece, Pos, FIELD_COUNT, WIDTH};
use crate::chess::{BLACK_KINGSIDE, BLACK_QUEENSIDE, WHITE_KINGSIDE, WHITE_QUEENSIDE};

impl Board {
    /// Writes all legal moves of the side to move into `buf` and returns their count.
    ///
    /// Like yacboard, moves are generated pseudo-legally and each candidate is
    /// verified by making it on the board, testing whether the own king is attacked
    /// and taking it back (see `is_legal`). Castling is validated separately.
    /// `&mut self` because of that temporary modification; the board is unchanged afterwards.
    pub fn gen_moves(&mut self, buf: &mut MoveBuffer) -> usize {
        let us = self.side_to_move();
        let them = us.opponent();
        let mut n = 0;

        // pawn geometry depends on color
        let (fwd, start_y, promo_y, cap_dirs) = if us == Piece::WHITE {
            (-WIDTH, 6, 1, [DIR_NW, DIR_NE]) // white pawns move towards rank 8 = smaller index
        } else {
            (WIDTH, 1, 6, [DIR_SW, DIR_SE])
        };

        for sq in 0..FIELD_COUNT {
            let p = self.fields[sq];
            if !p.is(us) {
                continue;
            }
            let sq = Pos(sq as i8);
            let dist = EDGE_DIST[sq.idx()];

            match p.kind() {
                Piece::PAWN => {
                    let y = sq.y(); // never 0 or 7, set_fen rejects pawns on the back ranks
                    // single and double push
                    let to = sq + fwd;
                    if self.fields[to.idx()] == Piece::NONE {
                        let mut m = Move::new(sq, to, Piece::NONE);
                        if y == promo_y {
                            n = self.add_promotions(buf, n, m, us);
                        } else {
                            if self.is_legal(m) {
                                buf[n] = m;
                                n += 1;
                            }
                            let to2 = to + fwd;
                            if y == start_y && self.fields[to2.idx()] == Piece::NONE {
                                m.to = to2;
                                if self.is_legal(m) {
                                    buf[n] = m;
                                    n += 1;
                                }
                            }
                        }
                    }
                    // captures and en passant
                    for d in cap_dirs {
                        if dist[d] == 0 {
                            continue;
                        }
                        let to = sq + DIR_DELTA[d];
                        let target = self.fields[to.idx()];
                        if target.is(them) {
                            let m = Move::new(sq, to, target);
                            if y == promo_y {
                                n = self.add_promotions(buf, n, m, us);
                            } else if self.is_legal(m) {
                                buf[n] = m;
                                n += 1;
                            }
                        } else if to == self.en_passant {
                            let m = Move::new(sq, to, Piece::NONE);
                            if self.is_legal(m) {
                                buf[n] = m;
                                n += 1;
                            }
                        }
                    }
                }

                Piece::KNIGHT => {
                    for &to in KNIGHT_TARGETS[sq.idx()].as_slice() {
                        let target = self.fields[to.idx()];
                        if !target.is(us) {
                            let m = Move::new(sq, to, target);
                            if self.is_legal(m) {
                                buf[n] = m;
                                n += 1;
                            }
                        }
                    }
                }

                Piece::KING => {
                    for &to in KING_TARGETS[sq.idx()].as_slice() {
                        let target = self.fields[to.idx()];
                        if !target.is(us) {
                            let m = Move::new(sq, to, target);
                            if self.is_legal(m) {
                                buf[n] = m;
                                n += 1;
                            }
                        }
                    }
                    n = self.add_castling(buf, n, us, them);
                }

                kind => {
                    // sliders
                    let (first, last) = match kind {
                        Piece::ROOK => (DIR_N, DIR_E),
                        Piece::BISHOP => (DIR_NW, DIR_SE),
                        _ => (DIR_N, DIR_SE),
                    };
                    for d in first..=last {
                        let mut to = sq;
                        for _ in 0..dist[d] {
                            to = to + DIR_DELTA[d];
                            let target = self.fields[to.idx()];
                            if target.is(us) {
                                break;
                            }
                            let m = Move::new(sq, to, target);
                            if self.is_legal(m) {
                                buf[n] = m;
                                n += 1;
                            }
                            if target != Piece::NONE {
                                break;
                            }
                        }
                    }
                }
            }
        }
        n
    }

    /// Adds the four promotion variants of m. Legality is identical for all of them.
    fn add_promotions(&mut self, buf: &mut MoveBuffer, mut n: usize, mut m: Move, us: Piece) -> usize {
        if !self.is_legal(m) {
            return n;
        }
        for pt in [Piece::QUEEN, Piece::ROOK, Piece::BISHOP, Piece::KNIGHT] {
            m.promo = us | pt;
            buf[n] = m;
            n += 1;
        }
        n
    }

    /// Adds legal castling moves. The king may not be in check, pass through or
    /// land on an attacked square, and the squares between king and rook must be empty.
    fn add_castling(&self, buf: &mut MoveBuffer, mut n: usize, us: Piece, them: Piece) -> usize {
        let (kingside, queenside, king) = if us == Piece::WHITE {
            (WHITE_KINGSIDE, WHITE_QUEENSIDE, Pos(60))
        } else {
            (BLACK_KINGSIDE, BLACK_QUEENSIDE, Pos(4))
        };
        if self.castling & (kingside | queenside) == 0 || self.is_attacked(king, them) {
            return n;
        }
        let f = &self.fields;
        if self.castling & kingside != 0
            && f[(king + 1).idx()] == Piece::NONE
            && f[(king + 2).idx()] == Piece::NONE
            && !self.is_attacked(king + 1, them)
            && !self.is_attacked(king + 2, them)
        {
            buf[n] = Move::new(king, king + 2, Piece::NONE);
            n += 1;
        }
        if self.castling & queenside != 0
            && f[(king - 1).idx()] == Piece::NONE
            && f[(king - 2).idx()] == Piece::NONE
            && f[(king - 3).idx()] == Piece::NONE
            && !self.is_attacked(king - 1, them)
            && !self.is_attacked(king - 2, them)
        {
            buf[n] = Move::new(king, king - 2, Piece::NONE);
            n += 1;
        }
        n
    }

    /// Makes the pseudo-legal move m, checks whether the own king is attacked and
    /// takes the move back. Promotion piece and castling are irrelevant here.
    fn is_legal(&mut self, m: Move) -> bool {
        let us = self.side_to_move();
        let them = us.opponent();
        let p = self.fields[m.from.idx()];

        self.fields[m.to.idx()] = p;
        self.fields[m.from.idx()] = Piece::NONE;

        let mut ep_captured = Pos::NONE;
        if p.is(Piece::PAWN) && m.to == self.en_passant && m.from.x() != m.to.x() {
            ep_captured = if us == Piece::WHITE { m.to + WIDTH } else { m.to - WIDTH };
            self.fields[ep_captured.idx()] = Piece::NONE;
        }

        let king_sq = if p.is(Piece::KING) { m.to } else { self.king_pos(us) };
        let attacked = self.is_attacked(king_sq, them);

        self.fields[m.from.idx()] = p;
        self.fields[m.to.idx()] = m.capture;
        if ep_captured != Pos::NONE {
            self.fields[ep_captured.idx()] = them | Piece::PAWN;
        }
        !attacked
    }

    /// All legal moves as a freshly allocated vector (convenience, not for hot loops).
    pub fn moves(&mut self) -> Vec<Move> {
        let mut buf = new_buffer();
        let n = self.gen_moves(&mut buf);
        buf[..n].to_vec()
    }

    /// Looks up a legal move by its UCI string.
    pub fn find_uci(&mut self, uci: &str) -> Option<Move> {
        self.moves().into_iter().find(|m| m.uci() == uci)
    }
}
