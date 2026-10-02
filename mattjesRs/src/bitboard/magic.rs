use super::bits::*;
use crate::chess::{Pos, FIELD_COUNT, HEIGHT, WIDTH};

/// Magic bitboards: for a slider on sq, the relevant occupancy (blockers on its
/// rays, edges excluded) is multiplied by a "magic" constant so that the top
/// bits form a collision-free index into a precomputed attack table.
///
/// The magics are searched when the tables are first used, with a deterministic
/// PRNG, which takes a few tens of milliseconds.
#[derive(Clone, Copy, Default)]
pub struct Magic {
    mask: u64,
    mul: u64,
    shift: u32,
    offset: usize,
}

pub struct Magics {
    rook: [Magic; FIELD_COUNT],
    bishop: [Magic; FIELD_COUNT],
    table: Vec<u64>,
}

impl Magics {
    #[inline(always)]
    pub fn rook_attacks(&self, sq: Pos, occ: u64) -> u64 {
        let m = &self.rook[sq.idx()];
        self.table[m.offset + ((occ & m.mask).wrapping_mul(m.mul) >> m.shift) as usize]
    }

    #[inline(always)]
    pub fn bishop_attacks(&self, sq: Pos, occ: u64) -> u64 {
        let m = &self.bishop[sq.idx()];
        self.table[m.offset + ((occ & m.mask).wrapping_mul(m.mul) >> m.shift) as usize]
    }

    #[inline(always)]
    pub fn queen_attacks(&self, sq: Pos, occ: u64) -> u64 {
        self.rook_attacks(sq, occ) | self.bishop_attacks(sq, occ)
    }

    pub fn table_len(&self) -> usize {
        self.table.len()
    }

    pub fn new() -> Magics {
        let mut m = Magics { rook: [Magic::default(); FIELD_COUNT], bishop: [Magic::default(); FIELD_COUNT], table: Vec::new() };

        let mut total = 0;
        for sq in 0..FIELD_COUNT {
            for (magic, dirs) in [(&mut m.rook[sq], &ROOK_DIRS), (&mut m.bishop[sq], &BISHOP_DIRS)] {
                magic.mask = slider_mask(Pos(sq as i8), dirs);
                magic.shift = 64 - magic.mask.count_ones();
                total += 1usize << magic.mask.count_ones();
            }
        }
        m.table = vec![0; total];

        let mut rng = Rng(0x9E3779B97F4A7C15);
        let mut offset = 0;
        for sq in 0..FIELD_COUNT {
            offset = find_magic(&mut m.rook[sq], &mut m.table, Pos(sq as i8), &ROOK_DIRS, offset, &mut rng);
        }
        for sq in 0..FIELD_COUNT {
            offset = find_magic(&mut m.bishop[sq], &mut m.table, Pos(sq as i8), &BISHOP_DIRS, offset, &mut rng);
        }
        m
    }
}

const ROOK_DIRS: [(i32, i32); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
const BISHOP_DIRS: [(i32, i32); 4] = [(-1, -1), (1, -1), (-1, 1), (1, 1)];

/// Walks the rays square by square. Only used to build the tables.
fn sliding_attacks_slow(sq: Pos, occ: u64, dirs: &[(i32, i32); 4]) -> u64 {
    let mut att = 0;
    for &(dx, dy) in dirs {
        let (mut x, mut y) = (sq.x() as i32 + dx, sq.y() as i32 + dy);
        while x >= 0 && x < WIDTH as i32 && y >= 0 && y < HEIGHT as i32 {
            let t = bit(Pos::from_xy(x, y));
            att |= t;
            if occ & t != 0 {
                break;
            }
            x += dx;
            y += dy;
        }
    }
    att
}

/// The attack set on an empty board without the last square of each ray:
/// a blocker on the edge cannot hide anything behind it.
fn slider_mask(sq: Pos, dirs: &[(i32, i32); 4]) -> u64 {
    let mut mask = 0;
    for &(dx, dy) in dirs {
        let (mut x, mut y) = (sq.x() as i32 + dx, sq.y() as i32 + dy);
        loop {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || nx >= WIDTH as i32 || ny < 0 || ny >= HEIGHT as i32 {
                break;
            }
            mask |= bit(Pos::from_xy(x, y));
            x = nx;
            y = ny;
        }
    }
    mask
}

/// xorshift64* with a fixed seed: the same magics on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(2685821657736338717)
    }

    /// Sparse numbers (few set bits) are far more likely to be valid magics.
    fn sparse(&mut self) -> u64 {
        self.next() & self.next() & self.next()
    }
}

/// Searches a multiplier for one square and fills its part of the attack table.
/// Returns the next free offset.
fn find_magic(m: &mut Magic, table: &mut [u64], sq: Pos, dirs: &[(i32, i32); 4], offset: usize, rng: &mut Rng) -> usize {
    let n = 1usize << (64 - m.shift);
    m.offset = offset;
    let attacks = &mut table[offset..offset + n];

    // all subsets of the mask ("carry-rippler" enumeration) and their reference attacks
    let mut occs = Vec::with_capacity(n);
    let mut refs = Vec::with_capacity(n);
    let mut occ = 0u64;
    loop {
        occs.push(occ);
        refs.push(sliding_attacks_slow(sq, occ, dirs));
        occ = occ.wrapping_sub(m.mask) & m.mask;
        if occ == 0 {
            break;
        }
    }

    let mut used = vec![0u32; n]; // epoch per slot, avoids clearing the table on every attempt
    let mut epoch = 0u32;
    loop {
        let mul = rng.sparse();
        if (m.mask.wrapping_mul(mul) >> 56).count_ones() < 6 {
            continue;
        }
        epoch += 1;
        let mut ok = true;
        for (i, &occ) in occs.iter().enumerate() {
            let idx = (occ.wrapping_mul(mul) >> m.shift) as usize;
            if used[idx] != epoch {
                used[idx] = epoch;
                attacks[idx] = refs[i];
            } else if attacks[idx] != refs[i] {
                ok = false; // destructive collision: two occupancies with different attacks
                break;
            }
        }
        if ok {
            m.mul = mul;
            return offset + n;
        }
    }
}
