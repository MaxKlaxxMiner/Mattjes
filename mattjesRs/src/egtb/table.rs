//! Tables, values and the lookup, see `mattjesGo/egtb/table.go`.

use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

use super::generate::Bitset;
use super::index::{KkTable, PAWN_OFFSET, PAWN_SQUARES, PIECE_SQUARES, SYMMETRY};
use super::{board_signature, Material, SIGNATURE_SPACE};
use crate::bitboard::Board;
use crate::chess::{Piece, Pos};

/// The table entry of a position from the point of view of the side to move:
/// 0 draw, 1..127 mates in 2v-1 plies, 128..255 gets mated in 2(v-128) plies
/// (128 = is mated right now). A win always takes an odd number of plies and a
/// loss an even one, so storing the move count instead of the ply count doubles
/// the range for free: wins up to 253 plies, losses up to 254, which covers
/// every five-piece ending (KPPKP needs a loss in 254). Dead and illegal
/// indices have no value of their own: the generator keeps them in a bitset,
/// stores 0 and fills them with their predecessor before compression; they are
/// never looked up.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(transparent)]
pub struct Value(pub u8);

const LOSS_BASE: u8 = 128;
/// The longest distance a table can express (wins 253, losses 254).
pub const MAX_PLIES: u32 = 254;

impl Value {
    pub const DRAW: Value = Value(0);

    /// A win in `plies` (odd).
    pub fn win_in(plies: u32) -> Value {
        Value(plies.div_ceil(2) as u8)
    }

    /// A loss in `plies` (even).
    pub fn loss_in(plies: u32) -> Value {
        Value(LOSS_BASE + (plies / 2) as u8)
    }

    pub fn is_win(self) -> bool {
        self.0 >= 1 && self.0 < LOSS_BASE
    }

    pub fn is_loss(self) -> bool {
        self.0 >= LOSS_BASE
    }

    /// The distance to mate for a win or a loss, 0 otherwise.
    pub fn plies(self) -> u32 {
        if self.is_win() {
            2 * self.0 as u32 - 1
        } else if self.is_loss() {
            2 * (self.0 - LOSS_BASE) as u32
        } else {
            0
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Value::DRAW {
            write!(f, "draw")
        } else if self.is_win() {
            write!(f, "win in {}", self.plies())
        } else {
            write!(f, "loss in {}", self.plies())
        }
    }
}

/// The endgame table of one material: a dense array indexed by the position.
/// Squares are listed as white king, black king, then the slots of the
/// material in order. The values are atomic bytes so that the generator's
/// workers can write distinct entries without locks; reads are plain loads.
pub struct Table {
    pub mat: Material,
    /// Colored pieces after the kings.
    pub slots: Vec<Piece>,
    /// Index digits: 64 per piece, 48 per pawn.
    sizes: Vec<usize>,
    /// Slot holds the same piece as the previous slot: squares are kept ascending.
    equal_prev: Vec<bool>,
    pawns: bool,
    pub size: usize,
    /// Empty until generated or loaded.
    pub values: Vec<AtomicU8>,
    /// The checksum of the values as generated (dead and illegal entries as 0);
    /// set after generation or read from the cache file header. Checksum
    /// constants in the code refer to this. 0 = not computed yet.
    pub raw_checksum: std::sync::atomic::AtomicU64,
    /// The dead and illegal indices after generation (None for a loaded
    /// table); only `fill` reads it, so a mutex costs nothing.
    invalid: Mutex<Option<Bitset>>,
    /// Dead and illegal entries were overwritten with their predecessor for
    /// compression ("don't care"); the values no longer hash to raw_checksum.
    filled: std::sync::atomic::AtomicBool,
}

impl Table {
    fn new(mat: Material) -> Table {
        let slots = mat.slots();
        let equal_prev = (0..slots.len()).map(|i| i > 0 && slots[i - 1] == slots[i]).collect();
        let pawns = mat.pawns() > 0;
        let mut size = 2 * if pawns { SYMMETRY.pawns.squares.len() } else { SYMMETRY.pawnless.squares.len() };
        let sizes: Vec<usize> = slots.iter().map(|p| if p.is(Piece::PAWN) { PAWN_SQUARES } else { PIECE_SQUARES }).collect();
        for s in &sizes {
            size *= s;
        }
        Table {
            mat,
            slots,
            sizes,
            equal_prev,
            pawns,
            size,
            values: Vec::new(),
            raw_checksum: std::sync::atomic::AtomicU64::new(0),
            invalid: Mutex::new(None),
            filled: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Hands the generator's bitset of dead and illegal indices to the table.
    pub(super) fn set_invalid(&self, invalid: Bitset) {
        *self.invalid.lock().unwrap() = Some(invalid);
    }

    /// The checksum of the generated values, computed when the table was never
    /// saved or loaded.
    pub fn raw_checksum(&self) -> u64 {
        let mut sum = self.raw_checksum.load(Ordering::Relaxed);
        if sum == 0 && !self.is_filled() && self.is_generated() {
            sum = super::checksum(self.bytes());
            self.raw_checksum.store(sum, Ordering::Relaxed);
        }
        sum
    }

    pub fn is_filled(&self) -> bool {
        self.filled.load(Ordering::Relaxed)
    }

    pub(super) fn set_filled(&self, filled: bool) {
        self.filled.store(filled, Ordering::Relaxed);
    }

    /// Replaces dead and illegal entries by their predecessor ("don't care"),
    /// once, and drops the generator's bitset afterwards (what writing the
    /// table does; also for tables kept in RAM only).
    pub fn fill(&self) {
        if self.is_filled() {
            return;
        }
        self.raw_checksum();
        if let Some(invalid) = self.invalid.lock().unwrap().take() {
            let mut prev = Value::DRAW;
            for (i, v) in self.values.iter().enumerate() {
                if invalid.get(i) {
                    v.store(prev.0, Ordering::Relaxed);
                } else {
                    prev = Value(v.load(Ordering::Relaxed));
                }
            }
        }
        self.set_filled(true);
    }

    /// Resets the table to "not generated".
    pub(super) fn clear(&mut self) {
        self.values = Vec::new();
        self.raw_checksum.store(0, Ordering::Relaxed);
        *self.invalid.lock().unwrap() = None;
        self.set_filled(false);
    }

    #[inline(always)]
    fn kk(&self) -> &'static KkTable {
        if self.pawns {
            &SYMMETRY.pawns
        } else {
            &SYMMETRY.pawnless
        }
    }

    pub fn is_generated(&self) -> bool {
        !self.values.is_empty()
    }

    #[inline(always)]
    pub fn value(&self, idx: usize) -> Value {
        Value(self.values[idx].load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn set_value(&self, idx: usize, v: Value) {
        self.values[idx].store(v.0, Ordering::Relaxed)
    }

    /// The position index; -1 if the kings are adjacent or equal. A square
    /// occupied twice still gives an index (marked `INVALID` in the table). Two
    /// equal pieces are ordered by square, so both orders give the same index;
    /// the indices with the other order are dead (`decode` does not know that,
    /// the generator marks them `INVALID`).
    pub fn index(&self, white_move: bool, sq: &[Pos]) -> i64 {
        let kk = self.kk();
        let (k, tf) = kk.at(sq[0], sq[1]);
        if k < 0 {
            return -1;
        }
        let mut base = k as usize;
        if !white_move {
            base += kk.squares.len();
        }
        let mut idx = self.digits(base, tf, sq);
        if kk.diagonal[k as usize] {
            // both kings on the diagonal: the kings do not fix the transposition,
            // so the smaller of the two indices is the canonical one (independent
            // of the order in which equal pieces are listed)
            idx = idx.min(self.digits(base, tf | 4, sq));
        }
        idx as i64
    }

    /// Appends the piece squares, transformed by tf, to the king pair index.
    fn digits(&self, mut idx: usize, tf: usize, sq: &[Pos]) -> usize {
        let xf = &SYMMETRY.xform[tf];
        let mut s = [0usize; 4];
        for i in 0..self.sizes.len() {
            s[i] = xf[sq[2 + i].idx()].idx();
            // keep every run of equal pieces sorted by square (insertion sort: a
            // single neighbour swap is a full sort for pairs, but not for triples)
            let mut j = i;
            while j > 0 && self.equal_prev[j] && s[j] < s[j - 1] {
                s.swap(j, j - 1);
                j -= 1;
            }
        }
        for (i, &size) in self.sizes.iter().enumerate() {
            let mut d = s[i];
            if size == PAWN_SQUARES {
                d -= PAWN_OFFSET;
            }
            idx = idx * size + d;
        }
        idx
    }

    /// The inverse of `index`: fills sq (len 2 + slots) and returns the side to
    /// move. Decoded positions are always in canonical form, but an index with two
    /// equal pieces in descending square order (or the transposed twin when both
    /// kings are on the diagonal) is dead: index(decode(idx)) then differs from
    /// idx, which is how the generator recognizes it.
    pub fn decode(&self, mut idx: usize, sq: &mut [Pos]) -> bool {
        for i in (0..self.sizes.len()).rev() {
            let size = self.sizes[i];
            let mut s = idx % size;
            idx /= size;
            if size == PAWN_SQUARES {
                s += PAWN_OFFSET;
            }
            sq[2 + i] = Pos(s as i8);
        }
        let kk = self.kk();
        let n = kk.squares.len();
        let white_move = idx < n;
        if !white_move {
            idx -= n;
        }
        sq[0] = kk.squares[idx][0];
        sq[1] = kk.squares[idx][1];
        white_move
    }

    /// The position of an index (a convenience for tests and output; the
    /// generator does the same inline).
    pub fn board(&self, idx: usize) -> Board {
        let mut sq = vec![Pos::NONE; 2 + self.slots.len()];
        let white_move = self.decode(idx, &mut sq);
        let mut pieces = vec![Piece::WHITE_KING, Piece::BLACK_KING];
        pieces.extend_from_slice(&self.slots);
        Board::from_pieces(white_move, &sq, &pieces)
    }

    /// Extracts the squares of the board in slot order, with the colors swapped
    /// and the board mirrored vertically when flip is set. Returns the count.
    fn squares(&self, b: &Board, flip: bool, sq: &mut [Pos]) -> usize {
        let m: i8 = if flip { 56 } else { 0 };
        let (us, them) = if flip { (1, 0) } else { (0, 1) };
        sq[0] = Pos(b.king_pos(us).0 ^ m);
        sq[1] = Pos(b.king_pos(them).0 ^ m);
        let mut taken = 0u64;
        for (i, &p) in self.slots.iter().enumerate() {
            let p = if flip { p.opponent() } else { p }; // xor of the color bits
            let bb = b.piece_bb(p) & !taken;
            let s = bb.trailing_zeros() as i8;
            taken |= 1 << s;
            sq[2 + i] = Pos(s ^ m);
        }
        2 + self.slots.len()
    }

    /// The values as plain bytes. Only call while no worker writes (the
    /// generator has finished): `AtomicU8` is `repr(transparent)` over `u8`.
    pub fn bytes(&self) -> &[u8] {
        // SAFETY: AtomicU8 has the same size and alignment as u8, and the slice
        // is only read while the table is not being generated.
        unsafe { std::slice::from_raw_parts(self.values.as_ptr() as *const u8, self.values.len()) }
    }

    /// The values as mutable bytes (for loading).
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: as in bytes(); &mut self excludes every other access.
        unsafe { std::slice::from_raw_parts_mut(self.values.as_mut_ptr() as *mut u8, self.values.len()) }
    }
}

/// The largest material a table can hold (two kings + four pieces: the square
/// arrays and index digits are sized for it).
pub const MAX_PIECES: usize = 6;

/// The collection of tables: the base of all materials up to four pieces
/// (always complete, one cache file) plus any larger materials added on demand
/// (one file each).
pub struct Set {
    pub tables: Vec<Table>,
    /// The first `base_count` tables are the four-piece base.
    base_count: usize,
    /// signature -> table index, -1 none
    by_sig: Vec<i8>,
    /// Holds the cache files (the base file and one per larger material);
    /// next to the binary unless changed.
    pub cache_dir: std::path::PathBuf,
}

impl Set {
    /// A new set with copies of every generated table, so that a background
    /// generation can work on it while this set stays untouched (the copies
    /// cost the size of the tables once, 173 MB for the base).
    pub fn fork(&self) -> Set {
        let mut n = Set::new();
        n.cache_dir = self.cache_dir.clone();
        for t in &self.tables {
            let i = n.add_material(t.mat.clone());
            if t.is_generated() {
                let nt = &mut n.tables[i];
                nt.values = t.bytes().iter().map(|&b| AtomicU8::new(b)).collect();
                nt.raw_checksum.store(t.raw_checksum.load(Ordering::Relaxed), Ordering::Relaxed);
                nt.set_filled(t.is_filled());
            }
        }
        n
    }

    /// Moves every generated table this set lacks over from another set (the
    /// result of a forked generation).
    pub fn adopt(&mut self, from: Set) {
        for mut ft in from.tables {
            if !ft.is_generated() {
                continue;
            }
            let i = self.add_material(ft.mat.clone());
            let t = &mut self.tables[i];
            if !t.is_generated() {
                t.values = std::mem::take(&mut ft.values);
                t.raw_checksum.store(ft.raw_checksum.load(Ordering::Relaxed), Ordering::Relaxed);
                t.set_filled(ft.is_filled());
                *t.invalid.lock().unwrap() = ft.invalid.lock().unwrap().take();
            }
        }
    }

    /// Estimates the RAM a generation of the table needs: the table, four
    /// bitsets of 1/8 each and the direct dependencies that are not loaded yet.
    pub fn generation_bytes(&self, ti: usize) -> usize {
        let t = &self.tables[ti];
        let mut n = t.size + t.size / 2;
        for d in t.mat.dependencies() {
            match self.find(&d.name()) {
                Some(i) if self.tables[i].is_generated() => {}
                _ => n += Table::new(d).size,
            }
        }
        n
    }

    /// Allocates the base tables without values; `generate_all` or `load` fills them.
    pub fn new() -> Set {
        let mut s = Set { tables: Vec::new(), base_count: 0, by_sig: vec![-1; SIGNATURE_SPACE], cache_dir: super::persist::default_cache_dir() };
        for m in Material::all() {
            s.add_material(m);
        }
        s.base_count = s.tables.len();
        s
    }

    /// The four-piece tables (the content of the base cache file).
    pub fn base(&self) -> &[Table] {
        &self.tables[..self.base_count]
    }

    /// Registers a table for a material beyond the base (up to MAX_PIECES),
    /// without values. Returns the index of the existing table if already present.
    pub fn add_material(&mut self, m: Material) -> usize {
        if let Some(i) = self.find(&m.name()) {
            return i;
        }
        assert!(m.pieces() <= MAX_PIECES && self.tables.len() < 127, "egtb: cannot add material {}", m.name());
        self.by_sig[m.signature() as usize] = self.tables.len() as i8;
        self.tables.push(Table::new(m));
        self.tables.len() - 1
    }

    /// The number of bytes of the base file.
    pub fn base_size(&self) -> usize {
        self.base().iter().map(|t| t.size).sum()
    }

    /// The table of a material by name, e.g. "KBNK".
    pub fn find(&self, name: &str) -> Option<usize> {
        self.tables.iter().position(|t| t.mat.name() == name)
    }

    /// The number of bytes of all tables.
    pub fn total_size(&self) -> usize {
        self.tables.iter().map(|t| t.size).sum()
    }

    /// The value of a position with up to four pieces, without castling rights
    /// and without an en passant square. `None` when the position is not covered:
    /// more pieces, castling or en passant (the search plays one more ply) or a
    /// table that is not computed yet. King against king is a draw without a
    /// table.
    pub fn lookup(&self, b: &Board) -> Option<Value> {
        let occ = b.by_color[0] | b.by_color[1];
        if occ.count_ones() == 2 && b.castling == 0 {
            return Some(Value::DRAW);
        }
        let (t, idx) = self.locate(b)?;
        if !t.is_generated() || idx < 0 {
            return None; // idx < 0: adjacent kings, never reached from a legal position
        }
        Some(t.value(idx as usize))
    }

    /// The table and the index of a position with three or four pieces, no
    /// castling rights and no en passant square. The index is -1 for adjacent
    /// kings. `None` when no table covers the position.
    pub fn locate(&self, b: &Board) -> Option<(&Table, i64)> {
        if b.castling != 0 || b.en_passant.valid() {
            return None;
        }
        let occ = b.by_color[0] | b.by_color[1];
        let n = occ.count_ones();
        if !(3..=MAX_PIECES as u32).contains(&n) {
            return None;
        }
        let (sig, flipped) = board_signature(b);
        let mut flip = false;
        let mut ti = self.by_sig[sig as usize];
        if ti < 0 {
            ti = self.by_sig[flipped as usize];
            flip = true;
        }
        if ti < 0 {
            return None;
        }
        let t = &self.tables[ti as usize];
        let mut sq = [Pos::NONE; 6];
        let n = t.squares(b, flip, &mut sq);
        Some((t, t.index(b.white_move != flip, &sq[..n])))
    }
}

impl Default for Set {
    fn default() -> Set {
        Set::new()
    }
}
