//! The inverse of `encode` for one encoding: index -> squares (the port of
//! mattjesGo/syzygy/decode.go). The index is a mixed-radix number: the
//! leading group's value times its factor plus every other group's
//! combination rank (colex order over the squares the earlier listed pieces
//! leave free) times its factor. The leading group is inverted by a table
//! enumerated once per shape, the other groups by colex unranking.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::encode::{self, EncInfo, Shape, DIAG, ENC_FILE, ENC_PIECE, FLAP, FLIP_DIAG, INDICES, KK_IDX, LOWER, OFF_DIAG, PAWN_TWIST, TRIANGLE};
use super::BaseEntry;

/// Inverts the index of one encoding (an order, a pawn file class).
pub struct Decoder<'a> {
    ei: &'a EncInfo,
    num: usize,
    lead_factor: u64,
    lead_size: u64,
    lead: Arc<Vec<Vec<usize>>>,
    groups: Vec<DecGroup>,
}

/// A group of like pieces after the leading group.
struct DecGroup {
    start: usize,
    count: usize,
    factor: u64,
    size: u64,
    lo: usize,
    hi: usize,
}

/// The leading-group inversions per shape (kk, triple, pawns with their
/// count and file class), computed once.
type LeadTable = Arc<Vec<Vec<usize>>>;

fn lead_tables() -> &'static Mutex<HashMap<String, LeadTable>> {
    static TABLES: OnceLock<Mutex<HashMap<String, LeadTable>>> = OnceLock::new();
    TABLES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lead_table(key: &str, build: impl FnOnce() -> Vec<Vec<usize>>) -> Arc<Vec<Vec<usize>>> {
    if let Some(t) = lead_tables().lock().unwrap().get(key) {
        return t.clone();
    }
    let t = Arc::new(build());
    lead_tables().lock().unwrap().insert(key.to_string(), t.clone());
    t
}

impl<'a> Decoder<'a> {
    /// The decoder of an encoding; t is the file class of a pawn table.
    pub fn new(be: &BaseEntry, ei: &'a EncInfo, enc: usize, t: usize) -> Decoder<'a> {
        let ix = &*INDICES;
        let shape = &be.shape;
        let lead_k = ei.norm[0] as usize;
        let (lead_size, lead) = if enc != ENC_PIECE {
            (ix.pawn_factor_file[lead_k - 1][t], lead_table(&format!("pawns{}/{}", lead_k, t), || enumerate_pawn_lead(shape, lead_k, t)))
        } else if shape.kk_enc {
            (462, lead_table("kk", || enumerate_piece_lead(2)))
        } else {
            (31332, lead_table("triple", || enumerate_piece_lead(3)))
        };
        let mut d = Decoder { ei, num: shape.num, lead_factor: ei.factor[0], lead_size, lead, groups: Vec::new() };
        let mut k = lead_k;
        let mut n = 64 - k;
        if enc != ENC_PIECE && shape.pawns[1] != 0 {
            let count = ei.norm[k] as usize;
            d.groups.push(DecGroup { start: k, count, factor: ei.factor[k], size: encode::subfactor(count as u64, (48 - k) as u64), lo: 8, hi: 56 });
            k += count;
            n = 64 - k;
        }
        while k < shape.num {
            let count = ei.norm[k] as usize;
            d.groups.push(DecGroup { start: k, count, factor: ei.factor[k], size: encode::subfactor(count as u64, n as u64), lo: 0, hi: 64 });
            n -= count;
            k += count;
        }
        d
    }

    /// Fills p (Syzygy squares, in the list order) for an index.
    pub fn squares(&self, idx: u64, p: &mut [usize]) {
        let ix = &*INDICES;
        let lead = &self.lead[((idx / self.lead_factor) % self.lead_size) as usize];
        p[..lead.len()].copy_from_slice(lead);
        for g in &self.groups {
            let mut v = (idx / g.factor) % g.size;
            // colex unranking: the i-th rank square (1-based, ascending) is
            // the largest r with C(r, i) <= v
            let mut ranks = [0usize; 6];
            for i in (1..=g.count).rev() {
                let mut r = i - 1;
                while r + 1 < 64 && ix.binomial[i][r + 1] <= v {
                    r += 1;
                }
                ranks[i - 1] = r;
                v -= ix.binomial[i][r];
            }
            // the r-th square the earlier listed pieces leave free
            for i in 0..g.count {
                let mut r = ranks[i];
                let mut sq = g.lo;
                while sq < g.hi {
                    let taken = p[..g.start].contains(&sq);
                    if !taken {
                        if r == 0 {
                            break;
                        }
                        r -= 1;
                    }
                    sq += 1;
                }
                p[g.start + i] = sq;
            }
        }
        let _ = self.num;
    }
}

/// Applies encode's mirrors and the diagonal flip to the leading squares
/// alone (they decide the flips).
fn canonical_lead(q: &mut [usize], k: usize) {
    if q[0] & 4 != 0 {
        for v in q.iter_mut() {
            *v ^= 7;
        }
    }
    if q[0] & 0x20 != 0 {
        for v in q.iter_mut() {
            *v ^= 0x38;
        }
    }
    for i in 0..k {
        if OFF_DIAG[q[i]] != 0 {
            if OFF_DIAG[q[i]] > 0 {
                for v in q.iter_mut() {
                    *v = FLIP_DIAG[*v] as usize;
                }
            }
            break;
        }
    }
}

/// The leading group's value of canonical squares (encode's formulas), -1
/// for an impossible king pair.
fn piece_lead_value(p: &[usize], k: usize) -> i64 {
    if k == 2 {
        return KK_IDX[TRIANGLE[p[0]] as usize][p[1]] as i64;
    }
    let s1 = (p[1] > p[0]) as usize;
    let s2 = (p[2] > p[0]) as usize + (p[2] > p[1]) as usize;
    let v = if OFF_DIAG[p[0]] != 0 {
        TRIANGLE[p[0]] as usize * 63 * 62 + (p[1] - s1) * 62 + (p[2] - s2)
    } else if OFF_DIAG[p[1]] != 0 {
        6 * 63 * 62 + DIAG[p[0]] as usize * 28 * 62 + LOWER[p[1]] as usize * 62 + (p[2] - s2)
    } else if OFF_DIAG[p[2]] != 0 {
        6 * 63 * 62 + 4 * 28 * 62 + DIAG[p[0]] as usize * 7 * 28 + (DIAG[p[1]] as usize - s1) * 28 + LOWER[p[2]] as usize
    } else {
        6 * 63 * 62 + 4 * 28 * 62 + 4 * 7 * 28 + DIAG[p[0]] as usize * 7 * 6 + (DIAG[p[1]] as usize - s1) * 6 + (DIAG[p[2]] as usize - s2)
    };
    v as i64
}

/// Inverts the leading group of a pawnless table (the two kings, or three
/// single pieces): every placement is canonicalised as encode does it and
/// the first placement of every value is kept.
fn enumerate_piece_lead(k: usize) -> Vec<Vec<usize>> {
    let size = if k == 2 { 462 } else { 31332 };
    let mut table: Vec<Vec<usize>> = vec![Vec::new(); size];
    let mut q = [0usize; 3];
    fn walk(i: usize, k: usize, q: &mut [usize; 3], table: &mut Vec<Vec<usize>>) {
        if i == k {
            let mut c = q[..k].to_vec();
            canonical_lead(&mut c, k);
            let v = piece_lead_value(&c, k);
            if v >= 0 && table[v as usize].is_empty() {
                table[v as usize] = c;
            }
            return;
        }
        for s in 0..64 {
            if !q[..i].contains(&s) {
                q[i] = s;
                walk(i + 1, k, q, table);
            }
        }
    }
    walk(0, k, &mut q, &mut table);
    for (v, sq) in table.iter().enumerate() {
        assert!(!sq.is_empty(), "syzygy: leading value {} of {} has no placement", v, size);
    }
    table
}

/// Inverts the leading pawns of a pawn table for one file class: every
/// placement is ordered as the probe orders it (the pawn with the smallest
/// flap first, mirrored to the files a-d, the rest by twist) and the first
/// placement of every value is kept.
fn enumerate_pawn_lead(shape: &Shape, k: usize, t: usize) -> Vec<Vec<usize>> {
    let ix = &*INDICES;
    let size = ix.pawn_factor_file[k - 1][t] as usize;
    let mut table: Vec<Vec<usize>> = vec![Vec::new(); size];
    let mut q = [0usize; 6];
    fn walk(i: usize, k: usize, t: usize, shape: &Shape, q: &mut [usize; 6], table: &mut Vec<Vec<usize>>) {
        let ix = &*INDICES;
        if i == k {
            let mut p = q[..k].to_vec();
            if encode::leading_pawn(&mut p, shape, ENC_FILE) != t {
                return;
            }
            if p[0] & 4 != 0 {
                for v in p.iter_mut() {
                    *v ^= 7;
                }
            }
            for a in 1..k {
                for b in a + 1..k {
                    if PAWN_TWIST[0][p[a]] < PAWN_TWIST[0][p[b]] {
                        p.swap(a, b);
                    }
                }
            }
            let mut v = ix.pawn_idx[0][k - 1][FLAP[0][p[0]] as usize] as usize;
            for a in 1..k {
                v += ix.binomial[k - a][PAWN_TWIST[0][p[a]] as usize] as usize;
            }
            if table[v].is_empty() {
                table[v] = p;
            }
            return;
        }
        for s in 8..56 {
            if !q[..i].contains(&s) {
                q[i] = s;
                walk(i + 1, k, t, shape, q, table);
            }
        }
    }
    walk(0, k, t, shape, &mut q, &mut table);
    for (v, sq) in table.iter().enumerate() {
        assert!(!sq.is_empty(), "syzygy: pawn leading value {} of {} (class {}) has no placement", v, size, t);
    }
    table
}

/// Round-trips random indices of up to max_orders candidate orders of a
/// material (every pawn file class) through the decoder and encode;
/// returns the indices checked and the mismatches (the first few reported).
pub fn check_decoder(m: &crate::egtb::Material, max_orders: usize, samples: usize, seed: u64, report: &dyn Fn(&str)) -> (usize, usize) {
    use super::write::{candidates, counts_of};
    let (first, second) = counts_of(m);
    let be = BaseEntry::describe(super::name_of(&first, &second), &first, &second).expect("pieces");
    let (enc, files) = if be.has_pawns { (ENC_FILE, 4) } else { (ENC_PIECE, 1) };
    let mut cands = candidates(&be, &first, &second);
    cands.truncate(max_orders);
    let mut x = seed;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let (mut checked, mut bad) = (0, 0);
    for o in &cands {
        for f in 0..files {
            let mut ei = EncInfo::default();
            ei.pieces[..o.pieces.len()].copy_from_slice(&o.pieces);
            let size = encode::setup(&mut ei, &be.shape, o.order, o.order2, f, enc);
            let d = Decoder::new(&be, &ei, enc, f);
            let mut p = [0usize; encode::MAX_PIECES];
            for _ in 0..samples {
                let idx = next() % size;
                d.squares(idx, &mut p[..be.shape.num]);
                let mut q = p;
                let back = encode::encode(&mut q[..be.shape.num], &ei, &be.shape, enc);
                checked += 1;
                if back != idx {
                    bad += 1;
                    if bad <= 5 {
                        report(&format!("  {} order {} file {}: index {} decodes to {:?}, encodes to {}", m.name(), o, f, idx, &p[..be.shape.num], back));
                    }
                }
            }
        }
    }
    (checked, bad)
}
