//! The own endgame tables of milestone 5 (working title, "endgame tablebase"):
//! one byte per position for every material with up to four pieces, computed
//! by retrograde analysis and kept in RAM. Direct port of `mattjesGo/egtb`;
//! the design is explained in docs/m5-endgame-tables-design.md.

mod control;
mod generate;
mod index;
mod peak;
mod persist;
mod requirements;
mod table;

#[allow(unused_imports)]
pub use control::Control;
#[allow(unused_imports)]
pub use peak::{available_memory, format_bytes, peak_memory};
#[allow(unused_imports)]
pub use requirements::{append_log, requirement, Requirement, LOG_FILE_NAME, REQUIREMENTS};
#[allow(unused_imports)]
pub use generate::{group, Progress, Stats, KNOWN_MAXIMA};
#[allow(unused_imports)]
pub use index::{KK_PAWNLESS, KK_PAWNS};
#[allow(unused_imports)]
pub use persist::{checksum, default_cache_dir, default_path, recorded_checksum, BASE_FILE_NAME, CACHE_DIR, FILE_CHECKSUM, IO_WORKERS, TABLE_CHECKSUMS};
#[allow(unused_imports)]
pub use table::{Set, Table, Value, MAX_PIECES, MAX_PLIES};

use crate::bitboard::Board;
use crate::chess::Piece;

/// The set of pieces besides the two kings. By convention White is the stronger
/// side (more pieces, then the higher piece), so KQKR exists and KRKQ is looked
/// up with swapped colors. Types are sorted queen first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Material {
    /// Piece types without color.
    pub white: Vec<Piece>,
    pub black: Vec<Piece>,
}

/// Ranks the types for sorting and for the "stronger side" rule.
pub fn piece_order(p: Piece) -> i32 {
    match p.kind() {
        Piece::QUEEN => 5,
        Piece::ROOK => 4,
        Piece::BISHOP => 3,
        Piece::KNIGHT => 2,
        Piece::PAWN => 1,
        _ => 0,
    }
}

pub fn sort_types(ts: &mut [Piece]) {
    ts.sort_by_key(|&p| std::cmp::Reverse(piece_order(p)));
}

/// >0 if side a is stronger than side b, 0 if equal.
pub fn compare_sides(a: &[Piece], b: &[Piece]) -> i32 {
    if a.len() != b.len() {
        return a.len() as i32 - b.len() as i32;
    }
    for i in 0..a.len() {
        let d = piece_order(a[i]) - piece_order(b[i]);
        if d != 0 {
            return d;
        }
    }
    0
}

impl Material {
    /// The usual notation, e.g. "KQKR" or "KBNK".
    pub fn name(&self) -> String {
        let mut s = String::from("K");
        for p in &self.white {
            s.push((p.to_char() & !0x20) as char);
        }
        s.push('K');
        for p in &self.black {
            s.push((p.to_char() & !0x20) as char);
        }
        s
    }

    /// The total number of pieces including the kings.
    pub fn pieces(&self) -> usize {
        2 + self.white.len() + self.black.len()
    }

    /// The pawns of both sides.
    pub fn pawns(&self) -> usize {
        self.white.iter().chain(self.black.iter()).filter(|&&p| p == Piece::PAWN).count()
    }

    /// The pieces besides the kings with their color, white first. The order of
    /// the slots is the order of the index digits.
    pub fn slots(&self) -> Vec<Piece> {
        let mut s = Vec::with_capacity(self.white.len() + self.black.len());
        s.extend(self.white.iter().map(|&p| Piece::WHITE | p));
        s.extend(self.black.iter().map(|&p| Piece::BLACK | p));
        s
    }

    /// The 35 materials with up to four pieces in generation order: fewer pawns
    /// first, then fewer pieces. Every material a table depends on (captures lead
    /// to fewer pieces, promotions to fewer pawns) comes earlier. The order is
    /// also the layout of the cache file, so it must not change.
    pub fn all() -> Vec<Material> {
        let types = [Piece::QUEEN, Piece::ROOK, Piece::BISHOP, Piece::KNIGHT, Piece::PAWN];
        let mut all = Vec::new();
        for &x in &types {
            all.push(Material { white: vec![x], black: vec![] });
        }
        for (i, &x) in types.iter().enumerate() {
            for &y in &types[i..] {
                all.push(Material { white: vec![x, y], black: vec![] });
            }
        }
        for (i, &x) in types.iter().enumerate() {
            for &y in &types[i..] {
                all.push(Material { white: vec![x], black: vec![y] });
            }
        }
        all.sort_by_key(|m| (m.pawns(), m.pieces())); // stable
        all
    }

    /// All materials with exactly `pieces` pieces (kings included) in
    /// dependency order: fewer pawns first (promotions lead to fewer pawns),
    /// then by name. Captures lead to fewer pieces, which are always available.
    pub fn all_with(pieces: usize) -> Vec<Material> {
        let types = [Piece::QUEEN, Piece::ROOK, Piece::BISHOP, Piece::KNIGHT, Piece::PAWN];
        fn multisets(types: &[Piece], k: usize, start: usize, cur: &mut Vec<Piece>, out: &mut Vec<Vec<Piece>>) {
            if cur.len() == k {
                out.push(cur.clone());
                return;
            }
            for i in start..types.len() {
                cur.push(types[i]);
                multisets(types, k, i, cur, out);
                cur.pop();
            }
        }
        let n = pieces - 2;
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for k in 0..=n {
            let (mut whites, mut blacks) = (Vec::new(), Vec::new());
            multisets(&types, k, 0, &mut Vec::new(), &mut whites);
            multisets(&types, n - k, 0, &mut Vec::new(), &mut blacks);
            for w in &whites {
                for b in &blacks {
                    let m = Material::canonical(w, b);
                    if seen.insert(m.name()) {
                        out.push(m);
                    }
                }
            }
        }
        out.sort_by_key(|m| (m.pawns(), m.name()));
        out
    }

    /// Reads a material name like "KQKBN" (white pieces, then black pieces, each
    /// side led by its king) into canonical form.
    pub fn parse(name: &str) -> Result<Material, String> {
        let mut sides: [Vec<Piece>; 2] = [Vec::new(), Vec::new()];
        let mut side: i32 = -1;
        for &c in name.as_bytes() {
            if c == b'K' {
                side += 1;
                if side > 1 {
                    return Err(format!("egtb: {:?} has more than two kings", name));
                }
                continue;
            }
            let p = Piece::from_char(c).kind();
            if side < 0 || p == Piece::NONE || p == Piece::KING || p == Piece::BLOCKED {
                return Err(format!("egtb: bad material {:?}", name));
            }
            sides[side as usize].push(p);
        }
        if side != 1 {
            return Err(format!("egtb: {:?} needs two kings", name));
        }
        let m = Material::canonical(&sides[0], &sides[1]);
        if m.pieces() > table::MAX_PIECES {
            return Err(format!("egtb: {:?} has more than {} pieces", name, table::MAX_PIECES));
        }
        Ok(m)
    }

    /// Estimates the positions of a pawnless material without any symmetry:
    /// 3612 legal king pairs, the other pieces on the remaining squares, both
    /// sides to move (before removing positions with the opponent in check).
    pub fn raw_positions(&self) -> u64 {
        let mut n: u64 = 3612 * 2;
        let mut free: u64 = 62;
        for _ in 0..self.white.len() + self.black.len() {
            n *= free;
            free -= 1;
        }
        n
    }

    /// The material of a board (kings excluded) in canonical form; None when a
    /// side has no king or the material exceeds MAX_PIECES.
    pub fn of_board(b: &Board) -> Option<Material> {
        let (mut w, mut bl) = (Vec::new(), Vec::new());
        let mut kings = 0;
        for &p in &b.squares {
            if p == Piece::NONE {
                continue;
            }
            if p.is(Piece::KING) {
                kings += 1;
            } else if p.color() == Piece::WHITE {
                w.push(p.kind());
            } else {
                bl.push(p.kind());
            }
        }
        if kings != 2 || 2 + w.len() + bl.len() > table::MAX_PIECES {
            return None;
        }
        Some(Material::canonical(&w, &bl))
    }

    /// Sorts both sides and puts the stronger one first.
    pub fn canonical(w: &[Piece], b: &[Piece]) -> Material {
        let (mut w, mut b) = (w.to_vec(), b.to_vec());
        sort_types(&mut w);
        sort_types(&mut b);
        if compare_sides(&w, &b) < 0 {
            std::mem::swap(&mut w, &mut b);
        }
        Material { white: w, black: b }
    }

    pub fn signature(&self) -> Signature {
        let mut s = 0u32;
        for &p in &self.white {
            s += 1 << type_bit(p);
        }
        for &p in &self.black {
            s += 1 << (10 + type_bit(p));
        }
        s
    }
}

/// Identifies a material by piece counts: two bits per type and color, white in
/// the low ten bits. A board's signature and the signature with swapped colors
/// are both cheap to compute, which is all a lookup needs.
pub type Signature = u32;

pub const SIGNATURE_SPACE: usize = 1 << 20;

fn type_bit(p: Piece) -> u32 {
    2 * (piece_order(p) - 1) as u32 // pawn 0, knight 2, bishop 4, rook 6, queen 8
}

/// The signature of the board and the one with swapped colors. The kings are
/// not counted.
pub fn board_signature(b: &Board) -> (Signature, Signature) {
    let (mut w, mut bl) = (0u32, 0u32);
    for p in [Piece::PAWN, Piece::KNIGHT, Piece::BISHOP, Piece::ROOK, Piece::QUEEN] {
        let (cw, cb) = (b.piece_bb(Piece::WHITE | p).count_ones(), b.piece_bb(Piece::BLACK | p).count_ones());
        if cw > 3 || cb > 3 {
            // four of a kind do not fit the two-bit field and would alias
            // another material (four pawns look like a knight): no table
            return (SIGNATURE_SPACE as u32 - 1, SIGNATURE_SPACE as u32 - 1);
        }
        w += cw << type_bit(p);
        bl += cb << type_bit(p);
    }
    (w | bl << 10, bl | w << 10)
}
