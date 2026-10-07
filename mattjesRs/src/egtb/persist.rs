//! The cache, see `mattjesGo/egtb/persist.go`: a directory next to the binary
//! with the base file (all tables up to four pieces in the order of
//! `Material::all()`, raw values, no header) and one file per larger material
//! ("<pieces>-<name>.bin", never shipped, only generated locally). Layouts are
//! fixed in code and the checksums are known constants, so loading is read,
//! hash, compare. Go and Rust must produce byte-identical tables.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::generate::{Progress, Stats};
use super::table::{Set, Table};
use super::Material;

pub const CACHE_DIR: &str = "mattjes-egtb-cache";
pub const BASE_FILE_NAME: &str = "4-all.bin";

/// The checksum of the complete base file. It doubles as the regression test
/// of the generator: a changed generator that still produces this value
/// produces the same tables. 0 means "not yet recorded".
pub const FILE_CHECKSUM: u64 = 0x865d59cc4efc880b;

/// The checksums of the single tables, recorded once from the Go generator
/// (2026-10-05) and compared by both generators after every generation.
/// Larger materials are added as they are measured.
pub const TABLE_CHECKSUMS: &[(&str, u64)] = &[
    ("KQK", 0x426e5bcfec52d05a),
    ("KRK", 0x6ebfdb596a00048f),
    ("KBK", 0x87e64ca38cea838f),
    ("KNK", 0x289514f64bac1d0f),
    ("KQQK", 0x8508ca59e98829f3),
    ("KQRK", 0x8e05463b5ff01aec),
    ("KQBK", 0xc530420b344a7195),
    ("KQNK", 0x5d69959d3de7dbad),
    ("KRRK", 0xdcc21130b554325a),
    ("KRBK", 0x309970b951da8967),
    ("KRNK", 0xc0edd92ed250667b),
    ("KBBK", 0x8958cfb6fd19de80),
    ("KBNK", 0xb1a138142ed9cdbe),
    ("KNNK", 0x70171309263830f1),
    ("KQKQ", 0x372ac8be4859c45a),
    ("KQKR", 0xcf68ccf411a0e752),
    ("KQKB", 0xe5ded49325b2e363),
    ("KQKN", 0x57701e4d919b3f82),
    ("KRKR", 0x895350eb52831767),
    ("KRKB", 0x4f1733d5c4fb6500),
    ("KRKN", 0x4bc8f873d71d6518),
    ("KBKB", 0xce7ed588fe9c163b),
    ("KBKN", 0xa4f14c3ecd9f1b9c),
    ("KNKN", 0xa42e774051095f94),
    ("KPK", 0x54ca397ab9f573fe),
    ("KQPK", 0xf4e064a417fad31b),
    ("KRPK", 0xe338e13091c22c21),
    ("KBPK", 0x69d6ff4cc467b63a),
    ("KNPK", 0xb943826d8ef75909),
    ("KQKP", 0xee37cff419c86800),
    ("KRKP", 0x1dadf3c74939058f),
    ("KBKP", 0xac967d51e02db1cd),
    ("KNKP", 0x1f1a6259674a3d69),
    ("KPPK", 0xd7cc08bb6c92e81b),
    ("KPKP", 0x4efb9e3c5e9f75fb),
    // five pieces (2026-10-07, Go, 47 s on 12 threads)
    ("KBNKQ", 0x7b54498535f836cb),
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

fn values_from(data: &[u8]) -> Vec<std::sync::atomic::AtomicU8> {
    data.iter().map(|&b| std::sync::atomic::AtomicU8::new(b)).collect()
}

impl Set {
    /// Checksum of the base tables in file order (they must be complete): a
    /// hash of the per-table hashes keeps the tables independent.
    pub fn checksum(&self) -> u64 {
        let mut h: u64 = 0x9e3779b97f4a7c15;
        for t in self.base() {
            h = (h ^ checksum(t.bytes())).wrapping_mul(0x100000001b3);
            h ^= h >> 31;
        }
        h
    }

    /// The cache file of a material beyond the base, e.g. "5-KBNKQ.bin".
    pub fn table_path(&self, t: &Table) -> PathBuf {
        default_cache_dir().join(format!("{}-{}.bin", t.mat.pieces(), t.mat.name()))
    }

    /// Writes the base tables to path.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        use std::io::Write;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
        for t in self.base() {
            if !t.is_generated() {
                return Err(std::io::Error::other(format!("egtb: table {} not generated", t.mat.name())));
            }
            f.write_all(t.bytes())?;
        }
        f.flush()
    }

    /// Reads the base tables from path and checks size and checksum.
    pub fn load(&mut self, path: &Path) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        if data.len() != self.base_size() {
            return Err(std::io::Error::other(format!("egtb: {} has {} bytes, expected {}", path.display(), data.len(), self.base_size())));
        }
        let n = self.base().len();
        let mut off = 0;
        for t in &mut self.tables[..n] {
            t.values = values_from(&data[off..off + t.size]);
            off += t.size;
        }
        let sum = self.checksum();
        if FILE_CHECKSUM != 0 && sum != FILE_CHECKSUM {
            for t in &mut self.tables[..n] {
                t.values = Vec::new();
            }
            return Err(std::io::Error::other(format!("egtb: {} has checksum {:016x}, expected {:016x}", path.display(), sum, FILE_CHECKSUM)));
        }
        Ok(())
    }

    /// Writes one table (a material beyond the base) to path.
    pub fn save_table(&self, ti: usize, path: &Path) -> std::io::Result<()> {
        let t = &self.tables[ti];
        if !t.is_generated() {
            return Err(std::io::Error::other(format!("egtb: table {} not generated", t.mat.name())));
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, t.bytes())
    }

    /// Reads one table from path and checks size and, when recorded, the checksum.
    pub fn load_table(&mut self, ti: usize, path: &Path) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        let t = &mut self.tables[ti];
        if data.len() != t.size {
            return Err(std::io::Error::other(format!("egtb: {} has {} bytes, expected {}", path.display(), data.len(), t.size)));
        }
        t.values = values_from(&data);
        if let Some(want) = recorded_checksum(&t.mat.name()) {
            let sum = checksum(t.bytes());
            if sum != want {
                t.values = Vec::new();
                return Err(std::io::Error::other(format!("egtb: {} has checksum {:016x}, expected {:016x}", path.display(), sum, want)));
            }
        }
        Ok(())
    }

    /// The complete base set: from the cache file when it is present and
    /// correct, otherwise freshly generated, checked against the constants and
    /// written.
    pub fn load_or_generate(path: &Path, workers: usize, progress: Progress) -> Set {
        let mut s = Set::new();
        let start = Instant::now();
        match s.load(path) {
            Ok(()) => {
                progress(&format!("egtb: loaded {} MB from {} in {:.2} s", s.base_size() >> 20, path.display(), start.elapsed().as_secs_f64()));
                return s;
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => progress(&format!("egtb: {}", e)),
            Err(_) => {}
        }
        s.generate_all(workers, &mut |_| {});
        progress(&format!("egtb: generated {} tables in {:.1} s", s.base().len(), start.elapsed().as_secs_f64()));
        s.check_checksums(progress);
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
            let sum = checksum(t.bytes());
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
