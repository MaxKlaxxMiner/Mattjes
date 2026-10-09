//! The cache, see `mattjesGo/egtb/persist.go`: a directory next to the binary
//! with the base file (all tables up to four pieces in the order of
//! `Material::all()`) and one file per larger material ("<pieces>-<name>.bin",
//! never shipped, only generated locally).
//!
//! File format: 4 bytes magic "MEGT", 8 bytes raw checksum (the constant in the
//! code, over the values as generated, dead and illegal entries as 0), then the
//! lz container with the values. Before compression every dead or illegal
//! entry is replaced by its predecessor: those positions are never looked up,
//! so their content is free and the runs get longer. Go and Rust must produce
//! byte-identical files.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU8;
use std::time::Instant;

use super::generate::{Progress, Stats};
use super::table::{Set, Table};
use super::Material;
use crate::lz;

pub const CACHE_DIR: &str = "mattjes-egtb-cache";
pub const BASE_FILE_NAME: &str = "4-all.bin";
const MAGIC: &[u8; 4] = b"MEGT";
const HEADER_BYTES: usize = 12;

/// Threads used to compress and decompress cache files (independent 4 MB blocks).
pub const IO_WORKERS: usize = 8;

/// The checksum of the complete base (a hash of the 35 table checksums). It
/// doubles as the regression test of the generator: a changed generator that
/// still produces this value produces the same tables. 0 means "not yet
/// recorded".
pub const FILE_CHECKSUM: u64 = 0x64fe872c7e217f6e; // format v3 (2026-10-08): losses up to 254 plies, no invalid value

/// The raw checksums of the single tables (format v3: move count per byte,
/// dead and illegal entries 0) and compared after every generation and load.
/// Larger materials are added as they are measured; the five-piece values of
/// format v2 (2026-10-07) are in the git history and in
/// docs/m5-endgame-tables.md.
pub const TABLE_CHECKSUMS: &[(&str, u64)] = &[
    ("KQK", 0x9260104b203b89c4),
    ("KRK", 0x5387c89faf9e0217),
    ("KBK", 0x73f143eadfe811a8),
    ("KNK", 0x73f143eadfe811a8), // = KBK: all zero
    ("KQQK", 0xea8550ab91d6bf04),
    ("KQRK", 0x7e56203083caed07),
    ("KQBK", 0x0daa2fa637ffbf47),
    ("KQNK", 0xfa22d9cbcc5750d2),
    ("KRRK", 0x9fa46d416e96e031),
    ("KRBK", 0x0fcf7cb7a35f16b7),
    ("KRNK", 0xf61bbd3f8e767a38),
    ("KBBK", 0x14956d0d156a61b2),
    ("KBNK", 0x3c55de9ecf98feb6),
    ("KNNK", 0xf3fe22b4f30f1651),
    ("KQKQ", 0xda1249305b769fb7),
    ("KQKR", 0xb7d88c4c91896ef9),
    ("KQKB", 0x2a2334b0f5d15047),
    ("KQKN", 0x2e37a9157d56f895),
    ("KRKR", 0xfb8ee6c6d503053c),
    ("KRKB", 0xefa63d9931328cda),
    ("KRKN", 0xc020cf7e4db95b94),
    ("KBKB", 0x5fef424e80401f69),
    ("KBKN", 0x5f096e24292f2568),
    ("KNKN", 0x667f4a21cc8aaab2),
    ("KPK", 0xb1d19b5318c5076c),
    ("KQPK", 0x1c74eb840969f447),
    ("KRPK", 0x05107c598fafd5ec),
    ("KBPK", 0x7943932114500468),
    ("KNPK", 0xa6464dc2cc6a01f8),
    ("KQKP", 0x24b8b54772f35819),
    ("KRKP", 0x57b1f53eaee0a89c),
    ("KBKP", 0x74931eaafaa29dd8),
    ("KNKP", 0x627ee5ef42b600f5),
    ("KPPK", 0x829f48ba8cf92ce4),
    ("KPKP", 0xae084110af4ab131),
    // five pieces (format v3, 2026-10-09, Go and Rust identical), added as they are measured
    ("KBBBK", 0xa552a70f5be1c3db),
    ("KBNKQ", 0xa03df227d9f5eb72),
];

/// The recorded checksum of a table, if any.
pub fn recorded_checksum(name: &str) -> Option<u64> {
    TABLE_CHECKSUMS.iter().find(|(n, _)| *n == name).map(|&(_, c)| c)
}

/// A 64-bit hash over 8-byte little-endian words (FNV-1a style with a final
/// mix), identical to the Go implementation.
pub fn checksum(data: &[u8]) -> u64 {
    const PRIME: u64 = 0x100000001b3;
    let mut h: u64 = 0xcbf29ce484222325;
    let mut words = data.chunks_exact(8);
    for w in &mut words {
        let w = u64::from_le_bytes(w.try_into().unwrap());
        h = (h ^ w).wrapping_mul(PRIME);
        h ^= h >> 29;
    }
    for &b in words.remainder() {
        h = (h ^ b as u64).wrapping_mul(PRIME);
    }
    h ^= h >> 32;
    h = h.wrapping_mul(0x9e3779b97f4a7c15);
    h ^= h >> 29;
    h
}

/// The cache directory next to the running binary.
pub fn default_cache_dir() -> PathBuf {
    match std::env::current_exe() {
        Ok(exe) => exe.with_file_name(CACHE_DIR),
        Err(_) => PathBuf::from(CACHE_DIR),
    }
}

/// The base cache file next to the running binary.
pub fn default_path() -> PathBuf {
    default_cache_dir().join(BASE_FILE_NAME)
}

fn io_err(msg: String) -> std::io::Error {
    std::io::Error::other(msg)
}

fn write_file(path: &Path, tables: &[&Table], raw_checksum: u64, workers: usize) -> std::io::Result<()> {
    let size: usize = tables.iter().map(|t| t.size).sum();
    let mut raw = Vec::with_capacity(size);
    for t in tables {
        if !t.is_generated() {
            return Err(io_err(format!("egtb: table {} not generated", t.mat.name())));
        }
        t.fill();
        raw.extend_from_slice(t.bytes());
    }
    let packed = lz::pack(&raw, workers);
    let mut out = Vec::with_capacity(HEADER_BYTES + packed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&raw_checksum.to_le_bytes());
    out.extend_from_slice(&packed);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, out)
}

/// Reads a cache file into the tables. Returns the raw checksum from the
/// header; the entries are filled (dead and illegal ones hold their
/// predecessor).
fn read_file(path: &Path, tables: &mut [&mut Table], workers: usize) -> std::io::Result<u64> {
    let data = std::fs::read(path)?;
    let size: usize = tables.iter().map(|t| t.size).sum();
    for t in tables.iter_mut() {
        t.clear();
        t.values = (0..t.size).map(|_| AtomicU8::new(0)).collect();
    }
    if data.len() < HEADER_BYTES || &data[..4] != MAGIC {
        return Err(io_err(format!("egtb: {} is not a cache file", path.display())));
    }
    let raw_checksum = u64::from_le_bytes(data[4..12].try_into().unwrap());
    let packed = &data[HEADER_BYTES..];
    match lz::raw_len(packed) {
        Ok(n) if n == size => {}
        Ok(n) => return Err(io_err(format!("egtb: {} holds {} bytes, expected {}", path.display(), n, size))),
        Err(e) => return Err(io_err(format!("egtb: {}: {}", path.display(), e))),
    }
    if tables.len() == 1 {
        lz::unpack_into(tables[0].bytes_mut(), packed, workers).map_err(|e| io_err(format!("egtb: {}: {}", path.display(), e)))?;
    } else {
        let mut dst = vec![0u8; size];
        lz::unpack_into(&mut dst, packed, workers).map_err(|e| io_err(format!("egtb: {}: {}", path.display(), e)))?;
        let mut off = 0;
        for t in tables.iter_mut() {
            let n = t.size;
            t.bytes_mut().copy_from_slice(&dst[off..off + n]);
            off += n;
        }
    }
    for t in tables.iter_mut() {
        t.set_filled(true);
    }
    Ok(raw_checksum)
}

impl Set {
    /// Checksum of the base tables in file order (they must be complete): a
    /// hash of the per-table hashes keeps the tables independent.
    pub fn checksum(&self) -> u64 {
        let mut h: u64 = 0x9e3779b97f4a7c15;
        for t in self.base() {
            h = (h ^ t.raw_checksum()).wrapping_mul(0x100000001b3);
            h ^= h >> 31;
        }
        h
    }

    /// The cache file of a material beyond the base, e.g. "5-KBNKQ.bin".
    pub fn table_path(&self, t: &Table) -> PathBuf {
        self.cache_dir.join(format!("{}-{}.bin", t.mat.pieces(), t.mat.name()))
    }

    /// The base cache file in the set's cache directory.
    pub fn base_path(&self) -> PathBuf {
        self.cache_dir.join(BASE_FILE_NAME)
    }

    fn clear_base(&mut self) {
        let n = self.base().len();
        for t in &mut self.tables[..n] {
            t.clear();
        }
    }

    /// Writes the base tables to path (compressed).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tables: Vec<&Table> = self.base().iter().collect();
        write_file(path, &tables, self.checksum(), IO_WORKERS)
    }

    /// Reads the base tables from path and checks size and checksum.
    pub fn load(&mut self, path: &Path) -> std::io::Result<()> {
        let n = self.base().len();
        let result = {
            let mut tables: Vec<&mut Table> = self.tables[..n].iter_mut().collect();
            read_file(path, &mut tables, IO_WORKERS)
        };
        let sum = match result {
            Ok(r) => r,
            Err(e) => {
                self.clear_base();
                return Err(e);
            }
        };
        // the header carries the combined checksum of the generator run; the
        // single table checksums cannot be recomputed from filled values, so
        // the recorded constants stand in for them
        for t in &self.tables[..n] {
            t.raw_checksum.store(recorded_checksum(&t.mat.name()).unwrap_or(0), std::sync::atomic::Ordering::Relaxed);
        }
        if FILE_CHECKSUM != 0 && sum != FILE_CHECKSUM {
            self.clear_base();
            return Err(io_err(format!("egtb: {} has checksum {:016x}, expected {:016x}", path.display(), sum, FILE_CHECKSUM)));
        }
        Ok(())
    }

    /// Writes one table (a material beyond the base) to path (compressed).
    pub fn save_table(&self, ti: usize, path: &Path) -> std::io::Result<()> {
        let t = &self.tables[ti];
        if !t.is_generated() {
            return Err(io_err(format!("egtb: table {} not generated", t.mat.name())));
        }
        write_file(path, &[t], t.raw_checksum(), IO_WORKERS)
    }

    /// Reads one table from path and checks size and, when recorded, the checksum.
    pub fn load_table(&mut self, ti: usize, path: &Path) -> std::io::Result<()> {
        let result = read_file(path, &mut [&mut self.tables[ti]], IO_WORKERS);
        let t = &mut self.tables[ti];
        let sum = match result {
            Ok(r) => r,
            Err(e) => {
                t.clear();
                return Err(e);
            }
        };
        t.raw_checksum.store(sum, std::sync::atomic::Ordering::Relaxed);
        if let Some(want) = recorded_checksum(&t.mat.name()) {
            if sum != want {
                t.clear();
                return Err(io_err(format!("egtb: {} has checksum {:016x}, expected {:016x}", path.display(), sum, want)));
            }
        }
        Ok(())
    }

    /// The complete base set: from the cache file when it is present and
    /// correct, otherwise freshly generated, checked against the constants and
    /// written.
    pub fn load_or_generate(path: &Path, workers: usize, write: bool, progress: Progress) -> Set {
        let mut s = Set::new();
        if let Some(dir) = path.parent() {
            s.cache_dir = dir.to_path_buf();
        }
        let start = Instant::now();
        match s.load(path) {
            Ok(()) => {
                progress(&format!("egtb: loaded {} MB from {} in {:.2} s", s.base_size() >> 20, path.display(), start.elapsed().as_secs_f64()));
                return s;
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => progress(&format!("egtb: {}", e)),
            Err(_) => {}
        }
        let (mut done, total) = (0, s.base().len());
        s.generate_all(workers, &mut |line| {
            if line.contains(" wins ") {
                done += 1; // one summary line per table, no levels
                progress(&format!("({} / {}) {}", done, total, line));
            }
        });
        progress(&format!("egtb: generated {} tables in {:.1} s", s.base().len(), start.elapsed().as_secs_f64()));
        s.check_checksums(progress);
        if !write {
            for t in s.base() {
                t.fill();
            }
            return s;
        }
        match s.save(path) {
            Ok(()) => progress(&format!("egtb: written to {}", path.display())),
            Err(e) => progress(&format!("egtb: {}", e)),
        }
        s
    }

    /// The table of a material beyond the base: from its cache file when
    /// present and correct, otherwise generated (with its dependencies) and
    /// written. Returns (table index, stats, loaded); the stats are only
    /// meaningful after a generation.
    pub fn load_or_generate_table(&mut self, m: Material, workers: usize, progress: Progress) -> (usize, Stats, bool) {
        let ti = self.add_material(m);
        if self.tables[ti].is_generated() {
            return (ti, Stats::default(), true);
        }
        let path = self.table_path(&self.tables[ti]);
        match self.load_table(ti, &path) {
            Ok(()) => {
                progress(&format!("egtb: loaded {} ({} MB)", path.display(), self.tables[ti].size >> 20));
                return (ti, Stats::default(), true);
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => progress(&format!("egtb: {}", e)),
            Err(_) => {}
        }
        let st = self.generate(ti, workers, progress);
        match self.save_table(ti, &path) {
            Ok(()) => progress(&format!("egtb: written to {}", path.display())),
            Err(e) => progress(&format!("egtb: {}", e)),
        }
        (ti, st, false)
    }

    /// Compares every base table and the whole base with the recorded constants
    /// and reports differences (or the values to record, when missing). Returns
    /// the number of mismatches.
    pub fn check_checksums(&self, progress: Progress) -> usize {
        let mut bad = 0;
        for t in self.base() {
            let sum = t.raw_checksum();
            let name = t.mat.name();
            match recorded_checksum(&name) {
                None => progress(&format!("  (\"{}\", 0x{:016x}),", name, sum)),
                Some(want) if sum != want => {
                    bad += 1;
                    progress(&format!("egtb: {} checksum {:016x}, expected {:016x}", name, sum, want));
                }
                _ => {}
            }
        }
        let sum = self.checksum();
        if FILE_CHECKSUM == 0 {
            progress(&format!("egtb: file checksum 0x{:016x} (record it as FILE_CHECKSUM)", sum));
        } else if sum != FILE_CHECKSUM {
            bad += 1;
            progress(&format!("egtb: file checksum {:016x}, expected {:016x}", sum, FILE_CHECKSUM));
        } else {
            progress("egtb: checksums ok");
        }
        bad
    }
}

