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
pub const FILE_CHECKSUM: u64 = 0x7df825b0564bbe7e;

/// The checksums of the single tables (format v2, recorded 2026-10-07 from the
/// Rust generator, Go identical) and compared after every generation and load.
/// Larger materials are added as they are measured.
pub const TABLE_CHECKSUMS: &[(&str, u64)] = &[
    ("KQK", 0xc46f0039d9183a89),
    ("KRK", 0xb242548554aa6f9b),
    ("KBK", 0x87e64ca38cea838f),
    ("KNK", 0x289514f64bac1d0f),
    ("KQQK", 0x924b1b4b52caf42e),
    ("KQRK", 0x6c8a08fc3c1365bf),
    ("KQBK", 0x71626d49da26e26f),
    ("KQNK", 0x5ee4cd974f5fcc68),
    ("KRRK", 0x31a4c5182277769c),
    ("KRBK", 0x07352bb55b72623b),
    ("KRNK", 0x5192db5ddc19851b),
    ("KBBK", 0x5366a78b60d00d20),
    ("KBNK", 0xc044466bf38479fc),
    ("KNNK", 0x70171309263830f1),
    ("KQKQ", 0xbfdacb5f1f211bdc),
    ("KQKR", 0xdcf2a1f4a0ab2684),
    ("KQKB", 0xd201a4bd72abe6ac),
    ("KQKN", 0xe121d7a8d8cb7799),
    ("KRKR", 0x45f769817ad860bc),
    ("KRKB", 0x1738e67a374583fd),
    ("KRKN", 0x69f5936f1a695510),
    ("KBKB", 0xce7ed588fe9c163b),
    ("KBKN", 0xa4f14c3ecd9f1b9c),
    ("KNKN", 0xa42e774051095f94),
    ("KPK", 0xe7bf5853573f7e18),
    ("KQPK", 0xc44b15fda79131aa),
    ("KRPK", 0x1f2a1dea38c16ef6),
    ("KBPK", 0x7fdf1834a126aecd),
    ("KNPK", 0xb375a451fa224576),
    ("KQKP", 0xdc68d6d275d18fe9),
    ("KRKP", 0xf47d41f96cf7c962),
    ("KBKP", 0xa357723843c99501),
    ("KNKP", 0x6bee17ca085a8e37),
    ("KPPK", 0x8e7fbcb01d11a97a),
    ("KPKP", 0x531a4a773b5355b5),
    // five pieces (2026-10-07, Rust 12 threads: KBBBK 4 s, KBNKQ 46 s; Go identical)
    ("KBBBK", 0x985cad50bec78a9d),
    ("KBNKQ", 0xb52d6cdf4e9bee6f),
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
