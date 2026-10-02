//! Compact position encoding for storing many positions (position lists, later
//! hash/persistence experiments): the 8-byte occupancy followed by one nibble
//! (color*6 + kind, 0..11) per occupied square in bit order, then 6 flag bytes
//! (side+castling, en passant, halfmove clock, move number).
//!
//! `Packed` is variable-length (19 bytes for 6 pieces, 30 for the start position,
//! which is the maximum because at most 32 pieces = 16 nibble bytes exist).
//! `PackedFixed` pads the same data to a constant 32-byte record for index-based
//! access. Milestone 1 measured variable as ~10 % faster for sequential streams.

use super::bits::*;
use super::board::Board;
use super::tables::{Tables, TABLES};
use crate::chess::{Castling, Piece, Pos, ALL_CASTLING};

const FLAG_BYTES: usize = 6;
const MAX_PIECES: usize = 32;
const FIXED_FLAGS_OFFSET: usize = 8 + MAX_PIECES / 2;

/// A compact encoding, selected at compile time (monomorphized) by `perft_breadth_encoded`.
pub trait Codec {
    const NAME: &'static str;
    /// Upper bound of one record in bytes.
    const MAX_BYTES: usize;
    fn append(b: &Board, dst: &mut Vec<u8>);
    /// Reads one position from the start of `src` and returns it with the bytes consumed.
    fn decode(src: &[u8]) -> (Board, usize);
}

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

/// Reads occupancy and nibbles; returns the board (flags not yet set) and the bytes consumed.
fn decode_packed_pieces(t: &Tables, src: &[u8]) -> (Board, usize) {
    let mut b = Board::empty();
    let occ = u64::from_le_bytes(src[..8].try_into().unwrap());
    let mut i = 8;
    let mut bb = occ;
    while bb != 0 {
        let code = src[i];
        i += 1;
        b.put(t, pop_lsb(&mut bb), NIBBLE_PIECE[(code & 15) as usize]);
        if bb != 0 {
            b.put(t, pop_lsb(&mut bb), NIBBLE_PIECE[(code >> 4) as usize]);
        }
    }
    (b, i)
}

/// Variable-length packed encoding.
pub struct Packed;

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
        let t: &Tables = &TABLES;
        let (mut b, mut i) = decode_packed_pieces(t, src);
        i += read_flags(t, &mut b, &src[i..]);
        (b, i)
    }
}

/// The packed encoding in a constant 32-byte record: 8 bytes occupancy, 16 nibble
/// bytes (zero padded), 6 flag bytes, 2 bytes padding. Two records per 64-byte
/// cache line; records can be addressed by index.
pub struct PackedFixed;

/// One PackedFixed record as a stack array (no allocation).
pub fn packed_fixed_record(b: &Board) -> [u8; PackedFixed::MAX_BYTES] {
    let mut rec = [0u8; PackedFixed::MAX_BYTES];
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
    rec
}

impl Codec for PackedFixed {
    const NAME: &'static str = "packed-fixed";
    const MAX_BYTES: usize = 32;

    fn append(b: &Board, dst: &mut Vec<u8>) {
        dst.extend_from_slice(&packed_fixed_record(b));
    }

    fn decode(src: &[u8]) -> (Board, usize) {
        let t: &Tables = &TABLES;
        let (mut b, _) = decode_packed_pieces(t, src);
        read_flags(t, &mut b, &src[FIXED_FLAGS_OFFSET..]);
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

/// Completes a decoded board: flags and the state part of the Zobrist key.
fn read_flags(t: &Tables, b: &mut Board, src: &[u8]) -> usize {
    b.white_move = src[0] & 1 != 0;
    b.castling = (src[0] >> 1) as Castling & ALL_CASTLING;
    b.en_passant = Pos(src[1] as i8);
    b.halfmove_clock = u16::from_le_bytes([src[2], src[3]]);
    b.move_number = u16::from_le_bytes([src[4], src[5]]);
    b.finish_key(t);
    FLAG_BYTES
}
