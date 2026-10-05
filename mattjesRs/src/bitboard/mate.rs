//! The questions a mate search asks that perft never did: is there any legal
//! move at all (mate, stalemate), and does a move give check (the last attacker
//! ply only needs checking moves). Everything here reuses the legality logic of
//! `gen_moves` (danger map, checkers, pins) but stops early or never builds a list.

use super::bits::*;
use super::board::Board;
use super::tables::{Tables, TABLES};
use crate::chess::{Move, MoveBuffer, Piece, WIDTH};

impl Board {
    /// Whether the side to move has at least one legal move. It is `gen_moves`
    /// with early exit: king moves first (the only moves in double check), then
    /// knights, sliders, pawns and en passant. Castling is never needed, because a
    /// legal castling implies a legal plain king step onto the crossed square.
    pub fn has_moves(&self) -> bool {
        let t: &Tables = &TABLES;
        let us = self.us();
        let them = us ^ 1;
        let (ours, theirs) = (self.by_color[us], self.by_color[them]);
        let occ = ours | theirs;
        let ksq = lsb(self.pieces[us][K_KING]);
        let tp = &self.pieces[them];
        let their_rq = tp[K_ROOK] | tp[K_QUEEN];
        let their_bq = tp[K_BISHOP] | tp[K_QUEEN];

        // --- danger map (as in gen_moves) ---
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
        if t.king[ksq.idx()] & !ours & !danger != 0 {
            return true;
        }

        // --- checkers ---
        let checkers = (t.pawn[us][ksq.idx()] & tp[K_PAWN])
            | (t.knight[ksq.idx()] & tp[K_KNIGHT])
            | (t.magics.bishop_attacks(ksq, occ) & their_bq)
            | (t.magics.rook_attacks(ksq, occ) & their_rq);
        if checkers & checkers.wrapping_sub(1) != 0 {
            return false; // double check and the king cannot move
        }
        let target = if checkers != 0 { t.between[ksq.idx()][lsb(checkers).idx()] | checkers } else { !ours };

        // --- pins ---
        let mut pinned = 0u64;
        let mut snipers = (t.magics.rook_attacks(ksq, 0) & their_rq) | (t.magics.bishop_attacks(ksq, 0) & their_bq);
        while snipers != 0 {
            let blockers = t.between[ksq.idx()][pop_lsb(&mut snipers).idx()] & occ;
            if blockers != 0 && blockers & (blockers - 1) == 0 && blockers & ours != 0 {
                pinned |= blockers;
            }
        }

        // --- knights, sliders ---
        let up = &self.pieces[us];
        bb = up[K_KNIGHT] & !pinned;
        while bb != 0 {
            if t.knight[pop_lsb(&mut bb).idx()] & target != 0 {
                return true;
            }
        }
        bb = up[K_BISHOP] | up[K_QUEEN];
        while bb != 0 {
            let from = pop_lsb(&mut bb);
            let mut tg = t.magics.bishop_attacks(from, occ) & target;
            if pinned & bit(from) != 0 {
                tg &= t.line[ksq.idx()][from.idx()];
            }
            if tg != 0 {
                return true;
            }
        }
        bb = up[K_ROOK] | up[K_QUEEN];
        while bb != 0 {
            let from = pop_lsb(&mut bb);
            let mut tg = t.magics.rook_attacks(from, occ) & target;
            if pinned & bit(from) != 0 {
                tg &= t.line[ksq.idx()][from.idx()];
            }
            if tg != 0 {
                return true;
            }
        }

        // --- pawns: all unpinned pawns at once, pinned pawns one by one ---
        let pawns = up[K_PAWN];
        let empty = !occ;
        let pawn_targets = |p: u64| -> u64 {
            if us == 0 {
                let single = north(p) & empty;
                single | (north(single & rank_mask(5)) & empty) | (north_west(p) & theirs) | (north_east(p) & theirs)
            } else {
                let single = south(p) & empty;
                single | (south(single & rank_mask(2)) & empty) | (south_west(p) & theirs) | (south_east(p) & theirs)
            }
        };
        if pawn_targets(pawns & !pinned) & target != 0 {
            return true;
        }
        bb = pawns & pinned;
        while bb != 0 {
            let from = pop_lsb(&mut bb);
            if pawn_targets(bit(from)) & target & t.line[ksq.idx()][from.idx()] != 0 {
                return true;
            }
        }

        // --- en passant ---
        if self.en_passant.valid() {
            let ep = self.en_passant;
            let cap_sq = if us == 0 { ep + WIDTH } else { ep - WIDTH };
            let mut c = t.pawn[us ^ 1][ep.idx()] & pawns;
            while c != 0 {
                if self.en_passant_legal(t, pop_lsb(&mut c), ep, cap_sq, ksq, us, occ) {
                    return true;
                }
            }
        }
        false
    }

    /// Whether the side to move is checkmated.
    pub fn is_mate(&self) -> bool {
        self.in_check() && !self.has_moves()
    }

    /// Whether the side to move has no legal move and is not in check.
    pub fn is_stalemate(&self) -> bool {
        !self.in_check() && !self.has_moves()
    }

    /// The squares of our pieces that would attack the enemy king after the legal
    /// move m, without playing it, and whether the moved piece itself is among them
    /// (direct check). Checkers other than the moved piece are discovered checks:
    /// the moved piece left a line, en passant removed a second pawn from a line,
    /// or the castling rook landed on one. For castling the rook counts as the
    /// moved piece.
    pub(super) fn checkers_after(&self, m: Move) -> (u64, bool) {
        let t: &Tables = &TABLES;
        let us = self.us();
        let them = us ^ 1;
        let ksq = lsb(self.pieces[them][K_KING]);
        let (from, to) = (bit(m.from), bit(m.to));
        let mut occ = (self.occupied() & !from) | to;
        let up = &self.pieces[us];
        let mut rq = (up[K_ROOK] | up[K_QUEEN]) & !from;
        let mut bq = (up[K_BISHOP] | up[K_QUEEN]) & !from;
        let mut kn = up[K_KNIGHT] & !from;
        let mut pw = up[K_PAWN] & !from;
        let moved = if m.promo != Piece::NONE { m.promo } else { self.squares[m.from.idx()] };
        let mut mover = to;
        match kind_idx(moved) {
            K_PAWN => {
                if m.to == self.en_passant && self.en_passant.valid() {
                    let cap_sq = if us == 0 { m.to + WIDTH } else { m.to - WIDTH };
                    occ &= !bit(cap_sq);
                }
                pw |= to;
            }
            K_KNIGHT => kn |= to,
            K_BISHOP => bq |= to,
            K_ROOK => rq |= to,
            K_QUEEN => {
                rq |= to;
                bq |= to;
            }
            _ => {
                let d = m.to.0 - m.from.0;
                if d == 2 || d == -2 {
                    let (rook_from, rook_to) = if d > 0 { (m.from + 3, m.from + 1) } else { (m.from - 4, m.from - 1) };
                    occ = (occ & !bit(rook_from)) | bit(rook_to);
                    rq = (rq & !bit(rook_from)) | bit(rook_to);
                    mover = bit(rook_to);
                }
            }
        }
        let checkers = (t.magics.rook_attacks(ksq, occ) & rq) | (t.magics.bishop_attacks(ksq, occ) & bq) | (t.knight[ksq.idx()] & kn) | (t.pawn[them][ksq.idx()] & pw);
        (checkers, checkers & mover != 0)
    }

    /// Whether the legal move m gives check, without playing it.
    pub fn gives_check(&self, m: Move) -> bool {
        self.checkers_after(m).0 != 0
    }

    /// Writes only the legal moves that give check into `buf` and returns their
    /// count. It is `gen_moves` followed by a `gives_check` filter; a dedicated
    /// generator with target masks is an optimisation to be measured, not a
    /// correctness requirement.
    pub fn gen_checks(&self, buf: &mut MoveBuffer) -> usize {
        let n = self.gen_moves(buf);
        let mut k = 0;
        for i in 0..n {
            if self.gives_check(buf[i]) {
                buf[k] = buf[i];
                k += 1;
            }
        }
        k
    }
}
