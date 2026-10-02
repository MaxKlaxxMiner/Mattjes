//! Compact position encodings for storing many positions (position lists,
//! later hash/persistence experiments). Both are variable-length and end with
//! the same 6 flag bytes: side+castling, en passant, halfmove clock, move number.

use super::bits::*;
use super::board::Board;
use crate::chess::{Castling, Piece, Pos, ALL_CASTLING, FIELD_COUNT};

const FLAG_BYTES: usize = 6;

/// A compact encoding, selected at compile time (monomorphized) by `perft_breadth_encoded`.
pub trait Codec {
    const NAME: &'static str;
    /// Upper bound of one record in bytes.
    const MAX_BYTES: usize;
    fn append(b: &Board, dst: &mut Vec<u8>);
    /// Reads one position from the start of `src` and returns it with the bytes consumed.
    fn decode(src: &[u8]) -> (Board, usize);
}

/// The run-length encoding from the old C# code (GetFastFen): one byte per piece
/// (always >= 0x41) and one count byte (< 64) per gap of empty squares.
/// Start position 39 bytes, typical middlegame ~50, late endgame ~20.
pub struct FastFen;

impl Codec for FastFen {
    const NAME: &'static str = "fastfen";
    const MAX_BYTES: usize = FIELD_COUNT + FLAG_BYTES;

    fn append(b: &Board, dst: &mut Vec<u8>) {
        let mut gap = 0u8;
        for &p in &b.squares {
            if p == Piece::NONE {
                gap += 1;
                continue;
            }
            if gap > 0 {
                dst.push(gap);
                gap = 0;
            }
            dst.push(p.0);
        }
        if gap > 0 {
            dst.push(gap);
        }
        append_flags(b, dst);
    }

    fn decode(src: &[u8]) -> (Board, usize) {
        let mut b = Board::empty();
        let mut i = 0;
        let mut sq = 0usize;
        while sq < FIELD_COUNT {
            let c = src[i];
            i += 1;
            if c < 64 {
                sq += c as usize;
                continue;
            }
            b.put(Pos(sq as i8), Piece(c));
            sq += 1;
        }
        i += read_flags(&mut b, &src[i..]);
        (b, i)
    }
}

/// The usual bitboard encoding: the 8-byte occupancy followed by one nibble
/// (color*6 + kind, 0..11) per occupied square in bit order.
/// At most 32 pieces = 16 nibble bytes, so the start position (30 bytes) is the
/// maximum; 6 pieces take 17 bytes.
pub struct Packed;

const MAX_PIECES: usize = 32;

const NIBBLE_PIECE: [Piece; 12] = [
    Piece::WHITE_KING,
    Piece::WHITE_QUEEN,
    Piece::WHITE_ROOK,
    Piece::WHITE_BISHOP,
    Piece::WHITE_KNIGHT,
    Piece::WHITE_PAWN,
    Piece::BLACK_KING,
    Piece::BLACK_QUEEN,
    Piece::BLACK_ROOK,
    Piece::BLACK_BISHOP,
    Piece::BLACK_KNIGHT,
    Piece::BLACK_PAWN,
];

#[inline(always)]
fn nibble(p: Piece) -> u8 {
    (color_idx(p) * 6 + kind_idx(p)) as u8
}

impl Codec for Packed {
    const NAME: &'static str = "packed";
    const MAX_BYTES: usize = 8 + MAX_PIECES / 2 + FLAG_BYTES;

    fn append(b: &Board, dst: &mut Vec<u8>) {
        let occ = b.occupied();
        dst.extend_from_slice(&occ.to_le_bytes());
        let mut bb = occ;
        while bb != 0 {
            let mut code = nibble(b.squares[pop_lsb(&mut bb).idx()]);
            if bb != 0 {
                code |= nibble(b.squares[pop_lsb(&mut bb).idx()]) << 4;
            }
            dst.push(code);
        }
        append_flags(b, dst);
    }

    fn decode(src: &[u8]) -> (Board, usize) {
        let mut b = Board::empty();
        let occ = u64::from_le_bytes(src[..8].try_into().unwrap());
        let mut i = 8;
        let mut bb = occ;
        while bb != 0 {
            let code = src[i];
            i += 1;
            b.put(pop_lsb(&mut bb), NIBBLE_PIECE[(code & 15) as usize]);
            if bb != 0 {
                b.put(pop_lsb(&mut bb), NIBBLE_PIECE[(code >> 4) as usize]);
            }
        }
        i += read_flags(&mut b, &src[i..]);
        (b, i)
    }
}

/// The packed encoding in a constant 32-byte record: 8 bytes occupancy, 16 nibble
/// bytes (zero padded), 6 flag bytes, 2 bytes padding. Two records per 64-byte
/// cache line; records can be addressed by index.
pub struct PackedFixed;

const FIXED_FLAGS_OFFSET: usize = 8 + MAX_PIECES / 2;

impl Codec for PackedFixed {
    const NAME: &'static str = "packed-fixed";
    const MAX_BYTES: usize = 32;

    fn append(b: &Board, dst: &mut Vec<u8>) {
        let mut rec = [0u8; Self::MAX_BYTES];
        let occ = b.occupied();
        rec[..8].copy_from_slice(&occ.to_le_bytes());
        let mut i = 8;
        let mut bb = occ;
        while bb != 0 {
            let mut code = nibble(b.squares[pop_lsb(&mut bb).idx()]);
            if bb != 0 {
                code |= nibble(b.squares[pop_lsb(&mut bb).idx()]) << 4;
            }
            rec[i] = code;
            i += 1;
        }
        let f = &mut rec[FIXED_FLAGS_OFFSET..];
        f[0] = (b.castling << 1) | b.white_move as u8;
        f[1] = b.en_passant.0 as u8;
        f[2..4].copy_from_slice(&b.halfmove_clock.to_le_bytes());
        f[4..6].copy_from_slice(&b.move_number.to_le_bytes());
        dst.extend_from_slice(&rec);
    }

    fn decode(src: &[u8]) -> (Board, usize) {
        let (mut b, _) = Packed::decode(src);
        read_flags(&mut b, &src[FIXED_FLAGS_OFFSET..]);
        (b, Self::MAX_BYTES)
    }
}

fn append_flags(b: &Board, dst: &mut Vec<u8>) {
    let flags = (b.castling << 1) | b.white_move as u8;
    dst.extend_from_slice(&[
        flags,
        b.en_passant.0 as u8,
        b.halfmove_clock as u8,
        (b.halfmove_clock >> 8) as u8,
        b.move_number as u8,
        (b.move_number >> 8) as u8,
    ]);
}

fn read_flags(b: &mut Board, src: &[u8]) -> usize {
    b.white_move = src[0] & 1 != 0;
    b.castling = (src[0] >> 1) as Castling & ALL_CASTLING;
    b.en_passant = Pos(src[1] as i8);
    b.halfmove_clock = u16::from_le_bytes([src[2], src[3]]);
    b.move_number = u16::from_le_bytes([src[4], src[5]]);
    FLAG_BYTES
}
