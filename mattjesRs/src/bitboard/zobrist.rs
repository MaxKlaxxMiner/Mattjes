//! Zobrist hashing: every (piece, square) pair, every castling-rights combination,
//! every en passant file and the side to move get random 64-bit numbers. The key
//! of a position is the XOR of all applicable numbers, which makes it incremental:
//! moving a piece XORs out its old square and XORs in the new one.

use super::bits::*;
use super::board::Board;
use super::magic::Rng;
use super::tables::Tables;
use crate::chess::{Castling, Piece, Pos, FIELD_COUNT};

/// The key is 128 bit (two independent 64-bit sets). Milestone 2 showed that
/// 64 bit is collision-free for millions of positions, but a mate proof must
/// never rely on a false hit, so the wider key is the standard. It costs nothing
/// measurable in Rust compared with 64 bit (the two XORs vectorize).
pub const KEY_WORDS: usize = 2;

/// The Zobrist key of a position (pieces, side to move, castling, en passant;
/// halfmove clock and move number are not part of the identity).
pub type Key = [u64; KEY_WORDS];

pub struct Zobrist {
    pub piece: [[Key; FIELD_COUNT]; 12], // [color_idx*6+kind_idx][sq]
    pub castle: [Key; 16],
    pub en_passant: [Key; 9], // 0 = none, 1..8 = file a..h
    pub side: Key,            // XORed in when black is to move
}

impl Zobrist {
    pub fn new() -> Zobrist {
        let mut rng = Rng::new(0x6A09E667F3BCC908);
        let mut fill = || {
            let mut k = [0u64; KEY_WORDS];
            for w in k.iter_mut() {
                *w = rng.next();
            }
            k
        };
        let mut z = Zobrist { piece: [[[0; KEY_WORDS]; FIELD_COUNT]; 12], castle: [[0; KEY_WORDS]; 16], en_passant: [[0; KEY_WORDS]; 9], side: [0; KEY_WORDS] };
        for p in z.piece.iter_mut() {
            for k in p.iter_mut() {
                *k = fill();
            }
        }
        for k in z.castle.iter_mut() {
            *k = fill();
        }
        for k in z.en_passant.iter_mut() {
            *k = fill();
        }
        z.side = fill();
        z
    }
}

#[inline(always)]
fn ep_idx(ep: Pos) -> usize {
    if ep.valid() {
        ep.x() as usize + 1
    } else {
        0
    }
}

impl Board {
    /// Called by put and remove.
    #[inline(always)]
    pub(super) fn xor_piece(&mut self, t: &Tables, p: Piece, sq: Pos) {
        let z = &t.zobrist.piece[color_idx(p) * 6 + kind_idx(p)][sq.idx()];
        for (k, zk) in self.key.iter_mut().zip(z) {
            *k ^= zk;
        }
    }

    /// Flips castling rights and en passant between the current and the given values
    /// and toggles the side to move. XOR is symmetric, so do_move and undo_move both
    /// call it with the "other" state.
    #[inline(always)]
    pub(super) fn xor_state(&mut self, t: &Tables, other_castling: Castling, other_ep: Pos) {
        let z = &t.zobrist;
        let (zc1, zc2) = (&z.castle[self.castling as usize], &z.castle[other_castling as usize]);
        let (ze1, ze2) = (&z.en_passant[ep_idx(self.en_passant)], &z.en_passant[ep_idx(other_ep)]);
        for i in 0..KEY_WORDS {
            self.key[i] ^= zc1[i] ^ zc2[i] ^ ze1[i] ^ ze2[i] ^ z.side[i];
        }
    }

    /// Adds the non-piece components (castling, en passant, side to move) to a key
    /// that so far only contains the pieces (after from_setup or a stream decode).
    pub(super) fn finish_key(&mut self, t: &Tables) {
        let z = &t.zobrist;
        for i in 0..KEY_WORDS {
            self.key[i] ^= z.castle[self.castling as usize][i] ^ z.en_passant[ep_idx(self.en_passant)][i];
            if !self.white_move {
                self.key[i] ^= z.side[i];
            }
        }
    }

    /// Recomputes the key from scratch. Used to verify the incremental updates.
    pub fn zobrist_full(&self, t: &Tables) -> Key {
        let z = &t.zobrist;
        let mut k = [0u64; KEY_WORDS];
        for (sq, &p) in self.squares.iter().enumerate() {
            if p != Piece::NONE {
                let zp = &z.piece[color_idx(p) * 6 + kind_idx(p)][sq];
                for (kw, zw) in k.iter_mut().zip(zp) {
                    *kw ^= zw;
                }
            }
        }
        for (i, kw) in k.iter_mut().enumerate() {
            *kw ^= z.castle[self.castling as usize][i] ^ z.en_passant[ep_idx(self.en_passant)][i];
            if !self.white_move {
                *kw ^= z.side[i];
            }
        }
        k
    }
}
