//! Non-incremental keys for comparison with Zobrist.

use super::board::Board;
use super::encode::{packed_fixed_record, Codec, PackedFixed};
use super::tables::TABLES;

/// CRC64 start value and multiplier as used by yacboard and the old C# code
/// (this is FNV-1a: xor the byte, then multiply by the FNV prime).
pub const CRC64_START: u64 = 0xcbf29ce484222325;
pub const CRC64_MUL: u64 = 0x100000001b3;

/// A collision-free 32-byte identity of the position: the `PackedFixed` record
/// with halfmove clock and move number zeroed.
pub type ExactKey = [u8; PackedFixed::MAX_BYTES];

impl Board {
    /// Hashes the 64 squares, side to move, castling rights and en passant square
    /// from scratch, exactly like yacboard's checksum (without move counters).
    pub fn crc64(&self) -> u64 {
        let mut h = CRC64_START;
        for p in &self.squares {
            h = (h ^ p.0 as u64).wrapping_mul(CRC64_MUL);
        }
        h = (h ^ self.white_move as u64).wrapping_mul(CRC64_MUL);
        h = (h ^ self.castling as u64).wrapping_mul(CRC64_MUL);
        h = (h ^ self.en_passant.0 as u8 as u64).wrapping_mul(CRC64_MUL);
        h
    }

    pub fn exact_key(&self) -> ExactKey {
        if self.halfmove_clock != 0 || self.move_number != 0 {
            let mut c = *self;
            c.halfmove_clock = 0;
            c.move_number = 0;
            packed_fixed_record(&c)
        } else {
            packed_fixed_record(self)
        }
    }

    /// Restores the position from an exact key (move counters are zero).
    pub fn from_exact_key(k: &ExactKey) -> Board {
        let _ = &*TABLES;
        PackedFixed::decode(k).0
    }
}

/// splitmix64 finalizer: spreads the bits of x. Used to turn a key with poor
/// low-bit distribution into a table index.
#[inline(always)]
pub fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58476d1ce4e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d049bb133111eb);
    x ^= x >> 31;
    x
}

/// Folds the 32 exact bytes into 64 bits.
pub fn exact_hash(k: &ExactKey) -> u64 {
    let mut h = 0u64;
    for chunk in k.chunks_exact(8) {
        h = mix64(h ^ u64::from_le_bytes(chunk.try_into().unwrap()));
    }
    h
}
