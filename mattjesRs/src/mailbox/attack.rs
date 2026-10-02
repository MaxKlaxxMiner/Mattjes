use super::board::Board;
use super::tables::*;
use crate::chess::{Piece, Pos};

impl Board {
    /// Reports whether square `sq` is attacked by any piece of color `by`.
    pub fn is_attacked(&self, sq: Pos, by: Piece) -> bool {
        let dist = &EDGE_DIST[sq.idx()];
        let f = &self.fields;

        // pawns: a white pawn attacks diagonally towards rank 8, so it attacks sq from the south-west/south-east
        if by == Piece::WHITE {
            if (dist[DIR_SW] > 0 && f[(sq + DIR_DELTA[DIR_SW]).idx()] == Piece::WHITE_PAWN)
                || (dist[DIR_SE] > 0 && f[(sq + DIR_DELTA[DIR_SE]).idx()] == Piece::WHITE_PAWN)
            {
                return true;
            }
        } else if (dist[DIR_NW] > 0 && f[(sq + DIR_DELTA[DIR_NW]).idx()] == Piece::BLACK_PAWN)
            || (dist[DIR_NE] > 0 && f[(sq + DIR_DELTA[DIR_NE]).idx()] == Piece::BLACK_PAWN)
        {
            return true;
        }

        // knights and king
        let knight = by | Piece::KNIGHT;
        let king = by | Piece::KING;
        for &t in KNIGHT_TARGETS[sq.idx()].as_slice() {
            if f[t.idx()] == knight {
                return true;
            }
        }
        for &t in KING_TARGETS[sq.idx()].as_slice() {
            if f[t.idx()] == king {
                return true;
            }
        }

        // sliders: the first piece seen on each ray decides
        for d in 0..8 {
            let slider = if d >= DIR_NW { Piece::QUEEN | Piece::BISHOP } else { Piece::QUEEN | Piece::ROOK };
            let mut t = sq;
            for _ in 0..dist[d] {
                t = t + DIR_DELTA[d];
                let p = f[t.idx()];
                if p == Piece::NONE {
                    continue;
                }
                if p.is(by) && p.is(slider) {
                    return true;
                }
                break;
            }
        }
        false
    }

    /// Reports whether the side to move is in check.
    pub fn in_check(&self) -> bool {
        let us = self.side_to_move();
        self.is_attacked(self.king_pos(us), us.opponent())
    }
}
