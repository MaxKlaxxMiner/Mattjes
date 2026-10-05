//! TT value layout, 21 bits, so it fits the 22 value bits of 256 MB buckets:
//!
//! ```text
//! bits 20..19  type: 1 = no mate / escape within depth, 2 = mate / mated in depth
//! bits 18..12  depth in plies (7 bits, up to 127; mate in 39 is 77 plies)
//! bits 11..6   move from
//! bits  5..0   move to
//! ```
//!
//! The type sits in the high bits, so bucket replacement evicts "no mate" entries
//! before proven mates and shallow entries before deep ones. The move carries no
//! promotion piece (queen is assumed when matching); it only orders moves, so a
//! lost underpromotion costs ordering, never correctness.
//!
//! Semantics per node kind:
//! - attacker, type 2: mate in depth plies, exact, valid for any remaining depth >= depth
//! - attacker, type 1: no mate within depth plies, valid for any remaining depth <= depth
//! - defender, type 2: mated in depth plies (every move loses), move = the longest defence
//! - defender, type 1: an escape exists within depth plies, move = the escape
//!
//! Unlike `perft_tt` the key carries no depth salt: a proven mate does not depend
//! on the search depth.

use crate::chess::{Move, Piece, Pos};

pub(super) const TT_NO_MATE: u32 = 1;
pub(super) const TT_MATE: u32 = 2;

/// The width of a packed value; a table must offer at least that.
pub const VALUE_BITS: u32 = 21;

/// `m` may be `Move::NONE` (squares -1) when there is no move to remember; the
/// squares are masked to 6 bits like in Go, where the zero move is used.
pub(super) fn pack_value(typ: u32, depth: u32, m: Move) -> u64 {
    (typ as u64) << 19 | (depth as u64) << 12 | (m.from.0 as u64 & 63) << 6 | (m.to.0 as u64 & 63)
}

/// Returns (type, depth, from, to).
pub(super) fn unpack_value(v: u64) -> (u32, u32, Pos, Pos) {
    ((v >> 19) as u32, (v >> 12) as u32 & 127, Pos((v >> 6) as i8 & 63), Pos(v as i8 & 63))
}

/// Matches a generated move against the from/to pair of a TT entry; among
/// promotions only the queen matches.
pub(super) fn same_move(m: Move, from: Pos, to: Pos) -> bool {
    m.from == from && m.to == to && (m.promo == Piece::NONE || m.promo.0 & Piece::QUEEN.0 != 0)
}
