//! The cache, see `mattjesGo/egtb/persist.go`: a directory next to the binary
//! with the base file (all tables up to four pieces in the order of
//! `Material::all()`) and one file per larger material ("<pieces>-<name>.bin",
//! never shipped, only generated locally).
//!
//! File format: 4 bytes magic "MEGT", 8 bytes raw checksum (the constant in the
//! code, over the values as generated, Invalid entries included), then the lz
//! container with the values. Before compression every Invalid entry is
//! replaced by its predecessor: those positions are never looked up, so their
//! content is free and the runs get longer. A file whose size equals the raw
//! table size is accepted as uncompressed (the layout before compression).
//! Go and Rust must produce byte-identical files.

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
pub const FILE_CHECKSUM: u64 = 0x7df825b0564bbe7e;

/// The raw checksums of the single tables (move count per byte, recorded
/// 2026-10-07 from the Rust generator, Go identical) and compared after every
/// generation and load. Larger materials are added as they are measured.
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
    // five pieces: all 110 materials from the measurement run of 2026-10-07
    // (measure-egtb.bat, Rust, 12 threads, 4953 s, every table forward-verified;
    // Go is byte-identical where checked). KPPKP is incomplete: its deepest
    // losses (254 plies) exceed the byte range and read as draws.
    ("KBBBK", 0x985cad50bec78a9d),
    ("KBBKB", 0x35aeff97f4dcf6e9),
    ("KBBKN", 0xd4e475056311e413),
    ("KBBKQ", 0xe7efffa62ded744d),
    ("KBBKR", 0xf1ff23cccfcfdc2f),
    ("KBBNK", 0xe5fedab06c47bf9f),
    ("KBNKB", 0xde72c4c91280419d),
    ("KBNKN", 0x664ecda0ec6e3aed),
    ("KBNKQ", 0xb52d6cdf4e9bee6f),
    ("KBNKR", 0x7426ac0fcf5e5565),
    ("KBNNK", 0xc3c3a9ec4b714bff),
    ("KNNKB", 0xdfd8153ec0c88027),
    ("KNNKN", 0x674398e2ff0a9a3a),
    ("KNNKQ", 0xc3cfe34d51c2073b),
    ("KNNKR", 0x654b31c1394f625f),
    ("KNNNK", 0x38031b8ca47ce417),
    ("KQBBK", 0x6a0ce04fa153f207),
    ("KQBKB", 0x681d91e6196bdc7b),
    ("KQBKN", 0x2ec6a87537b772d2),
    ("KQBKQ", 0x196cfbc1e93c3f5d),
    ("KQBKR", 0x29b6ed1ffa6d270a),
    ("KQBNK", 0xf91f7ea18fa1d5f6),
    ("KQNKB", 0xc992d53be8ca7d92),
    ("KQNKN", 0x1f0572e8de3f5dd7),
    ("KQNKQ", 0x359f224624fbdb4e),
    ("KQNKR", 0x16b8f7fe86f1a71a),
    ("KQNNK", 0xcf70620539854aeb),
    ("KQQBK", 0xdab613fd75f95631),
    ("KQQKB", 0x46585e1abb321c51),
    ("KQQKN", 0x5fef0fcd1fbd0bca),
    ("KQQKQ", 0xc93de3b5b227a414),
    ("KQQKR", 0x07d2ff57a51748f3),
    ("KQQNK", 0x0cfbbdde55c6c66e),
    ("KQQQK", 0x361952f9bce1e718),
    ("KQQRK", 0x501617ecfc7c5e65),
    ("KQRBK", 0xed765e57ef684436),
    ("KQRKB", 0x7f73fca2a02230ec),
    ("KQRKN", 0x044c25f7fd9be4ad),
    ("KQRKQ", 0x11f06758a8510b70),
    ("KQRKR", 0xdb37688bf9ae145f),
    ("KQRNK", 0x7838648715ebec28),
    ("KQRRK", 0xd678769751e609b9),
    ("KRBBK", 0x2d0e62ab37a33e57),
    ("KRBKB", 0x4d575853518d17cd),
    ("KRBKN", 0x39d00951cee153f0),
    ("KRBKQ", 0x80b2094027f2c1be),
    ("KRBKR", 0xc67d83f12317d718),
    ("KRBNK", 0xb1f78214b129a2f0),
    ("KRNKB", 0x36b6615f73ba2447),
    ("KRNKN", 0xbcdf9962966d3019),
    ("KRNKQ", 0x2dfcd5142e44c262),
    ("KRNKR", 0x72e07be541898ed6),
    ("KRNNK", 0x7b284d7a765ad7b5),
    ("KRRBK", 0x6ab053ad6e8b66c3),
    ("KRRKB", 0xcc40c5df13e820a2),
    ("KRRKN", 0xf5670bd98e3a1678),
    ("KRRKQ", 0x7e1c69eb8e6ebe80),
    ("KRRKR", 0x59544ee4a2dd269d),
    ("KRRNK", 0xf2d29f1e867e6927),
    ("KRRRK", 0x51e915b368c5a1b5),
    ("KBBKP", 0x2e020d9c3dcd2da9),
    ("KBBPK", 0x75bc3c5b438ca7f1),
    ("KBNKP", 0x1f81e70c0a562b46),
    ("KBNPK", 0xebd92a83040f5b03),
    ("KBPKB", 0xf346d62a2caf0698),
    ("KBPKN", 0x28811ff108a4addf),
    ("KBPKQ", 0x0d5fa8594a808917),
    ("KBPKR", 0x15169c4c1ccbb36e),
    ("KNNKP", 0x60911ba98b96fc7c),
    ("KNNPK", 0x71012ccc3138cb05),
    ("KNPKB", 0xc71e41cb6d5e8010),
    ("KNPKN", 0xea24076a0ad0fb74),
    ("KNPKQ", 0xb1b0a25ceea6df8f),
    ("KNPKR", 0x13cdabcbbbf4aa95),
    ("KQBKP", 0x631bec864c8f4f30),
    ("KQBPK", 0x1e077f9657bf7a12),
    ("KQNKP", 0x13173702ff6c02ab),
    ("KQNPK", 0x10bf49094145be1b),
    ("KQPKB", 0xacabeece0b69bd9f),
    ("KQPKN", 0xa810d786fb8610b0),
    ("KQPKQ", 0x1254ace34d1ad801),
    ("KQPKR", 0xcfd15f693d4b8b7b),
    ("KQQKP", 0x03622f01a50df0ec),
    ("KQQPK", 0x217d0b3d41cb3eb9),
    ("KQRKP", 0x98aa719c53818d6a),
    ("KQRPK", 0x9041b5e5d808351a),
    ("KRBKP", 0xf934270018d42bbb),
    ("KRBPK", 0x436749a08f8f9db6),
    ("KRNKP", 0x70b5f97e425f7909),
    ("KRNPK", 0xe157558ee56a6527),
    ("KRPKB", 0x7e94161b79efe51a),
    ("KRPKN", 0x048da519637ab404),
    ("KRPKQ", 0xde6853cf7896b836),
    ("KRPKR", 0xbf9c4faea5044fe3),
    ("KRRKP", 0xbe17e09f93cee4c3),
    ("KRRPK", 0x1a22fef3d7ebe443),
    ("KBPKP", 0x07a9bdc7c59e05aa),
    ("KBPPK", 0xe21fd507d5d11286),
    ("KNPKP", 0xa0748c8cd02c257a),
    ("KNPPK", 0xcd0fa706d6f44238),
    ("KPPKB", 0x1d9adcc39dfb48e9),
    ("KPPKN", 0x1630b2545112e003),
    ("KPPKQ", 0xc974efeaa1359647),
    ("KPPKR", 0xb41f4e743c946863),
    ("KQPKP", 0xd542a49825cf57d2),
    ("KQPPK", 0xa9bf9c3122e2b7e7),
    ("KRPKP", 0x634dde86a6ea07bb),
    ("KRPPK", 0x558f512241c5cfcb),
    ("KPPKP", 0xd2f3cdafa0bb5ca5),
    ("KPPPK", 0x5e63e63e36900d6f),
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

/// Reads a cache file into the tables (compressed or legacy raw). Returns the
/// raw checksum from the header (0 for a raw file with several tables) and
/// whether the file was compressed.
fn read_file(path: &Path, tables: &mut [&mut Table], workers: usize) -> std::io::Result<(u64, bool)> {
    let data = std::fs::read(path)?;
    let size: usize = tables.iter().map(|t| t.size).sum();
    for t in tables.iter_mut() {
        t.values = (0..t.size).map(|_| AtomicU8::new(0)).collect();
        t.set_filled(false);
        t.raw_checksum.store(0, std::sync::atomic::Ordering::Relaxed);
    }
    if data.len() == size {
        // legacy: raw values, Invalid entries present
        let mut off = 0;
        for t in tables.iter_mut() {
            let n = t.size;
            t.bytes_mut().copy_from_slice(&data[off..off + n]);
            off += n;
        }
        if tables.len() == 1 {
            return Ok((tables[0].raw_checksum(), false));
        }
        return Ok((0, false));
    }
    if data.len() < HEADER_BYTES || &data[..4] != MAGIC {
        return Err(io_err(format!("egtb: {} is neither a raw table ({} bytes) nor a cache file", path.display(), size)));
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
    Ok((raw_checksum, true))
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
        default_cache_dir().join(format!("{}-{}.bin", t.mat.pieces(), t.mat.name()))
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
        let (mut sum, compressed) = match result {
            Ok(r) => r,
            Err(e) => {
                self.clear_base();
                return Err(e);
            }
        };
        if !compressed {
            sum = self.checksum(); // raw file: hash the values
        } else {
            // the header carries the combined checksum of the generator run; the
            // single table checksums cannot be recomputed from filled values, so
            // the recorded constants stand in for them
            for t in &self.tables[..n] {
                t.raw_checksum.store(recorded_checksum(&t.mat.name()).unwrap_or(0), std::sync::atomic::Ordering::Relaxed);
            }
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
        let (sum, _) = match result {
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

/// Rewrites every raw table file in the cache directory in the compressed
/// format (the base file and all materials), one file at a time. Returns the
/// number of converted files and the raw and compressed byte totals.
pub fn compress_cache_dir(dir: &Path, workers: usize, progress: Progress) -> std::io::Result<(usize, u64, u64)> {
    let (mut converted, mut raw_bytes, mut packed_bytes) = (0usize, 0u64, 0u64);
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.ends_with(".bin") {
            continue;
        }
        let path = dir.join(&name);
        let size = e.metadata()?.len();
        let mut s = Set::new();
        let start = Instant::now();
        let sum;
        if name == BASE_FILE_NAME {
            if size as usize != s.base_size() {
                progress(&format!("egtb: {} already compressed", name));
                continue;
            }
            s.load(&path)?;
            sum = s.checksum();
            s.save(&path)?;
        } else {
            let stem = name.trim_end_matches(".bin");
            let Some((_, mat_name)) = stem.split_once('-') else { continue };
            let m = match Material::parse(mat_name) {
                Ok(m) => m,
                Err(err) => {
                    progress(&format!("egtb: skipping {}: {}", name, err));
                    continue;
                }
            };
            let ti = s.add_material(m);
            if size as usize != s.tables[ti].size {
                progress(&format!("egtb: {} already compressed", name));
                continue;
            }
            s.load_table(ti, &path)?;
            sum = s.tables[ti].raw_checksum();
            s.save_table(ti, &path)?;
        }
        let _ = workers;
        let after = std::fs::metadata(&path)?.len();
        converted += 1;
        raw_bytes += size;
        packed_bytes += after;
        progress(&format!("egtb: {} {} MB -> {} MB ({:.2}x) in {:.1} s, checksum {:016x}", name, size >> 20, after >> 20, size as f64 / after as f64, start.elapsed().as_secs_f64(), sum));
    }
    Ok((converted, raw_bytes, packed_bytes))
}
