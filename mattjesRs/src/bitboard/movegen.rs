use super::bits::*;
use super::board::Board;
use super::tables::{Tables, TABLES};
use crate::chess::{Move, MoveBuffer, Piece, Pos, BLACK_KINGSIDE, BLACK_QUEENSIDE, WHITE_KINGSIDE, WHITE_QUEENSIDE, WIDTH};

impl Board {
    /// Writes all legal moves of the side to move into `buf` and returns their count.
    ///
    /// No move is ever made and taken back to test legality (the classic mailbox
    /// approach), so this only needs `&self`. Legality comes from three bit sets
    /// computed once per position:
    ///   - danger: every square the opponent attacks (with our king removed, so a
    ///     slider's attack continues "through" the king). King moves avoid it.
    ///   - checkers: opponent pieces attacking our king. Double check allows only
    ///     king moves; single check restricts other pieces to capturing the checker
    ///     or blocking the line between checker and king.
    ///   - pinned: our pieces on a line between our king and an enemy slider with
    ///     nothing else in between. They may only move along that line.
    ///
    /// En passant is the one case that still needs an explicit test, because it
    /// removes two pawns from a rank at once and can uncover a rook attack.
    pub fn gen_moves(&self, buf: &mut MoveBuffer) -> usize {
        let t: &Tables = &TABLES;
        let us = self.us();
        let them = us ^ 1;
        let (ours, theirs) = (self.by_color[us], self.by_color[them]);
        let occ = ours | theirs;
        let ksq = lsb(self.pieces[us][K_KING]);
        let tp = &self.pieces[them];
        let their_rq = tp[K_ROOK] | tp[K_QUEEN];
        let their_bq = tp[K_BISHOP] | tp[K_QUEEN];

        // --- danger map ---
        let occ_no_king = occ & !bit(ksq);
        let mut danger = if them == 0 {
            north_west(tp[K_PAWN]) | north_east(tp[K_PAWN])
        } else {
            south_west(tp[K_PAWN]) | south_east(tp[K_PAWN])
        };
        let mut bb = tp[K_KNIGHT];
        while bb != 0 {
            danger |= t.knight[pop_lsb(&mut bb).idx()];
        }
        danger |= t.king[lsb(tp[K_KING]).idx()];
        bb = their_bq;
        while bb != 0 {
            danger |= t.magics.bishop_attacks(pop_lsb(&mut bb), occ_no_king);
        }
        bb = their_rq;
        while bb != 0 {
            danger |= t.magics.rook_attacks(pop_lsb(&mut bb), occ_no_king);
        }

        // --- king moves ---
        let mut n = self.emit(buf, 0, ksq, t.king[ksq.idx()] & !ours & !danger);

        // --- checkers ---
        let checkers = (t.pawn[us][ksq.idx()] & tp[K_PAWN])
            | (t.knight[ksq.idx()] & tp[K_KNIGHT])
            | (t.magics.bishop_attacks(ksq, occ) & their_bq)
            | (t.magics.rook_attacks(ksq, occ) & their_rq);
        if checkers & checkers.wrapping_sub(1) != 0 {
            return n; // double check: only the king may move
        }

        let target = if checkers != 0 {
            t.between[ksq.idx()][lsb(checkers).idx()] | checkers
        } else {
            n = self.add_castling(buf, n, us, occ, danger);
            !ours
        };

        // --- pins: enemy sliders aligned with our king with exactly one of our pieces in between ---
        let mut pinned = 0u64;
        let mut snipers = (t.magics.rook_attacks(ksq, 0) & their_rq) | (t.magics.bishop_attacks(ksq, 0) & their_bq);
        while snipers != 0 {
            let blockers = t.between[ksq.idx()][pop_lsb(&mut snipers).idx()] & occ;
            if blockers != 0 && blockers & (blockers - 1) == 0 && blockers & ours != 0 {
                pinned |= blockers;
            }
        }

        // --- knights (a pinned knight can never move) ---
        let up = &self.pieces[us];
        bb = up[K_KNIGHT] & !pinned;
        while bb != 0 {
            let from = pop_lsb(&mut bb);
            n = self.emit(buf, n, from, t.knight[from.idx()] & target);
        }

        // --- sliders ---
        bb = up[K_BISHOP] | up[K_QUEEN];
        while bb != 0 {
            let from = pop_lsb(&mut bb);
            let mut tg = t.magics.bishop_attacks(from, occ) & target;
            if pinned & bit(from) != 0 {
                tg &= t.line[ksq.idx()][from.idx()];
            }
            n = self.emit(buf, n, from, tg);
        }
        bb = up[K_ROOK] | up[K_QUEEN];
        while bb != 0 {
            let from = pop_lsb(&mut bb);
            let mut tg = t.magics.rook_attacks(from, occ) & target;
            if pinned & bit(from) != 0 {
                tg &= t.line[ksq.idx()][from.idx()];
            }
            n = self.emit(buf, n, from, tg);
        }

        // --- pawns ---
        self.gen_pawns(t, buf, n, us, target, pinned, ksq, occ, theirs)
    }

    /// Appends one move per set bit in targets.
    #[inline(always)]
    fn emit(&self, buf: &mut MoveBuffer, mut n: usize, from: Pos, mut targets: u64) -> usize {
        while targets != 0 {
            let to = pop_lsb(&mut targets);
            buf[n] = Move::new(from, to, self.squares[to.idx()]);
            n += 1;
        }
        n
    }

    /// Only called when not in check. The two squares the king crosses must be
    /// empty and safe; queenside additionally needs b1/b8 empty.
    fn add_castling(&self, buf: &mut MoveBuffer, mut n: usize, us: usize, occ: u64, danger: u64) -> usize {
        if us == 0 {
            if self.castling & WHITE_KINGSIDE != 0 && (occ | danger) & (bit(Pos(61)) | bit(Pos(62))) == 0 {
                buf[n] = Move::new(Pos(60), Pos(62), Piece::NONE);
                n += 1;
            }
            if self.castling & WHITE_QUEENSIDE != 0
                && occ & (bit(Pos(57)) | bit(Pos(58)) | bit(Pos(59))) == 0
                && danger & (bit(Pos(58)) | bit(Pos(59))) == 0
            {
                buf[n] = Move::new(Pos(60), Pos(58), Piece::NONE);
                n += 1;
            }
        } else {
            if self.castling & BLACK_KINGSIDE != 0 && (occ | danger) & (bit(Pos(5)) | bit(Pos(6))) == 0 {
                buf[n] = Move::new(Pos(4), Pos(6), Piece::NONE);
                n += 1;
            }
            if self.castling & BLACK_QUEENSIDE != 0
                && occ & (bit(Pos(1)) | bit(Pos(2)) | bit(Pos(3))) == 0
                && danger & (bit(Pos(2)) | bit(Pos(3))) == 0
            {
                buf[n] = Move::new(Pos(4), Pos(2), Piece::NONE);
                n += 1;
            }
        }
        n
    }

    /// Pushes and captures for all pawns at once with shifts, then en passant per candidate pawn.
    #[allow(clippy::too_many_arguments)]
    fn gen_pawns(&self, t: &Tables, buf: &mut MoveBuffer, mut n: usize, us: usize, target: u64, pinned: u64, ksq: Pos, occ: u64, theirs: u64) -> usize {
        let pawns = self.pieces[us][K_PAWN];
        let empty = !occ;
        let us_color = COLOR_PIECE[us];

        let (single, double, cap_l, cap_r, d_push, d_cap_l, d_cap_r, d_ep, promo_rank) = if us == 0 {
            let single = north(pawns) & empty;
            (
                single,
                north(single & rank_mask(5)) & empty, // from rank 2 (y=6) via y=5 to y=4
                north_west(pawns) & theirs,
                north_east(pawns) & theirs,
                -8i8,
                -9i8,
                -7i8,
                8i8,
                RANK_8,
            )
        } else {
            let single = south(pawns) & empty;
            (
                single,
                south(single & rank_mask(2)) & empty,
                south_west(pawns) & theirs,
                south_east(pawns) & theirs,
                8i8,
                7i8,
                9i8,
                -8i8,
                RANK_1,
            )
        };

        n = self.emit_pawns(t, buf, n, single & target, d_push, pinned, ksq, promo_rank, us_color);
        n = self.emit_pawns(t, buf, n, double & target, 2 * d_push, pinned, ksq, 0, us_color);
        n = self.emit_pawns(t, buf, n, cap_l & target, d_cap_l, pinned, ksq, promo_rank, us_color);
        n = self.emit_pawns(t, buf, n, cap_r & target, d_cap_r, pinned, ksq, promo_rank, us_color);

        if self.en_passant.valid() {
            let ep = self.en_passant;
            let cap_sq = ep + d_ep; // the pawn that just double-pushed
            let mut c = t.pawn[us ^ 1][ep.idx()] & pawns;
            while c != 0 {
                let from = pop_lsb(&mut c);
                if self.en_passant_legal(t, from, ep, cap_sq, ksq, us, occ) {
                    buf[n] = Move::new(from, ep, Piece::NONE);
                    n += 1;
                }
            }
        }
        n
    }

    /// One move per target square; from = to - delta. Pinned pawns may only move
    /// along the pin line. Targets on promo_rank produce four moves.
    #[allow(clippy::too_many_arguments)]
    fn emit_pawns(&self, t: &Tables, buf: &mut MoveBuffer, mut n: usize, mut targets: u64, delta: i8, pinned: u64, ksq: Pos, promo_rank: u64, us_color: Piece) -> usize {
        while targets != 0 {
            let to = pop_lsb(&mut targets);
            let from = to - delta;
            if pinned & bit(from) != 0 && t.line[ksq.idx()][from.idx()] & bit(to) == 0 {
                continue;
            }
            let mut m = Move::new(from, to, self.squares[to.idx()]);
            if bit(to) & promo_rank != 0 {
                for pt in [Piece::QUEEN, Piece::ROOK, Piece::BISHOP, Piece::KNIGHT] {
                    m.promo = us_color | pt;
                    buf[n] = m;
                    n += 1;
                }
            } else {
                buf[n] = m;
                n += 1;
            }
        }
        n
    }

    /// Reports whether `side` has at least one legal en passant capture onto ep.
    /// Used by do_move and from_setup to store the square canonically.
    pub(super) fn has_legal_en_passant(&self, t: &Tables, ep: Pos, side: usize) -> bool {
        let mut cands = t.pawn[side ^ 1][ep.idx()] & self.pieces[side][K_PAWN];
        if cands == 0 {
            return false;
        }
        let cap_sq = if side == 0 { ep + WIDTH } else { ep - WIDTH };
        let ksq = lsb(self.pieces[side][K_KING]);
        let occ = self.occupied();
        while cands != 0 {
            if self.en_passant_legal(t, pop_lsb(&mut cands), ep, cap_sq, ksq, side, occ) {
                return true;
            }
        }
        false
    }

    /// Applies the capture to the occupancy and checks whether our king is attacked
    /// afterwards. Covers pins, discovered rank attacks and the case where the
    /// double-pushed pawn is the checker.
    #[allow(clippy::too_many_arguments)]
    fn en_passant_legal(&self, t: &Tables, from: Pos, ep: Pos, cap_sq: Pos, ksq: Pos, us: usize, occ: u64) -> bool {
        let new_occ = (occ & !bit(from) & !bit(cap_sq)) | bit(ep);
        let tp = &self.pieces[us ^ 1];
        let attackers = (t.magics.rook_attacks(ksq, new_occ) & (tp[K_ROOK] | tp[K_QUEEN]))
            | (t.magics.bishop_attacks(ksq, new_occ) & (tp[K_BISHOP] | tp[K_QUEEN]))
            | (t.knight[ksq.idx()] & tp[K_KNIGHT])
            | (t.pawn[us][ksq.idx()] & tp[K_PAWN] & !bit(cap_sq));
        attackers == 0
    }
}
