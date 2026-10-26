//! The cache, see `mattjesGo/egtb/persist.go`: a directory next to the binary
//! with the base file (all tables up to four pieces in the order of
//! `Material::all()`) and one file per larger material ("<pieces>-<name>.bin",
//! never shipped, only generated locally).
//!
//! File format (v3, 2026-10-16): 4 bytes magic "MEG3", 1 byte step (the
//! resolution of the values in moves, `Table::step`: 1 = exact, n = buckets of
//! n moves), 1 byte value width (0 or 1 = one byte per entry, 2 = a wide
//! table, since 2026-10-26: the container then holds 2n bytes, the low plane
//! of all entries, then the high plane, `Value::to_planes`), 2 bytes zero, 8
//! bytes raw checksum (the constant in the code, over
//! the exact values as generated, dead and illegal entries as 0), then the
//! indexed lz container with the values (`lz::pack_indexed_to`: block offsets
//! up front, so a table beyond the base is read block by block through a
//! block cache instead of being unpacked as a whole; the base uses 4 MB blocks
//! and is unpacked, the rest `BLOCK_BYTES`). Before compression every dead or
//! illegal entry is replaced by its predecessor: those positions are never
//! looked up, so their content is free and the runs get longer. Tables beyond
//! the base are written quantised to `FILE_STEP` (the search resolves the
//! exact distance from the bounds, docs/m5-endgame-tables.md); the base is
//! always exact. Go and Rust must produce byte-identical files.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::generate::{Progress, Stats};
use super::table::{Set, Table, Value};
use super::Material;
use crate::lz;

pub const CACHE_DIR: &str = "mattjes-egtb-cache";

/// The cache folder holds the base file at its root; the tables beyond the
/// base live in two subfolders: the (quantised) distance tables and our own
/// WDL tables in Syzygy's structure.
pub const DTM_DIR: &str = "dtm";
pub const WDL_DIR: &str = "wdl";
pub const BASE_FILE_NAME: &str = "4-all.bin";
const MAGIC: &[u8; 4] = b"MEG3";
const HEADER_BYTES: usize = 16;
/// The raw block size of the files beyond the base. Measured on KRRKN
/// (docs/m5-endgame-tables.md, "Blockformat"): 64 KB costs 5 % file size over
/// 4 MB blocks but a search touches 45 % of the table, 16 KB costs 17 % and
/// touches 27 %, 4 KB costs 40 % and touches 15 %; a miss decodes one block,
/// 16 KB in about 18 µs. 16 KB is the compromise.
pub const BLOCK_BYTES: usize = 16 << 10;

/// The resolution in moves of every table beyond the base in the cache files
/// ("mate in at most 8 moves" instead of the exact count). It halves the files
/// twice over against exact values; the search resolves the 16-ply window in
/// about 0.3 s at the root (KRR-KN, KQ-KBN), step 4 would take 1 ms and save
/// one halving less. The base stays exact (37 MB), and the generator needs
/// exact dependencies, so a quantised cache file is never used as one
/// (`generate_controlled` regenerates it in memory instead).
pub const FILE_STEP: u32 = 8;

/// Marks the exact twin of a cache file ("5-KBNKQ.bin.raw"): the same
/// container, but step 1. A generation writes it for every dependency it had
/// to compute, so that an interrupted job keeps them and a later generation
/// can load them instead of computing them again (the quantised file cannot
/// serve as a dependency). The user deletes them by hand.
pub const RAW_SUFFIX: &str = ".raw";

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
    // five pieces, all 110 (format v3; from the sweep of 2026-10-23 on the home machine, Rust,
    // measure.log; KBBBK, KBNKQ and KRRKN agree with the earlier Go and Rust runs)
    ("KBBBK", 0xa552a70f5be1c3db), ("KBBKB", 0x72234cc21a7defaf), ("KBBKN", 0xcac529d6b477774b),
    ("KBBKQ", 0xd12a7747cc7d9f45), ("KBBKR", 0x25fcfbb6a10810e8), ("KBBNK", 0x234d18bcb557ad7f),
    ("KBNKB", 0xfd2f437a0a54b862), ("KBNKN", 0x12113e9a5e7c4e48), ("KBNKQ", 0xa03df227d9f5eb72),
    ("KBNKR", 0x187c046383539781), ("KBNNK", 0xda5cd512f702000a), ("KNNKB", 0xdaa5176ca837e544),
    ("KNNKN", 0x2cadd8a85308b7cf), ("KNNKQ", 0xb723b4fae7715b91), ("KNNKR", 0x3b77145d4f6957c5),
    ("KNNNK", 0xa44c31959604c935), ("KQBBK", 0x54972d4b84afa912), ("KQBKB", 0x08c8342dc0e58d3e),
    ("KQBKN", 0xd1fbb25d97b6f3a3), ("KQBKQ", 0x3aa37ee109fb585f), ("KQBKR", 0x0be7752be640c86c),
    ("KQBNK", 0x81586b6e6f1ccc6d), ("KQNKB", 0xa342715277bab8c6), ("KQNKN", 0x10749f36888b538a),
    ("KQNKQ", 0x878542d0f0bdfe00), ("KQNKR", 0x9f4a7fdb8db76e86), ("KQNNK", 0xfafc1f102f4d3edb),
    ("KQQBK", 0x6b79d5e753baa689), ("KQQKB", 0xfed65a59f7c286cf), ("KQQKN", 0x5eaeaf8457ca0e25),
    ("KQQKQ", 0x32f36f5b986ac0de), ("KQQKR", 0x67d79a19f27e30ad), ("KQQNK", 0xd7696e03b378e695),
    ("KQQQK", 0x26ccd333c5249437), ("KQQRK", 0x46018430785e3443), ("KQRBK", 0x24bdc7ea83a73e37),
    ("KQRKB", 0xaa14ca6028d40b9a), ("KQRKN", 0xc67bd8787b1cb2d4), ("KQRKQ", 0x9ace8ab5334edd2f),
    ("KQRKR", 0x280529af63d896c6), ("KQRNK", 0x2e0b74455ddcdb70), ("KQRRK", 0x5925eaafa071bf0d),
    ("KRBBK", 0x8973a57db42adaae), ("KRBKB", 0xec0971f832a3ec14), ("KRBKN", 0x8a0c296adea78498),
    ("KRBKQ", 0x37b8a37e2578a4c4), ("KRBKR", 0xf1eeeb2878de49a7), ("KRBNK", 0x777caf348dd076b9),
    ("KRNKB", 0x74b159efc1a16d9c), ("KRNKN", 0xf8abb5381fb82d8a), ("KRNKQ", 0x5ebac3e35ae6129d),
    ("KRNKR", 0xf176012ec8aa7c0f), ("KRNNK", 0x8067f92cf8f05172), ("KRRBK", 0x955121c5e7f34b2a),
    ("KRRKB", 0x1b2da1009c1b09ab), ("KRRKN", 0x443a1b3b48954ec3), ("KRRKQ", 0xd7a15047fa0ad275),
    ("KRRKR", 0xbe96f115eba44051), ("KRRNK", 0xa75e70e701bbf8ac), ("KRRRK", 0x38031bfa0e98bbee),
    ("KBBKP", 0x11ded18695795d6c), ("KBBPK", 0x1e90b35dd894b11e), ("KBNKP", 0xfe0d3bb9d3e5667f),
    ("KBNPK", 0xcc3f466efcc94af1), ("KBPKB", 0x3d83b0fd492bc18e), ("KBPKN", 0x4000947a38cd27ea),
    ("KBPKQ", 0x5365d229c58e74e2), ("KBPKR", 0xc9718c864cbcc955), ("KNNKP", 0x2fe018737a9a2387),
    ("KNNPK", 0xd83c464672e30fe1), ("KNPKB", 0x71772e64bd51012a), ("KNPKN", 0x5e0bfb8e51dd84e5),
    ("KNPKQ", 0x42deff8d914c91da), ("KNPKR", 0xc48002f00bdb4b09), ("KQBKP", 0x5fe49081bd07c0d9),
    ("KQBPK", 0xca397e89179e8f73), ("KQNKP", 0x835f7e19d227df05), ("KQNPK", 0x36e0f2e1db352887),
    ("KQPKB", 0x4828f117c85b5f39), ("KQPKN", 0xb70e06a378558b50), ("KQPKQ", 0x629bb099a6a90e07),
    ("KQPKR", 0xce224ecf69d3fdb4), ("KQQKP", 0xa5674b645afe063b), ("KQQPK", 0x15aa245d511d4a3b),
    ("KQRKP", 0xc3f60f7b8c6ee78a), ("KQRPK", 0x6a3d308b1f468e66), ("KRBKP", 0x23b40adbdf6c4fed),
    ("KRBPK", 0x01df5843fde7058a), ("KRNKP", 0x97ff32bf6d501a1e), ("KRNPK", 0x77e0598d862784ac),
    ("KRPKB", 0x0d3e7a4b90d9e182), ("KRPKN", 0x57d74af89ec77093), ("KRPKQ", 0x3055982894f56ddc),
    ("KRPKR", 0x7cc6ba239baf5da9), ("KRRKP", 0x3003050b376b4546), ("KRRPK", 0x8a1494460fb7f479),
    ("KBPKP", 0x75ea61ed8bf95531), ("KBPPK", 0xecc3de59fb86dff7), ("KNPKP", 0x7c2e668122934016),
    ("KNPPK", 0xba0f257aa2ecc2ed), ("KPPKB", 0xcc9a627ad3f55921), ("KPPKN", 0x5c456acab2bbab19),
    ("KPPKQ", 0x902875aaa6992709), ("KPPKR", 0x4c92580848463cd6), ("KQPKP", 0xfeeca92d9b94747d),
    ("KQPPK", 0x0826d0219e8be912), ("KRPKP", 0x62929c193fa9a46d), ("KRPPK", 0x71e1d1f22a9e53e8),
    ("KPPKP", 0x87cb3b021b3416b0), ("KPPPK", 0x9b0c4f0c85c11d02),
    // six pieces (format v3, Rust, home machine): KRRKBN 1294 s, 35 GiB committed
    ("KRRKBN", 0x11049fde56c4b77c),
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

/// Writes header + compressed values in blocks of `block_size`. The tables
/// are filled first; `step` is the resolution they all share (recorded in the
/// header).
fn write_file(path: &Path, tables: &[&Table], raw_checksum: u64, step: u32, block_size: usize, workers: usize) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // written beside the target and renamed when complete: a reader never
    // sees a partial file (a sweep may be killed while writing)
    let tmp = tmp_path(path);
    let mut w = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(&tmp)?);
    write_to(&mut w, tables, raw_checksum, step, block_size, workers)?;
    use std::io::Write;
    w.flush()?;
    drop(w);
    std::fs::rename(&tmp, path)
}

/// Writes a file that was packed into memory before (`Set::pack_raw_table`),
/// through its temporary twin like `write_file`: the sweep hands this to a
/// thread of its own, so the next material is computed meanwhile.
pub fn write_packed(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = tmp_path(path);
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

/// The content of a cache file: header and compressed values.
fn write_to<W: std::io::Write>(w: &mut W, tables: &[&Table], raw_checksum: u64, step: u32, block_size: usize, workers: usize) -> std::io::Result<()> {
    for t in tables {
        if !t.is_generated() {
            return Err(io_err(format!("egtb: table {} not generated", t.mat.name())));
        }
        if t.step() != step {
            return Err(io_err(format!("egtb: table {} has step {}, the file needs {}", t.mat.name(), t.step(), step)));
        }
        t.fill();
    }
    let wide = tables.len() == 1 && tables[0].is_wide();
    // a single table (every material beyond the base) is packed in place: no
    // copy of 15 GB for a six-piece table; only the base file concatenates
    let joined: Vec<u8>;
    let raw: &[u8] = if tables.len() == 1 {
        tables[0].bytes()
    } else {
        let size: usize = tables.iter().map(|t| t.size).sum();
        let mut v = Vec::with_capacity(size);
        for t in tables {
            v.extend_from_slice(t.bytes());
        }
        joined = v;
        &joined
    };
    w.write_all(MAGIC)?;
    w.write_all(&[step as u8, if wide { 2 } else { 1 }, 0, 0])?;
    w.write_all(&raw_checksum.to_le_bytes())?;
    if wide {
        // the two planes, built block by block from the entries
        let v = tables[0].wide_values();
        let n = v.len();
        lz::pack_indexed_with(w, 2 * n, block_size, workers, &|off, dst| {
            for (j, b) in dst.iter_mut().enumerate() {
                let pos = off + j;
                *b = if pos < n { Value(v[pos]).to_planes().0 } else { Value(v[pos - n]).to_planes().1 };
            }
        })?;
    } else {
        lz::pack_indexed_to(w, raw, block_size, workers)?;
    }
    Ok(())
}

/// The temporary twin a file is written to before it is renamed into place
/// ("5-KBNKQ.dtm.tmp"): a reader never sees a partial file.
pub fn tmp_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".tmp");
    PathBuf::from(p)
}

/// Opens a cache file and reads its header and block index; the caller either
/// unpacks everything (`read_file`) or keeps the file for block reads
/// (`open_table`). `size` is the number of entries the tables expect.
/// Returns the file, the index, the step, the raw checksum and whether the
/// entries are wide (two planes).
fn open_file(path: &Path, size: usize) -> std::io::Result<(std::fs::File, lz::Index, u32, u64, bool)> {
    let f = std::fs::File::open(path)?;
    let mut header = [0u8; HEADER_BYTES];
    if lz::read_exact_at(&f, &mut header, 0).is_err() || &header[..4] != MAGIC {
        return Err(io_err(format!("egtb: {} is not a cache file (format {:?}, regenerate)", path.display(), String::from_utf8_lossy(&header[..4]))));
    }
    let step = (header[4] as u32).max(1);
    let wide = match header[5] {
        0 | 1 => false,
        2 => true,
        w => return Err(io_err(format!("egtb: {} has {} bytes per entry", path.display(), w))),
    };
    let raw_checksum = u64::from_le_bytes(header[8..16].try_into().unwrap());
    let ix = lz::Index::read(&f, HEADER_BYTES as u64).map_err(|e| io_err(format!("egtb: {}: {}", path.display(), e)))?;
    let want = if wide { 2 * size } else { size };
    if ix.raw_len != want {
        return Err(io_err(format!("egtb: {} holds {} bytes, expected {}", path.display(), ix.raw_len, want)));
    }
    Ok((f, ix, step, raw_checksum, wide))
}

/// Reads a cache file into the tables as a whole (the base). Returns the raw
/// checksum from the header; the entries are filled (dead and illegal ones
/// hold their predecessor) and the tables carry the file's step.
fn read_file(path: &Path, tables: &mut [&mut Table], workers: usize) -> std::io::Result<u64> {
    let size: usize = tables.iter().map(|t| t.size).sum();
    for t in tables.iter_mut() {
        t.clear();
    }
    let (f, ix, step, raw_checksum, wide) = open_file(path, size)?;
    if wide && tables.len() != 1 {
        return Err(io_err(format!("egtb: {} is wide, only single tables can be", path.display())));
    }
    for t in tables.iter_mut() {
        t.allocate(wide);
    }
    // decompress into one buffer, then split (the tables are separate slices)
    if wide {
        // each plane block ors its byte into the entries (low and high
        // byte), then every entry turns from the plane form into its value
        let t = &tables[0];
        let n = t.size;
        ix.unpack_each(&f, workers, &|off, data| {
            for (j, &b) in data.iter().enumerate() {
                let pos = off + j;
                if pos < n {
                    t.wide[pos].fetch_or(b as u16, Ordering::Relaxed);
                } else {
                    t.wide[pos - n].fetch_or((b as u16) << 8, Ordering::Relaxed);
                }
            }
        })
        .map_err(|e| io_err(format!("egtb: {}: {}", path.display(), e)))?;
        let chunks = t.wide.chunks(1 << 20).collect::<Vec<_>>();
        let next = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..workers.max(1) {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(c) = chunks.get(i) else { break };
                    for e in c.iter() {
                        let raw = e.load(Ordering::Relaxed);
                        e.store(Value::from_planes(raw as u8, (raw >> 8) as u8).0, Ordering::Relaxed);
                    }
                });
            }
        });
    } else if tables.len() == 1 {
        ix.unpack_all(&f, tables[0].bytes_mut(), workers).map_err(|e| io_err(format!("egtb: {}: {}", path.display(), e)))?;
    } else {
        let mut dst = vec![0u8; size];
        ix.unpack_all(&f, &mut dst, workers).map_err(|e| io_err(format!("egtb: {}: {}", path.display(), e)))?;
        let mut off = 0;
        for t in tables.iter_mut() {
            let n = t.size;
            t.bytes_mut().copy_from_slice(&dst[off..off + n]);
            off += n;
        }
    }
    for t in tables.iter_mut() {
        t.set_filled(true);
        t.set_step(step);
    }
    Ok(raw_checksum)
}

/// Opens a cache file for block reads: the table stays compressed in the
/// file, a block cache (`CACHE_BYTES`) serves the lookups. Returns the raw
/// checksum from the header.
fn open_table(path: &Path, t: &mut Table) -> std::io::Result<u64> {
    t.clear();
    let (f, ix, step, raw_checksum, wide) = open_file(path, t.size)?;
    t.set_packed(super::blockstore::BlockStore::new(f, ix, if wide { t.size } else { 0 }));
    t.set_filled(true);
    t.set_step(step);
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

    /// The cache file of a material beyond the base, e.g. "dtm/5-KBNKQ.bin".
    pub fn table_path(&self, t: &Table) -> PathBuf {
        self.dtm_path().join(format!("{}-{}.bin", t.mat.pieces(), t.mat.name()))
    }

    /// The folder of the distance tables beyond the base.
    pub fn dtm_path(&self) -> PathBuf {
        self.cache_dir.join(DTM_DIR)
    }

    /// The folder of our own WDL tables (syzygy::write).
    pub fn wdl_path(&self) -> PathBuf {
        self.cache_dir.join(WDL_DIR)
    }

    /// The own WDL table of a material, e.g. "wdl/5-KBNKQ.wdl".
    pub fn wdl_file(&self, m: &Material) -> PathBuf {
        self.wdl_path().join(format!("{}-{}.wdl", m.pieces(), m.name()))
    }

    /// The own DTM table of a material, e.g. "dtm/5-KBNKQ.dtm".
    pub fn dtm_file(&self, m: &Material) -> PathBuf {
        self.dtm_path().join(format!("{}-{}.dtm", m.pieces(), m.name()))
    }

    /// Drops the values of a table beyond the base from memory (and closes
    /// its file, when it was loaded packed); the table stays registered and
    /// can be loaded or generated again.
    pub fn release(&mut self, ti: usize) {
        self.tables[ti].clear();
    }

    /// The exact cache file of a material beyond the base ("5-KBNKQ.bin.raw",
    /// `RAW_SUFFIX`).
    pub fn raw_path(&self, t: &Table) -> PathBuf {
        let mut p = self.table_path(t).into_os_string();
        p.push(RAW_SUFFIX);
        PathBuf::from(p)
    }

    /// Reads the material of a cache file name as `table_path` writes it
    /// ("5-KRRKN.bin", or its exact twin "5-KRRKN.bin.raw"); None for any
    /// other name, the base file included.
    pub fn parse_file_name(name: &str) -> Option<Material> {
        let name = name.strip_suffix(RAW_SUFFIX).unwrap_or(name);
        let base = name.strip_suffix(".bin")?;
        let (_, mat_name) = base.split_once('-')?;
        let m = Material::parse(mat_name).ok()?;
        if format!("{}-{}.bin", m.pieces(), m.name()) != name {
            return None;
        }
        Some(m)
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
        write_file(path, &tables, self.checksum(), 1, lz::BLOCK_SIZE, IO_WORKERS)
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
        if !self.tables[0].is_exact() {
            let step = self.tables[0].step();
            self.clear_base();
            return Err(io_err(format!("egtb: {} is quantised (step {}), the base must be exact", path.display(), step)));
        }
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

    /// Writes one table (a material beyond the base) to path (compressed). The
    /// table is quantised to `FILE_STEP` **in place** first (no copy of 15 GB
    /// for a six-piece table): afterwards it is what a reload gives, the search
    /// resolves its bounds, and it can no longer serve the generator as a
    /// dependency. The header keeps the checksum of the exact values. Verify
    /// before saving.
    pub fn save_table(&self, ti: usize, path: &Path) -> std::io::Result<()> {
        let t = &self.tables[ti];
        if !t.is_generated() {
            return Err(io_err(format!("egtb: table {} not generated", t.mat.name())));
        }
        let sum = t.raw_checksum(); // of the exact values, before quantising
        if t.is_exact() {
            t.quantize(FILE_STEP);
        }
        write_file(path, &[t], sum, t.step(), BLOCK_BYTES, IO_WORKERS)
    }

    /// Writes one exact table (a material beyond the base) to path
    /// unquantised (`raw_path`): the values stay exact in memory, the file
    /// can serve a later generation as a dependency. The table must still be
    /// exact.
    pub fn save_raw_table(&self, ti: usize, path: &Path) -> std::io::Result<()> {
        self.save_raw_table_with(ti, path, IO_WORKERS)
    }

    /// The exact file of a table (`save_raw_table`) packed into memory on
    /// `workers` threads, for `write_packed`: only the compressed bytes.
    pub fn pack_raw_table(&self, ti: usize, workers: usize) -> std::io::Result<Vec<u8>> {
        let t = &self.tables[ti];
        if !t.is_exact() {
            return Err(io_err(format!("egtb: table {} is quantised (step {}), no exact file", t.mat.name(), t.step())));
        }
        let mut v = Vec::new();
        write_to(&mut v, &[t], t.raw_checksum(), 1, BLOCK_BYTES, workers)?;
        Ok(v)
    }

    /// `save_raw_table` packed on `workers` threads.
    pub fn save_raw_table_with(&self, ti: usize, path: &Path, workers: usize) -> std::io::Result<()> {
        let t = &self.tables[ti];
        if !t.is_generated() {
            return Err(io_err(format!("egtb: table {} not generated", t.mat.name())));
        }
        if !t.is_exact() {
            return Err(io_err(format!("egtb: table {} is quantised (step {}), no exact file", t.mat.name(), t.step())));
        }
        write_file(path, &[t], t.raw_checksum(), 1, BLOCK_BYTES, workers)
    }

    /// Reads an exact table (`raw_path`) completely into memory, as a
    /// generation needs its dependencies. Checks the step, the size and, when
    /// recorded, the checksum.
    pub fn load_raw_table(&mut self, ti: usize, path: &Path) -> std::io::Result<()> {
        self.load_raw_table_checked(ti, path, true)
    }

    /// `load_raw_table`, comparing with the recorded checksum only when
    /// `check` is set (a test widens a recorded one-byte table on purpose).
    pub fn load_raw_table_checked(&mut self, ti: usize, path: &Path, check: bool) -> std::io::Result<()> {
        let result = read_file(path, &mut [&mut self.tables[ti]], IO_WORKERS);
        let t = &mut self.tables[ti];
        let sum = match result {
            Ok(r) => r,
            Err(e) => {
                t.clear();
                return Err(e);
            }
        };
        if !t.is_exact() {
            let step = t.step();
            t.clear();
            return Err(io_err(format!("egtb: {} is quantised (step {}), expected an exact table", path.display(), step)));
        }
        t.raw_checksum.store(sum, std::sync::atomic::Ordering::Relaxed);
        if let Some(want) = recorded_checksum(&t.mat.name()).filter(|_| check) {
            if sum != want {
                t.clear();
                return Err(io_err(format!("egtb: {} has checksum {:016x}, expected {:016x}", path.display(), sum, want)));
            }
        }
        Ok(())
    }

    /// Writes the exact file (`raw_path`) and the quantised file
    /// (`table_path`) of a table that is exact in memory, each only when
    /// missing, and leaves the table exact: `save_table` quantises in place,
    /// so the exact file is loaded back afterwards (0.4 s for a five-piece
    /// table). For a background job's dependencies, which later tables of the
    /// job still need exactly. `progress` gets a line per written file.
    /// Returns the first error.
    pub fn save_both(&mut self, ti: usize, progress: Progress) -> std::io::Result<()> {
        let (raw, quantised) = (self.raw_path(&self.tables[ti]), self.table_path(&self.tables[ti]));
        if !self.tables[ti].is_exact() {
            return Err(io_err(format!("egtb: table {} is quantised (step {}), no exact file", self.tables[ti].mat.name(), self.tables[ti].step())));
        }
        if !raw.exists() {
            self.save_raw_table(ti, &raw)?;
            progress(&format!("egtb: written exact to {}", raw.display()));
        }
        if !quantised.exists() {
            self.save_table(ti, &quantised)?;
            progress(&format!("egtb: written to {}", quantised.display()));
            self.load_raw_table(ti, &raw)?;
        }
        Ok(())
    }

    /// Opens one table (a material beyond the base) from path: it stays
    /// compressed in the file and is read block by block (`open_table`).
    /// Checks size and, when recorded, the checksum.
    pub fn load_table(&mut self, ti: usize, path: &Path) -> std::io::Result<()> {
        self.load_table_checked(ti, path, true)
    }

    /// `load_table`, comparing with the recorded checksum only when `check`
    /// is set.
    pub fn load_table_checked(&mut self, ti: usize, path: &Path, check: bool) -> std::io::Result<()> {
        let result = open_table(path, &mut self.tables[ti]);
        let t = &mut self.tables[ti];
        let sum = match result {
            Ok(r) => r,
            Err(e) => {
                t.clear();
                return Err(e);
            }
        };
        t.raw_checksum.store(sum, std::sync::atomic::Ordering::Relaxed);
        if let Some(want) = recorded_checksum(&t.mat.name()).filter(|_| check) {
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

