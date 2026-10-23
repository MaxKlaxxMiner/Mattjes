//! Syzygy WDL tablebases (files *.rtbw) for the search oracle: win, draw or
//! loss for the side to move, no distance. A port of `mattjesGo/syzygy`,
//! which follows Fathom (tbprobe.c, MIT licence, Ronald de Man, basil00,
//! Jon Dart): tables are found by name in a directory, mapped into memory
//! on first use and decoded per probe. DTZ files are ignored; cursed wins
//! and blessed losses are reported as such, the fifty-move rule is the
//! caller's business.

pub mod decode;
mod encode;
mod mmap;
mod pairs;
pub mod write;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use crate::bitboard::Board;
use crate::chess::{new_buffer, Move, Piece};
use crate::egtb::Set;
use encode::{EncInfo, Shape, ENC_FILE, ENC_PIECE, MAX_PIECES};
use mmap::Mapping;
use pairs::PairsData;

const WDL_MAGIC: u32 = 0x5d23e871;

/// The largest table the reader handles.
pub const MAX_TABLE_PIECES: usize = 7;

/// The piece kinds in bitboard order (K Q R B N P), the letters of a name.
const KIND_LETTERS: &[u8] = b"KQRBNP";

/// WDL values for the side to move.
pub const LOSS: i32 = -2;
pub const BLESSED_LOSS: i32 = -1;
pub const DRAW: i32 = 0;
pub const CURSED_WIN: i32 = 1;
pub const WIN: i32 = 2;

/// The mapped and parsed part of a table.
struct Loaded {
    mapping: Mapping,
    /// 2 (pieces: one per side to move) or 8 (pawns: 4 files, then the other side).
    ei: Vec<EncInfo>,
}

/// One table (one material, both colour assignments).
pub struct BaseEntry {
    pub(crate) name: String,
    path: PathBuf,
    pub(crate) shape: Shape,
    pub(crate) symmetric: bool,
    pub(crate) has_pawns: bool,
    /// Our own file (write.rs) instead of a Syzygy file.
    own: bool,
    /// An own DTM file: distance buckets of `step` moves (read from the header on first use).
    dtm: bool,
    step: std::sync::atomic::AtomicU32,
    loaded: OnceLock<Option<Loaded>>,
}

impl BaseEntry {
    /// Describes the table of a material given as piece counts (the first
    /// side first); None beyond MAX_TABLE_PIECES.
    pub fn describe(name: String, first: &[u8; 6], second: &[u8; 6]) -> Option<BaseEntry> {
        let num: usize = first.iter().chain(second.iter()).map(|&c| c as usize).sum();
        if num > MAX_TABLE_PIECES {
            return None;
        }
        let has_pawns = first[5] + second[5] > 0;
        let symmetric = material_key(first, second) == material_key(second, first);
        let mut shape = Shape { num, kk_enc: false, pawns: [0, 0] };
        if !has_pawns {
            let singles = first.iter().chain(second.iter()).filter(|&&c| c == 1).count();
            shape.kk_enc = singles == 2;
        } else {
            shape.pawns = [first[5], second[5]];
            if second[5] != 0 && (first[5] == 0 || first[5] > second[5]) {
                shape.pawns.swap(0, 1);
            }
        }
        Some(BaseEntry { name, path: PathBuf::new(), shape, symmetric, has_pawns, own: false, dtm: false, step: std::sync::atomic::AtomicU32::new(1), loaded: OnceLock::new() })
    }
}

/// Points a material key to its table; flip = the board's white is the
/// table's black.
#[derive(Clone, Copy)]
struct Lookup {
    entry: usize,
    flip: bool,
}

/// The set of WDL tables found in a directory.
pub struct Tables {
    dir: PathBuf,
    entries: HashMap<u64, Lookup>,
    list: Vec<BaseEntry>,
    /// Our own DTM files (add_dtm), by material key, indexing dtm_list.
    dtm: HashMap<u64, Lookup>,
    dtm_list: Vec<BaseEntry>,
    pub probes: AtomicU64,
    pub hits: AtomicU64,
}

/// Packs the piece counts per colour and kind (white first).
fn material_key(white: &[u8; 6], black: &[u8; 6]) -> u64 {
    let mut k = 0u64;
    for &c in white {
        k = k << 4 | c as u64;
    }
    for &c in black {
        k = k << 4 | c as u64;
    }
    k
}

/// Reads "KQvKRR" into the piece counts of the first and the second side.
fn parse_name(name: &str) -> Option<([u8; 6], [u8; 6])> {
    let (a, b) = name.split_once('v')?;
    let mut first = [0u8; 6];
    let mut second = [0u8; 6];
    for (part, counts) in [(a, &mut first), (b, &mut second)] {
        for ch in part.bytes() {
            let kind = KIND_LETTERS.iter().position(|&k| k == ch)?;
            counts[kind] += 1;
        }
    }
    if first[0] != 1 || second[0] != 1 {
        return None;
    }
    Some((first, second))
}

/// The table name of a material given as piece counts (white first).
pub fn name_of(white: &[u8; 6], black: &[u8; 6]) -> String {
    let mut s = String::new();
    for (kind, &c) in white.iter().enumerate() {
        for _ in 0..c {
            s.push(KIND_LETTERS[kind] as char);
        }
    }
    s.push('v');
    for (kind, &c) in black.iter().enumerate() {
        for _ in 0..c {
            s.push(KIND_LETTERS[kind] as char);
        }
    }
    s
}

impl Tables {
    /// Scans a directory for *.rtbw files and registers them; the files are
    /// mapped on first use. Files with an incomplete size are skipped.
    pub fn open(dir: &Path) -> std::io::Result<Tables> {
        let mut t = Tables::new();
        t.dir = dir.to_path_buf();
        let mut names: Vec<(String, PathBuf, u64)> = Vec::new();
        for de in std::fs::read_dir(dir)? {
            let de = de?;
            let fname = de.file_name().to_string_lossy().to_string();
            let Some(name) = fname.strip_suffix(".rtbw") else { continue };
            let md = de.metadata()?;
            if md.is_dir() {
                continue;
            }
            names.push((name.to_string(), de.path(), md.len()));
        }
        names.sort();
        for (name, path, size) in names {
            let Some((first, second)) = parse_name(&name) else { continue };
            if size & 63 != 16 {
                continue; // incomplete file
            }
            let Some(mut be) = BaseEntry::describe(name, &first, &second) else { continue };
            be.path = path;
            t.register(be, &first, &second);
        }
        Ok(t)
    }

    /// An empty set of tables (add_own fills it without Syzygy files).
    pub fn new() -> Tables {
        Tables { dir: PathBuf::new(), entries: HashMap::new(), list: Vec::new(), dtm: HashMap::new(), dtm_list: Vec::new(), probes: AtomicU64::new(0), hits: AtomicU64::new(0) }
    }

    /// Files a table under both colour assignments.
    fn register(&mut self, be: BaseEntry, first: &[u8; 6], second: &[u8; 6]) {
        let key = material_key(first, second);
        let key2 = material_key(second, first);
        let idx = self.list.len();
        self.list.push(be);
        self.entries.insert(key, Lookup { entry: idx, flip: false });
        if key != key2 {
            self.entries.insert(key2, Lookup { entry: idx, flip: true });
        }
    }

    /// Scans a directory for our own WDL tables (*.wdl, write.rs) and
    /// registers those materials that no Syzygy file covers yet: the
    /// originals win, ours are the fallback. Returns the number added.
    pub fn add_own(&mut self, dir: &Path) -> std::io::Result<usize> {
        let mut names: Vec<PathBuf> = Vec::new();
        for de in std::fs::read_dir(dir)? {
            let de = de?;
            let fname = de.file_name().to_string_lossy().to_string();
            if fname.ends_with(".wdl") && !de.metadata()?.is_dir() {
                names.push(de.path());
            }
        }
        names.sort();
        let mut added = 0;
        for path in names {
            let Some((first, second)) = read_own_header(&path, write::OWN_MAGIC) else { continue };
            if self.entries.contains_key(&material_key(&first, &second)) {
                continue;
            }
            let Some(mut be) = BaseEntry::describe(name_of(&first, &second), &first, &second) else { continue };
            be.path = path;
            be.own = true;
            self.register(be, &first, &second);
            added += 1;
        }
        Ok(added)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The number of tables found.
    pub fn count(&self) -> usize {
        self.list.len()
    }

    /// The piece count of the largest table found.
    pub fn largest(&self) -> usize {
        self.list.iter().map(|e| e.shape.num).max().unwrap_or(0)
    }

    /// Whether a table covers the board's material.
    pub fn has(&self, b: &Board) -> bool {
        let (w, bl) = counts(b);
        self.entries.contains_key(&material_key(&w, &bl))
    }

    /// The WDL value of the position for the side to move (LOSS .. WIN),
    /// None when no table covers the material. The tables store "don't
    /// care" values where a capture decides, so captures (and en passant)
    /// are resolved by a small search first, as in Fathom's probe_wdl.
    pub fn probe_wdl(&self, b: &Board) -> Option<i32> {
        self.probe_wdl_with(b, None)
    }

    /// probe_wdl with the exact base tables for the materials without a WDL
    /// file: our own WDL tables cover one material each, and a probe resolves
    /// captures into smaller materials first.
    pub fn probe_wdl_with(&self, b: &Board, base: Option<&Set>) -> Option<i32> {
        self.probes.fetch_add(1, Ordering::Relaxed);
        let (w, bl) = counts(b);
        if !self.entries.contains_key(&material_key(&w, &bl)) {
            return base_wdl(base?, b);
        }
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        let (mut best_cap, mut best_ep) = (-3, -3);
        for &m in &buf[..n] {
            let ep = is_en_passant(b, m);
            if m.capture == Piece::NONE && !ep {
                continue;
            }
            let mut c = *b;
            c.do_move(m);
            let v = -self.probe_ab(&c, -2, -best_cap, base)?;
            if v > best_cap {
                if v == WIN {
                    self.hits.fetch_add(1, Ordering::Relaxed);
                    return Some(WIN);
                }
                if !ep {
                    best_cap = v;
                } else if v > best_ep {
                    best_ep = v;
                }
            }
        }
        let v = self.probe_table(b, base)?;
        self.hits.fetch_add(1, Ordering::Relaxed);
        if best_ep > best_cap {
            if best_ep > v {
                return Some(best_ep);
            }
            best_cap = best_ep;
        }
        if best_cap >= v {
            return Some(best_cap);
        }
        if best_ep > -3 && v == DRAW {
            // the table says draw because without the en passant capture
            // the side to move would be stalemated: then the capture is forced
            let stalemate = !b.in_check() && buf[..n].iter().all(|&m| is_en_passant(b, m));
            if stalemate {
                return Some(best_ep);
            }
        }
        Some(v)
    }

    /// Resolves captures below a capture (never with en passant rights).
    fn probe_ab(&self, b: &Board, mut alpha: i32, beta: i32, base: Option<&Set>) -> Option<i32> {
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        for &m in &buf[..n] {
            if m.capture == Piece::NONE {
                continue;
            }
            let mut c = *b;
            c.do_move(m);
            let v = -self.probe_ab(&c, -beta, -alpha, base)?;
            if v > alpha {
                if v >= beta {
                    return Some(v);
                }
                alpha = v;
            }
        }
        let v = self.probe_table(b, base)?;
        Some(alpha.max(v))
    }

    /// Looks the position up in its table without capture resolution (from
    /// the base tables when the material has no file).
    fn probe_table(&self, b: &Board, base: Option<&Set>) -> Option<i32> {
        if (b.by_color[0] | b.by_color[1]).count_ones() == 2 {
            return Some(DRAW); // KvK has no table
        }
        let (w, bl) = counts(b);
        let Some(&l) = self.entries.get(&material_key(&w, &bl)) else {
            return base_wdl(base?, b); // exact, captures included
        };
        let be = &self.list[l.entry];
        let loaded = be.loaded.get_or_init(|| if be.own { be.init_own().ok() } else { be.init().ok() }).as_ref()?;
        let d = loaded.mapping.bytes();
        let (ei, idx) = be.locate(loaded, l.flip, b);
        if let Some((own, off, len)) = &ei.own {
            return Some(own.value(&d[*off..*off + *len], idx as usize) as i32 * 2 - 2); // our tables store 0 loss, 1 draw, 2 win
        }
        let w = ei.precomp.as_ref()?.decompress(d, idx);
        Some(w as i32 - 2)
    }

    /// Reads the distance bucket of a position from its own DTM table
    /// (add_dtm): the bucket of step moves ("mate in at most bucket*step
    /// moves" for a win, "mated in at most" for a loss; the WDL tables tell
    /// which and know the draws, a draw's bucket is meaningless), no capture
    /// resolution. None without a table for the material.
    pub fn probe_dtm(&self, b: &Board) -> Option<(usize, u32)> {
        let (w, bl) = counts(b);
        let l = *self.dtm.get(&material_key(&w, &bl))?;
        let be = &self.dtm_list[l.entry];
        let loaded = be.loaded.get_or_init(|| be.init_own().ok()).as_ref()?;
        let d = loaded.mapping.bytes();
        let (ei, idx) = be.locate(loaded, l.flip, b);
        let (own, off, len) = ei.own.as_ref()?;
        Some((own.value(&d[*off..*off + *len], idx as usize) as usize, be.step.load(Ordering::Relaxed)))
    }

    /// The number of own DTM tables registered.
    pub fn dtm_count(&self) -> usize {
        self.dtm_list.len()
    }

    /// Scans a directory for our own DTM tables (*.dtm, write.rs) and
    /// registers them (one per material, the first file wins). Returns the
    /// number added.
    pub fn add_dtm(&mut self, dir: &Path) -> std::io::Result<usize> {
        let mut names: Vec<PathBuf> = Vec::new();
        for de in std::fs::read_dir(dir)? {
            let de = de?;
            let fname = de.file_name().to_string_lossy().to_string();
            if fname.ends_with(".dtm") && !de.metadata()?.is_dir() {
                names.push(de.path());
            }
        }
        names.sort();
        let mut added = 0;
        for path in names {
            let Some((first, second)) = read_own_header(&path, write::DTM_MAGIC) else { continue };
            let key = material_key(&first, &second);
            if self.dtm.contains_key(&key) {
                continue;
            }
            let Some(mut be) = BaseEntry::describe(name_of(&first, &second), &first, &second) else { continue };
            be.path = path;
            be.own = true;
            be.dtm = true;
            let key2 = material_key(&second, &first);
            let idx = self.dtm_list.len();
            self.dtm_list.push(be);
            self.dtm.insert(key, Lookup { entry: idx, flip: false });
            if key != key2 {
                self.dtm.insert(key2, Lookup { entry: idx, flip: true });
            }
            added += 1;
        }
        Ok(added)
    }
}

impl BaseEntry {
    /// Finds the stream and the index of a position in a table (lflip: the
    /// board's colours are the table's second side).
    fn locate<'l>(&self, loaded: &'l Loaded, lflip: bool, b: &Board) -> (&'l EncInfo, u64) {
        let (flip, bside) = if !self.symmetric { (lflip, b.white_move == lflip) } else { (!b.white_move, false) };
        let mut p = [0usize; MAX_PIECES];
        let shape = &self.shape;
        if !self.has_pawns {
            let ei = &loaded.ei[bside as usize];
            let mut i = 0;
            while i < shape.num {
                i = fill_squares(b, &ei.pieces, flip, 0, &mut p, i);
            }
            let idx = encode::encode(&mut p, ei, shape, ENC_PIECE);
            (ei, idx)
        } else {
            let mirror = if flip { 0x38 } else { 0 };
            let mut i = fill_squares(b, &loaded.ei[0].pieces, flip, mirror, &mut p, 0);
            let f = encode::leading_pawn(&mut p, shape, ENC_FILE);
            let ei = &loaded.ei[f + 4 * bside as usize];
            while i < shape.num {
                i = fill_squares(b, &ei.pieces, flip, mirror, &mut p, i);
            }
            let idx = encode::encode(&mut p, ei, shape, ENC_FILE);
            (ei, idx)
        }
    }
}

/// Answers from the base tables (no WDL file for the material).
fn base_wdl(base: &Set, b: &Board) -> Option<i32> {
    let v = base.lookup(b)?;
    Some(if v.is_win() {
        WIN
    } else if v.is_loss() {
        LOSS
    } else {
        DRAW
    })
}

/// Reads the piece counts from the header of one of our files.
fn read_own_header(path: &Path, magic: &[u8; 4]) -> Option<([u8; 6], [u8; 6])> {
    use std::io::Read;
    let mut head = [0u8; write::OWN_HEADER];
    std::fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    if &head[..4] != magic || head[4] != write::OWN_VERSION {
        return None;
    }
    let mut first = [0u8; 6];
    let mut second = [0u8; 6];
    first.copy_from_slice(&head[8..14]);
    second.copy_from_slice(&head[14..20]);
    if first[0] != 1 || second[0] != 1 {
        return None;
    }
    Some((first, second))
}

/// The piece counts per kind of both colours.
fn counts(b: &Board) -> ([u8; 6], [u8; 6]) {
    let mut w = [0u8; 6];
    let mut bl = [0u8; 6];
    for kind in 0..6 {
        w[kind] = b.pieces[0][kind].count_ones() as u8;
        bl[kind] = b.pieces[1][kind].count_ones() as u8;
    }
    (w, bl)
}

fn is_en_passant(b: &Board, m: Move) -> bool {
    m.capture == Piece::NONE && m.to == b.en_passant && (b.squares[m.from.idx()].0 & Piece::TYPE_MASK.0) == Piece::PAWN.0
}

/// Appends the squares of the pieces with the Syzygy code pc[i] (1 = white
/// pawn .. 6 = white king, 9 .. 14 black; colours swapped with flip) to p,
/// as Syzygy squares (a1 = 0) XOR mirror, and returns the next index.
fn fill_squares(b: &Board, pc: &[u8], flip: bool, mirror: usize, p: &mut [usize], mut i: usize) -> usize {
    let mut color = (pc[i] >> 3) as usize;
    if flip {
        color ^= 1;
    }
    let kind = 6 - (pc[i] & 7) as usize; // Syzygy pawn 1 .. king 6, bitboard king 0 .. pawn 5
    let mut bb = b.pieces[color][kind];
    while bb != 0 {
        let sq = bb.trailing_zeros() as usize;
        p[i] = (sq ^ 0x38) ^ mirror; // a8 = 0 in the bitboard, a1 = 0 in Syzygy
        i += 1;
        bb &= bb - 1;
    }
    i
}

impl BaseEntry {
    /// Maps the file and reads its header: the piece orders and factors of
    /// every encoding, the pairs data and the block tables (Fathom's
    /// init_table for the WDL type).
    fn init(&self) -> std::io::Result<Loaded> {
        let mapping = Mapping::open(&self.path)?;
        let d = mapping.bytes();
        if d.len() < 5 || u32::from_le_bytes([d[0], d[1], d[2], d[3]]) != WDL_MAGIC {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("syzygy: {} is not a WDL table", self.name)));
        }
        let split = d[4] & 1 != 0;
        let mut pos = 5;
        let (num, enc) = if self.has_pawns { (4, ENC_FILE) } else { (1, ENC_PIECE) };
        let mut ei = vec![EncInfo::default(); 2 * num];
        let mut tb_size = [[0u64; 2]; 4];
        for t in 0..num {
            tb_size[t][0] = encode::init_enc_info(&mut ei[t], &self.shape, &d[pos..], 0, t, enc);
            if split {
                tb_size[t][1] = encode::init_enc_info(&mut ei[num + t], &self.shape, &d[pos..], 4, t, enc);
            }
            pos += self.shape.num + 1;
            if self.has_pawns && self.shape.pawns[1] != 0 {
                pos += 1;
            }
        }
        pos += pos & 1;
        let mut sizes = [[[0usize; 3]; 2]; 4];
        for t in 0..num {
            let (p, next, s) = PairsData::setup(d, pos, tb_size[t][0]);
            ei[t].precomp = Some(p);
            sizes[t][0] = s;
            pos = next;
            if split {
                let (p, next, s) = PairsData::setup(d, pos, tb_size[t][1]);
                ei[num + t].precomp = Some(p);
                sizes[t][1] = s;
                pos = next;
            }
        }
        for t in 0..num {
            ei[t].precomp.as_mut().unwrap().index_table = pos;
            pos += sizes[t][0][0];
            if split {
                ei[num + t].precomp.as_mut().unwrap().index_table = pos;
                pos += sizes[t][1][0];
            }
        }
        for t in 0..num {
            ei[t].precomp.as_mut().unwrap().size_table = pos;
            pos += sizes[t][0][1];
            if split {
                ei[num + t].precomp.as_mut().unwrap().size_table = pos;
                pos += sizes[t][1][1];
            }
        }
        for t in 0..num {
            // the mapping is page aligned, so file offsets align like addresses
            pos = (pos + 0x3f) & !0x3f;
            ei[t].precomp.as_mut().unwrap().data = pos;
            pos += sizes[t][0][2];
            if split {
                pos = (pos + 0x3f) & !0x3f;
                ei[num + t].precomp.as_mut().unwrap().data = pos;
                pos += sizes[t][1][2];
            }
        }
        if !split {
            for t in 0..num {
                ei[num + t] = ei[t].clone();
            }
        }
        Ok(Loaded { mapping, ei })
    }

    /// Maps one of our files and opens its streams (the counterpart of init
    /// for Syzygy files).
    fn init_own(&self) -> std::io::Result<Loaded> {
        let bad = |msg: String| std::io::Error::new(std::io::ErrorKind::InvalidData, msg);
        let mapping = Mapping::open(&self.path)?;
        let d = mapping.bytes();
        let magic = if self.dtm { write::DTM_MAGIC } else { write::OWN_MAGIC };
        if d.len() < write::OWN_HEADER || &d[..4] != magic || d[4] != write::OWN_VERSION {
            return Err(bad(format!("syzygy: {} is not an own {} table", self.name, if self.dtm { "DTM" } else { "WDL" })));
        }
        if self.dtm {
            self.step.store(d[20] as u32, Ordering::Relaxed);
        }
        let (sides, files) = (d[6] as usize, d[7] as usize);
        let (num, enc) = if self.has_pawns { (4, ENC_FILE) } else { (1, ENC_PIECE) };
        if files != num || sides != 2 - self.symmetric as usize || d.len() < write::OWN_HEADER + write::OWN_DESCRIPTOR * sides * files {
            return Err(bad(format!("syzygy: {} does not match its material", self.name)));
        }
        let mut ei = vec![EncInfo::default(); 2 * num];
        for side in 0..sides {
            for f in 0..files {
                let desc = &d[write::OWN_HEADER + write::OWN_DESCRIPTOR * (side * files + f)..];
                let e = &mut ei[side * num + f];
                e.pieces[..7].copy_from_slice(&desc[..7]);
                encode::setup(e, &self.shape, desc[7] as usize, desc[8] as usize, f, enc);
                let off = u64::from_le_bytes(desc[16..24].try_into().unwrap()) as usize;
                let len = u64::from_le_bytes(desc[24..32].try_into().unwrap()) as usize;
                if off + len > d.len() {
                    return Err(bad(format!("syzygy: {}: stream {} beyond the file", self.name, side * files + f)));
                }
                let t = crate::pairs::Table::open(&d[off..off + len]).map_err(|e| bad(format!("syzygy: {}: {}", self.name, e)))?;
                e.own = Some((std::sync::Arc::new(t), off, len));
            }
        }
        if sides == 1 {
            for f in 0..num {
                ei[num + f] = ei[f].clone();
            }
        }
        Ok(Loaded { mapping, ei })
    }
}
