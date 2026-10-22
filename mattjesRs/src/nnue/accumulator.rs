//! The hidden layer before the activation, one vector per perspective, and
//! its incremental update per move (port of `mattjesGo/nnue/accumulator.go`).
//! The row additions and the output dot product run on AVX2 when the CPU
//! has it (`std::arch`, detected at run time), else in plain loops; both
//! paths give the same result: the additions saturate in both, and the
//! output weights lie in -126..126, so the int16 products of the SIMD path
//! (at most 255 * 126) never wrap.

use super::{feature_indices, Net, HIDDEN_SIZE, QA, QB, SCALE};
use crate::bitboard::Board;
use crate::chess::{Move, Piece, Pos, WIDTH};

/// The bias plus the feature rows of every piece on the board, for white's
/// and for black's perspective. A move changes at most two features per
/// side (castling: king and rook) and removes at most two (capture or en
/// passant), so the next accumulator is the old one plus and minus a few
/// rows (`update`) instead of a sum over all pieces (`refresh`). The
/// additions saturate at the int16 range, as Patricia's SIMD path does.
#[derive(Clone, PartialEq, Eq)]
#[repr(C, align(32))]
pub struct Accumulator {
    pub white: [i16; HIDDEN_SIZE],
    pub black: [i16; HIDDEN_SIZE],
}

impl Default for Accumulator {
    fn default() -> Self {
        Accumulator { white: [0; HIDDEN_SIZE], black: [0; HIDDEN_SIZE] }
    }
}

/// The features a move adds and removes, per perspective.
#[derive(Clone, Copy, Default, Debug)]
pub struct Delta {
    pub add_white: [u16; 2],
    pub add_black: [u16; 2],
    pub sub_white: [u16; 2],
    pub sub_black: [u16; 2],
    pub adds: u8,
    pub subs: u8,
}

impl Delta {
    fn add(&mut self, p: Piece, sq: Pos) {
        let (w, b) = feature_indices(p, sq);
        self.add_white[self.adds as usize] = w as u16;
        self.add_black[self.adds as usize] = b as u16;
        self.adds += 1;
    }

    fn sub(&mut self, p: Piece, sq: Pos) {
        let (w, b) = feature_indices(p, sq);
        self.sub_white[self.subs as usize] = w as u16;
        self.sub_black[self.subs as usize] = b as u16;
        self.subs += 1;
    }

    /// The feature change of move m on board b (b before the move): the
    /// moved piece leaves its square and arrives (as the promotion piece if
    /// any), a captured piece leaves (the en passant pawn from its own
    /// square), a castling rook moves too.
    pub fn of(b: &Board, m: Move) -> Delta {
        let mut d = Delta::default();
        let p = b.squares[m.from.idx()];
        d.sub(p, m.from);
        if m.capture != Piece::NONE {
            d.sub(m.capture, m.to);
        }
        if m.promo != Piece::NONE {
            d.add(m.promo, m.to);
        } else {
            d.add(p, m.to);
        }
        if p.is(Piece::PAWN) {
            if m.to == b.en_passant && m.from.x() != m.to.x() {
                let sq = if b.white_move { m.to + WIDTH } else { m.to - WIDTH };
                d.sub(b.squares[sq.idx()], sq);
            }
        } else if p.is(Piece::KING) {
            match (m.to - m.from) as i32 {
                2 => {
                    let rook = b.squares[(m.to + 1).idx()];
                    d.sub(rook, m.to + 1);
                    d.add(rook, m.to - 1);
                }
                -2 => {
                    let rook = b.squares[(m.to - 2).idx()];
                    d.sub(rook, m.to - 2);
                    d.add(rook, m.to + 1);
                }
                _ => {}
            }
        }
        d
    }
}

/// Whether the AVX2 path is used (detected once; the environment variable
/// MATTJES_NNUE_SCALAR forces the plain loops, for the measurement).
pub fn simd() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        use std::sync::OnceLock;
        static AVX2: OnceLock<bool> = OnceLock::new();
        *AVX2.get_or_init(|| std::env::var_os("MATTJES_NNUE_SCALAR").is_none() && is_x86_feature_detected!("avx2"))
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

fn add_row(acc: &mut [i16; HIDDEN_SIZE], row: &[i16]) {
    #[cfg(target_arch = "x86_64")]
    if simd() {
        // SAFETY: AVX2 was detected; the slices have HIDDEN_SIZE elements.
        unsafe { avx2::add_row(acc, row) };
        return;
    }
    for (a, &r) in acc.iter_mut().zip(row) {
        *a = a.saturating_add(r);
    }
}

fn sub_row(acc: &mut [i16; HIDDEN_SIZE], row: &[i16]) {
    #[cfg(target_arch = "x86_64")]
    if simd() {
        // SAFETY: as in add_row.
        unsafe { avx2::sub_row(acc, row) };
        return;
    }
    for (a, &r) in acc.iter_mut().zip(row) {
        *a = a.saturating_sub(r);
    }
}

fn screlu(x: i16) -> i64 {
    let v = (x as i32).clamp(0, QA) as i64;
    v * v
}

/// The output sum of both perspectives: SCReLU of the side to move's
/// accumulator against the first half of the output weights, the opponent's
/// against the second.
fn dot(us: &[i16; HIDDEN_SIZE], them: &[i16; HIDDEN_SIZE], weights: &[i16]) -> i64 {
    #[cfg(target_arch = "x86_64")]
    if simd() {
        // SAFETY: AVX2 was detected; weights has 2 * HIDDEN_SIZE elements.
        return unsafe { avx2::dot(us, them, weights) };
    }
    let mut sum: i64 = 0;
    for i in 0..HIDDEN_SIZE {
        sum += screlu(us[i]) * weights[i] as i64;
        sum += screlu(them[i]) * weights[HIDDEN_SIZE + i] as i64;
    }
    sum
}

impl Accumulator {
    /// Computes both perspectives from scratch for board b.
    pub fn refresh(&mut self, net: &Net, b: &Board) {
        self.white.copy_from_slice(&net.bias);
        self.black.copy_from_slice(&net.bias);
        for (sq, &p) in b.squares.iter().enumerate() {
            if p == Piece::NONE {
                continue;
            }
            let (w, bl) = feature_indices(p, Pos(sq as i8));
            add_row(&mut self.white, net.row(w));
            add_row(&mut self.black, net.row(bl));
        }
    }

    /// Sets self to src plus the delta (the additions first, then the
    /// subtractions, each saturating, as Patricia's SIMD path does).
    pub fn update(&mut self, net: &Net, src: &Accumulator, d: &Delta) {
        self.white = src.white;
        self.black = src.black;
        for i in 0..d.adds as usize {
            add_row(&mut self.white, net.row(d.add_white[i] as usize));
            add_row(&mut self.black, net.row(d.add_black[i] as usize));
        }
        for i in 0..d.subs as usize {
            sub_row(&mut self.white, net.row(d.sub_white[i] as usize));
            sub_row(&mut self.black, net.row(d.sub_black[i] as usize));
        }
    }

    /// The network's output for the side to move in centipawns (Patricia's
    /// raw score before its eval() scaling): the side to move's perspective
    /// through the first half of the output weights, the opponent's through
    /// the second.
    pub fn evaluate(&self, net: &Net, white_to_move: bool) -> i32 {
        let (us, them) = if white_to_move { (&self.white, &self.black) } else { (&self.black, &self.white) };
        let sum = dot(us, them, &net.output);
        let biased = sum + net.output_bias as i64 * QA as i64;
        ((biased * SCALE) / (QA as i64 * (QA * QB) as i64)) as i32
    }
}

#[cfg(target_arch = "x86_64")]
mod avx2 {
    //! The AVX2 kernels: 16 lanes of int16 per register; the output dot
    //! product as in Patricia (clamp, int16 multiply, madd to int32 pairs,
    //! horizontal sum), with int32 accumulators that are summed in int64.
    use super::{HIDDEN_SIZE, QA};
    use std::arch::x86_64::*;

    const LANES: usize = 16;

    /// # Safety
    /// AVX2 must be available; acc and row have HIDDEN_SIZE elements.
    #[target_feature(enable = "avx2")]
    pub unsafe fn add_row(acc: &mut [i16; HIDDEN_SIZE], row: &[i16]) {
        for i in (0..HIDDEN_SIZE).step_by(LANES) {
            let a = _mm256_loadu_si256(acc.as_ptr().add(i) as *const __m256i);
            let r = _mm256_loadu_si256(row.as_ptr().add(i) as *const __m256i);
            _mm256_storeu_si256(acc.as_mut_ptr().add(i) as *mut __m256i, _mm256_adds_epi16(a, r));
        }
    }

    /// # Safety
    /// As in add_row.
    #[target_feature(enable = "avx2")]
    pub unsafe fn sub_row(acc: &mut [i16; HIDDEN_SIZE], row: &[i16]) {
        for i in (0..HIDDEN_SIZE).step_by(LANES) {
            let a = _mm256_loadu_si256(acc.as_ptr().add(i) as *const __m256i);
            let r = _mm256_loadu_si256(row.as_ptr().add(i) as *const __m256i);
            _mm256_storeu_si256(acc.as_mut_ptr().add(i) as *mut __m256i, _mm256_subs_epi16(a, r));
        }
    }

    /// # Safety
    /// AVX2 must be available; weights has 2 * HIDDEN_SIZE elements.
    #[target_feature(enable = "avx2")]
    pub unsafe fn dot(us: &[i16; HIDDEN_SIZE], them: &[i16; HIDDEN_SIZE], weights: &[i16]) -> i64 {
        let zero = _mm256_setzero_si256();
        let qa = _mm256_set1_epi16(QA as i16);
        let mut sum0 = _mm256_setzero_si256();
        let mut sum1 = _mm256_setzero_si256();
        let mut total: i64 = 0;
        for i in (0..HIDDEN_SIZE).step_by(LANES) {
            let a = _mm256_loadu_si256(us.as_ptr().add(i) as *const __m256i);
            let v = _mm256_min_epi16(_mm256_max_epi16(a, zero), qa);
            let w = _mm256_loadu_si256(weights.as_ptr().add(i) as *const __m256i);
            sum0 = _mm256_add_epi32(sum0, _mm256_madd_epi16(_mm256_mullo_epi16(v, w), v));
            let a = _mm256_loadu_si256(them.as_ptr().add(i) as *const __m256i);
            let v = _mm256_min_epi16(_mm256_max_epi16(a, zero), qa);
            let w = _mm256_loadu_si256(weights.as_ptr().add(HIDDEN_SIZE + i) as *const __m256i);
            sum1 = _mm256_add_epi32(sum1, _mm256_madd_epi16(_mm256_mullo_epi16(v, w), v));
            if i % 256 == 256 - LANES {
                // drain into int64 every 16 steps: 16 lanes x 16 steps x 2
                // products x 255 x 255 x 126 stays far below 2^31
                total += hsum(sum0) + hsum(sum1);
                sum0 = _mm256_setzero_si256();
                sum1 = _mm256_setzero_si256();
            }
        }
        total + hsum(sum0) + hsum(sum1)
    }

    #[target_feature(enable = "avx2")]
    unsafe fn hsum(v: __m256i) -> i64 {
        let mut lanes = [0i32; 8];
        _mm256_storeu_si256(lanes.as_mut_ptr() as *mut __m256i, v);
        lanes.iter().map(|&x| x as i64).sum()
    }
}
