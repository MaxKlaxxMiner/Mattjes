use crate::chess::{Piece, Pos};

pub const FILE_A: u64 = 0x0101010101010101;
pub const FILE_H: u64 = 0x8080808080808080;
pub const RANK_8: u64 = 0xff; // y = 0
pub const RANK_1: u64 = 0xff << 56; // y = 7

/// The bits of row y (y = 0 is rank 8).
#[inline(always)]
pub const fn rank_mask(y: u32) -> u64 {
    0xff << (8 * y)
}

#[inline(always)]
pub const fn bit(sq: Pos) -> u64 {
    1u64 << (sq.0 as u32)
}

/// The lowest set square. bb must not be 0.
#[inline(always)]
pub fn lsb(bb: u64) -> Pos {
    Pos(bb.trailing_zeros() as i8)
}

/// Removes and returns the lowest set square.
#[inline(always)]
pub fn pop_lsb(bb: &mut u64) -> Pos {
    let sq = lsb(*bb);
    *bb &= *bb - 1;
    sq
}

// Directional shifts. Edge files are masked off before shifting sideways.
#[inline(always)]
pub const fn north(bb: u64) -> u64 {
    bb >> 8
}
#[inline(always)]
pub const fn south(bb: u64) -> u64 {
    bb << 8
}
#[inline(always)]
pub const fn west(bb: u64) -> u64 {
    (bb & !FILE_A) >> 1
}
#[inline(always)]
pub const fn east(bb: u64) -> u64 {
    (bb & !FILE_H) << 1
}
#[inline(always)]
pub const fn north_west(bb: u64) -> u64 {
    (bb & !FILE_A) >> 9
}
#[inline(always)]
pub const fn north_east(bb: u64) -> u64 {
    (bb & !FILE_H) >> 7
}
#[inline(always)]
pub const fn south_west(bb: u64) -> u64 {
    (bb & !FILE_A) << 7
}
#[inline(always)]
pub const fn south_east(bb: u64) -> u64 {
    (bb & !FILE_H) << 9
}

/// Piece kind indices into `Board.pieces[color][kind]`. They equal the bit
/// position of the type flag in `Piece`, so `kind_idx` is a single BSF.
pub const K_KING: usize = 0;
pub const K_QUEEN: usize = 1;
pub const K_ROOK: usize = 2;
pub const K_BISHOP: usize = 3;
pub const K_KNIGHT: usize = 4;
pub const K_PAWN: usize = 5;
pub const KIND_COUNT: usize = 6;

pub const COLOR_PIECE: [Piece; 2] = [Piece::WHITE, Piece::BLACK];

#[inline(always)]
pub fn kind_idx(p: Piece) -> usize {
    (p.0 & Piece::TYPE_MASK.0).trailing_zeros() as usize
}

/// Maps WHITE (0x40) to 0 and BLACK (0x80) to 1.
#[inline(always)]
pub const fn color_idx(p: Piece) -> usize {
    (p.0 >> 7) as usize
}
