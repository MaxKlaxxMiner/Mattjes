//! File format: a 32-byte little-endian header followed by the raw entries. The
//! fingerprint identifies the Zobrist constants (`bitboard::zobrist_fingerprint`),
//! so a file written with other constants is rejected instead of producing silent
//! false hits. Go and Rust use the same constants and the same layout, so a table
//! saved by one can be loaded by the other.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use super::buckets::{Bucket, Buckets, BUCKET_BYTES};
use super::table::{Entry, Table, ENTRY_BYTES};

const FILE_MAGIC: u32 = 0x5454_4A4D; // "MJTT"
const FILE_VERSION: u32 = 1;
const KIND_TABLE: u32 = 1;
const KIND_BUCKETS: u32 = 2;

struct Header {
    magic: u32,
    version: u32,
    kind: u32,
    entry_bytes: u32,
    slots: u64,
    fingerprint: u64,
}

impl Header {
    fn to_bytes(&self) -> [u8; 32] {
        let mut b = [0u8; 32];
        b[0..4].copy_from_slice(&self.magic.to_le_bytes());
        b[4..8].copy_from_slice(&self.version.to_le_bytes());
        b[8..12].copy_from_slice(&self.kind.to_le_bytes());
        b[12..16].copy_from_slice(&self.entry_bytes.to_le_bytes());
        b[16..24].copy_from_slice(&self.slots.to_le_bytes());
        b[24..32].copy_from_slice(&self.fingerprint.to_le_bytes());
        b
    }

    fn from_bytes(b: &[u8; 32]) -> Header {
        let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        let u64_at = |i: usize| u64::from_le_bytes(b[i..i + 8].try_into().unwrap());
        Header { magic: u32_at(0), version: u32_at(4), kind: u32_at(8), entry_bytes: u32_at(12), slots: u64_at(16), fingerprint: u64_at(24) }
    }
}

/// Views a slice of plain-old-data values as bytes. Only used with `#[repr(C)]`
/// types made of integers, for which every bit pattern is valid.
fn as_bytes<T: Copy>(s: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}

fn as_bytes_mut<T: Copy>(s: &mut [T]) -> &mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(s.as_mut_ptr() as *mut u8, std::mem::size_of_val(s)) }
}

fn write_file(path: &Path, h: &Header, body: &[u8]) -> Result<(), String> {
    let mut f = File::create(path).map_err(|e| e.to_string())?;
    f.write_all(&h.to_bytes()).map_err(|e| e.to_string())?;
    f.write_all(body).map_err(|e| e.to_string())
}

/// Validates magic, version, kind, entry size and fingerprint and returns the
/// header; the file position is at the first entry afterwards.
fn read_header(f: &mut File, kind: u32, entry_bytes: u32, fingerprint: u64) -> Result<Header, String> {
    let mut b = [0u8; 32];
    f.read_exact(&mut b).map_err(|e| e.to_string())?;
    let h = Header::from_bytes(&b);
    if h.magic != FILE_MAGIC {
        return Err("not a tt file".into());
    }
    if h.version != FILE_VERSION {
        return Err(format!("tt file version {}, expected {}", h.version, FILE_VERSION));
    }
    if h.kind != kind {
        return Err(format!("tt file kind {}, expected {}", h.kind, kind));
    }
    if h.entry_bytes != entry_bytes {
        return Err(format!("tt file entry size {}, expected {}", h.entry_bytes, entry_bytes));
    }
    if h.fingerprint != fingerprint {
        return Err(format!("tt file written with other zobrist constants ({:016x}, expected {:016x})", h.fingerprint, fingerprint));
    }
    if h.slots == 0 || !h.slots.is_power_of_two() {
        return Err(format!("tt file slot count {} is not a power of two", h.slots));
    }
    Ok(h)
}

impl Table {
    /// Dumps the table to `path`.
    pub fn save(&self, path: &Path, fingerprint: u64) -> Result<(), String> {
        let h = Header { magic: FILE_MAGIC, version: FILE_VERSION, kind: KIND_TABLE, entry_bytes: ENTRY_BYTES as u32, slots: self.entries.len() as u64, fingerprint };
        write_file(path, &h, as_bytes(&self.entries))
    }

    /// Reads a table written by `save`. Statistics start at zero.
    pub fn load(path: &Path, fingerprint: u64) -> Result<Table, String> {
        let mut f = File::open(path).map_err(|e| e.to_string())?;
        let h = read_header(&mut f, KIND_TABLE, ENTRY_BYTES as u32, fingerprint)?;
        let mut t = Table::with_slots(h.slots as usize);
        f.read_exact(as_bytes_mut::<Entry>(&mut t.entries)).map_err(|e| e.to_string())?;
        Ok(t)
    }
}

impl Buckets {
    /// Dumps the buckets to `path`.
    pub fn save(&self, path: &Path, fingerprint: u64) -> Result<(), String> {
        let h = Header { magic: FILE_MAGIC, version: FILE_VERSION, kind: KIND_BUCKETS, entry_bytes: BUCKET_BYTES as u32, slots: self.buckets.len() as u64, fingerprint };
        write_file(path, &h, as_bytes(&self.buckets))
    }

    /// Reads a table written by `Buckets::save`.
    pub fn load(path: &Path, fingerprint: u64) -> Result<Buckets, String> {
        let mut f = File::open(path).map_err(|e| e.to_string())?;
        let h = read_header(&mut f, KIND_BUCKETS, BUCKET_BYTES as u32, fingerprint)?;
        let mut t = Buckets::with_buckets(h.slots as usize);
        f.read_exact(as_bytes_mut::<Bucket>(&mut t.buckets)).map_err(|e| e.to_string())?;
        Ok(t)
    }
}
