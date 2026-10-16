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

    /// The interval of plies the true distance lies in when the value comes
    /// from a table with the given step (moves per bucket, see `quantize`): an
    /// exact table (step 1) gives lo == hi, a quantised table the whole
    /// bucket. Draws and mates in 0 give (0, 0).
    pub fn bounds(self, step: u32) -> (u32, u32) {
        if step <= 1 || self == Value::DRAW || self.0 == LOSS_BASE {
            let p = self.plies();
            return (p, p);
        }
        if self.is_win() {
            let v = self.0 as u32;
            return (2 * (v + 1).saturating_sub(step).max(1) - 1, 2 * v - 1);
        }
        let m = (self.0 - LOSS_BASE) as u32;
        (2 * (m + 1).saturating_sub(step).max(1), 2 * m)
    }

    /// Rounds the value up to the next multiple of `step` moves ("mate in at
    /// most 8" instead of the exact count), the upper end of its bucket.
    /// Draws and mates in 0 stay; wins and losses stay apart.
    pub fn quantize(self, step: u32) -> Value {
        if step <= 1 || self == Value::DRAW || self.0 == LOSS_BASE {
            return self;
        }
        let top = (LOSS_BASE - 1) as u32;
        if self.is_win() {
            return Value(((self.0 as u32).div_ceil(step) * step).min(top) as u8);
        }
        Value(LOSS_BASE + (((self.0 - LOSS_BASE) as u32).div_ceil(step) * step).min(top) as u8)
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
    /// Empty until generated or loaded as a whole (the base); a table beyond
    /// the base loads packed, see `packed`.
    pub values: Vec<AtomicU8>,
    /// Serves a table that stays compressed in its cache file (every material
    /// beyond the base once loaded): a block cache behind `value()`. `values`
    /// is empty then; the generator never uses such a table. A mutex, because
    /// lookups take a shared reference (uncontended: one search thread).
    packed: Option<Mutex<super::blockstore::BlockStore>>,
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
    /// The resolution of the values in moves: 1 = exact distances, n = every
    /// value is the upper end of a bucket of n moves (`quantize`), so a lookup
    /// only bounds the distance (`Value::bounds`) and the search has to
    /// resolve the rest. The generator always produces exact values. Atomic
    /// like `filled`, because `save_table` quantises through a shared reference.
    step: std::sync::atomic::AtomicU32,
}

impl Table {
    pub(super) fn new(mat: Material) -> Table {
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
            packed: None,
            raw_checksum: std::sync::atomic::AtomicU64::new(0),
            invalid: Mutex::new(None),
            filled: std::sync::atomic::AtomicBool::new(false),
            step: std::sync::atomic::AtomicU32::new(1),
        }
    }

    /// Coarsens the values in memory to buckets of `step` moves (see
    /// `Value::bounds`): what the cache files beyond the base hold
    /// (docs/m5-endgame-tables.md). Exact values cannot be recovered, so the
    /// table can no longer serve the generator as a dependency.
    pub fn quantize(&self, step: u32) {
        if step <= 1 || self.values.is_empty() {
            return;
        }
        for v in &self.values {
            v.store(Value(v.load(Ordering::Relaxed)).quantize(step).0, Ordering::Relaxed);
        }
        self.set_step(step);
    }

    /// The resolution of the values in moves (1 = exact).
    pub fn step(&self) -> u32 {
        self.step.load(Ordering::Relaxed)
    }

    pub(super) fn set_step(&self, step: u32) {
        self.step.store(step.max(1), Ordering::Relaxed);
    }

    /// Whether the values are exact distances (step 1).
    pub fn is_exact(&self) -> bool {
        self.step() <= 1
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
        self.packed = None;
        self.raw_checksum.store(0, Ordering::Relaxed);
        *self.invalid.lock().unwrap() = None;
        self.set_filled(false);
        self.set_step(1);
    }

    #[inline(always)]
    fn kk(&self) -> &'static KkTable {
        if self.pawns {
            &SYMMETRY.pawns
        } else {
            &SYMMETRY.pawnless
        }
    }

    /// Whether the values are in memory as an array (generated, or the loaded base).
    pub fn is_generated(&self) -> bool {
        !self.values.is_empty()
    }

    /// Whether the table answers lookups: generated or loaded as a whole, or
    /// loaded packed.
    pub fn is_available(&self) -> bool {
        !self.values.is_empty() || self.packed.is_some()
    }

    /// Whether the table stays compressed in its file.
    pub fn is_packed(&self) -> bool {
        self.packed.is_some()
    }

    pub(super) fn set_packed(&mut self, store: super::blockstore::BlockStore) {
        self.packed = Some(Mutex::new(store));
    }

    pub(super) fn packed(&self) -> Option<&Mutex<super::blockstore::BlockStore>> {
        self.packed.as_ref()
    }

    /// One entry: from the array, or from the block cache of a packed table.
    #[inline(always)]
    pub fn value(&self, idx: usize) -> Value {
        if !self.values.is_empty() {
            return Value(self.values[idx].load(Ordering::Relaxed));
        }
        self.packed.as_ref().expect("a table answers lookups only when available").lock().unwrap().value(idx)
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
    /// Counts the lookups of materials without a table (up to MAX_PIECES
    /// pieces), by canonical signature: the demand a search creates for
    /// tables that could be generated (`missing_tables`). A mutex, because
    /// lookups take a shared reference; an Arc, because the main loop reads
    /// it (`demand_handle`) while the search thread owns the set.
    missing: std::sync::Arc<Mutex<std::collections::HashMap<super::Signature, u64>>>,
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
                nt.set_step(t.step());
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
                t.set_step(ft.step());
                *t.invalid.lock().unwrap() = ft.invalid.lock().unwrap().take();
            }
        }
    }


    /// Allocates the base tables without values; `generate_all` or `load` fills them.
    pub fn new() -> Set {
        let mut s = Set { tables: Vec::new(), base_count: 0, by_sig: vec![-1; SIGNATURE_SPACE], cache_dir: super::persist::default_cache_dir(), missing: std::sync::Arc::new(Mutex::new(std::collections::HashMap::new())) };
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
        self.lookup_table(b).map(|(_, v)| v)
    }

    /// `lookup` with the interval of plies the distance lies in
    /// (`Value::bounds` with the table's step): lo == hi for exact tables.
    pub fn lookup_bounds(&self, b: &Board) -> Option<(Value, u32, u32)> {
        let (t, v) = self.lookup_table(b)?;
        let (lo, hi) = v.bounds(t.map_or(1, |t| t.step()));
        Some((v, lo, hi))
    }

    /// The value and its table (`None` for king against king).
    fn lookup_table(&self, b: &Board) -> Option<(Option<&Table>, Value)> {
        let occ = b.by_color[0] | b.by_color[1];
        if occ.count_ones() == 2 && b.castling == 0 {
            return Some((None, Value::DRAW));
        }
        let (t, flip) = self.find_table(b)?;
        if !t.is_available() {
            // registered but not generated: no index for nothing (a search
            // asks millions of times), but counted as demand
            *self.missing.lock().unwrap().entry(t.mat.signature()).or_insert(0) += 1;
            return None;
        }
        let idx = t.index_of(b, flip);
        if idx < 0 {
            return None; // adjacent kings, never reached from a legal position
        }
        Some((Some(t), t.value(idx as usize)))
    }

    /// Sums the block cache hits and misses of every packed table, and the
    /// blocks ever decoded (the working set) out of all their blocks.
    pub fn cache_stats(&self) -> (u64, u64, u64, u64) {
        let mut sums = (0, 0, 0, 0);
        for t in &self.tables {
            if let Some(p) = t.packed() {
                let p = p.lock().unwrap();
                sums.0 += p.hits;
                sums.1 += p.misses;
                sums.2 += p.distinct;
                sums.3 += p.blocks() as u64;
            }
        }
        sums
    }

    /// Zeroes the counters of `cache_stats` (the working set too).
    pub fn reset_cache_stats(&self) {
        for t in &self.tables {
            if let Some(p) = t.packed() {
                p.lock().unwrap().reset_stats();
            }
        }
    }

    /// The first block read error of any packed table, if one happened (such
    /// a table answers Draw for the affected blocks).
    pub fn cache_error(&self) -> Option<String> {
        self.tables.iter().filter_map(|t| t.packed()).find_map(|p| p.lock().unwrap().err.clone())
    }

    /// The materials without a table that lookups asked for since the last
    /// reset, most asked first, at most n (0: all). That is the demand a
    /// search creates: the table asked for most is the one worth generating
    /// next (docs/m6-uci.md, "Erzeugung nach Bedarf").
    pub fn missing_tables(&self, n: usize) -> Vec<(Material, u64)> {
        let mut map = self.missing.lock().unwrap();
        // asked for before the table arrived: no demand any more
        map.retain(|&sig, _| self.find(&material_from_signature(sig).name()).is_none_or(|i| !self.tables[i].is_available()));
        missing_tables_of(&map, n)
    }

    /// A handle on the demand counters that outlives the set's move into the
    /// search thread: the main loop reads it to start a generation meanwhile.
    pub fn demand_handle(&self) -> std::sync::Arc<Mutex<std::collections::HashMap<super::Signature, u64>>> {
        self.missing.clone()
    }

    /// Zeroes the counters of `missing_tables`.
    pub fn reset_missing(&self) {
        self.missing.lock().unwrap().clear();
    }

    /// Drops one material from `missing_tables` (its generation was decided),
    /// so that the next decision looks at the rest.
    pub fn forget_missing(&self, m: &Material) {
        self.missing.lock().unwrap().remove(&m.signature());
    }
}

/// The materials of a demand map (see `Set::missing_tables`), most asked
/// first, at most n (0: all).
pub fn missing_tables_of(map: &std::collections::HashMap<super::Signature, u64>, n: usize) -> Vec<(Material, u64)> {
    let mut out: Vec<(Material, u64)> = map.iter().filter(|(_, &p)| p > 0).map(|(&sig, &p)| (material_from_signature(sig), p)).collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name().cmp(&b.0.name())));
    if n > 0 {
        out.truncate(n);
    }
    out
}

/// Decodes the two-bit counts per type and colour (see `board_signature`)
/// into a canonical material.
fn material_from_signature(sig: super::Signature) -> Material {
    let (mut w, mut bl) = (Vec::new(), Vec::new());
    for p in [Piece::PAWN, Piece::KNIGHT, Piece::BISHOP, Piece::ROOK, Piece::QUEEN] {
        let bit = super::type_bit(p);
        for _ in 0..(sig >> bit) & 3 {
            w.push(p);
        }
        for _ in 0..(sig >> (bit + 10)) & 3 {
            bl.push(p);
        }
    }
    Material::canonical(&w, &bl)
}

impl Set {

    /// The table and the index of a position with three or four pieces, no
    /// castling rights and no en passant square. The index is -1 for adjacent
    /// kings. `None` when no table covers the position.
    pub fn locate(&self, b: &Board) -> Option<(&Table, i64)> {
        let (t, flip) = self.find_table(b)?;
        Some((t, t.index_of(b, flip)))
    }

    /// Finds the table of a position's material (registered, generated or
    /// not) and whether the colors have to be swapped for it.
    fn find_table(&self, b: &Board) -> Option<(&Table, bool)> {
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
            // a material without a table: counted, so that the demand of a
            // search can drive which table is generated next
            // (`missing_tables`); four of a kind (the sentinel signature)
            // never has a table
            if sig as usize != SIGNATURE_SPACE - 1 {
                *self.missing.lock().unwrap().entry(sig.min(flipped)).or_insert(0) += 1;
            }
            return None;
        }
        Some((&self.tables[ti as usize], flip))
    }
}

impl Table {
    /// The index of a position in this table (-1 for adjacent kings).
    fn index_of(&self, b: &Board, flip: bool) -> i64 {
        let mut sq = [Pos::NONE; 6];
        let n = self.squares(b, flip, &mut sq);
        self.index(b.white_move != flip, &sq[..n])
    }
}

impl Default for Set {
    fn default() -> Set {
        Set::new()
    }
}
