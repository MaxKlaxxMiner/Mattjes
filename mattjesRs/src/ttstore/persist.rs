//! File format: 40-byte little-endian header, then the raw slots. Same
//! fingerprint rule as module `tt`.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use super::{Key, Store, ENTRY_BYTES};

const FILE_MAGIC: u32 = 0x5354_4A4D; // "MJTS"
const FILE_VERSION: u32 = 1;
const KIND_STORE: u32 = 3;

fn as_bytes(s: &[Key]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}

fn as_bytes_mut(s: &mut [Key]) -> &mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(s.as_mut_ptr() as *mut u8, std::mem::size_of_val(s)) }
}

impl Store {
    /// Dumps the store to `path`.
    pub fn save(&self, path: &Path, fingerprint: u64) -> Result<(), String> {
        let mut h = [0u8; 40];
        h[0..4].copy_from_slice(&FILE_MAGIC.to_le_bytes());
        h[4..8].copy_from_slice(&FILE_VERSION.to_le_bytes());
        h[8..12].copy_from_slice(&KIND_STORE.to_le_bytes());
        h[12..16].copy_from_slice(&(ENTRY_BYTES as u32).to_le_bytes());
        h[16..24].copy_from_slice(&(self.entries.len() as u64).to_le_bytes());
        h[24..32].copy_from_slice(&(self.count as u64).to_le_bytes());
        h[32..40].copy_from_slice(&fingerprint.to_le_bytes());
        let mut f = File::create(path).map_err(|e| e.to_string())?;
        f.write_all(&h).map_err(|e| e.to_string())?;
        f.write_all(as_bytes(&self.entries)).map_err(|e| e.to_string())
    }

    /// Reads a store written by `save`.
    pub fn load(path: &Path, fingerprint: u64) -> Result<Store, String> {
        let mut f = File::open(path).map_err(|e| e.to_string())?;
        let mut h = [0u8; 40];
        f.read_exact(&mut h).map_err(|e| e.to_string())?;
        let u32_at = |i: usize| u32::from_le_bytes(h[i..i + 4].try_into().unwrap());
        let u64_at = |i: usize| u64::from_le_bytes(h[i..i + 8].try_into().unwrap());
        let (magic, version, kind, entry_bytes) = (u32_at(0), u32_at(4), u32_at(8), u32_at(12));
        let (slots, count, fp) = (u64_at(16), u64_at(24), u64_at(32));
        if magic != FILE_MAGIC {
            return Err("not a ttstore file".into());
        }
        if version != FILE_VERSION {
            return Err(format!("ttstore file version {}, expected {}", version, FILE_VERSION));
        }
        if kind != KIND_STORE {
            return Err(format!("ttstore file kind {}, expected {}", kind, KIND_STORE));
        }
        if entry_bytes as usize != ENTRY_BYTES {
            return Err(format!("ttstore file entry size {}, expected {}", entry_bytes, ENTRY_BYTES));
        }
        if fp != fingerprint {
            return Err(format!("ttstore file written with other zobrist constants ({:016x}, expected {:016x})", fp, fingerprint));
        }
        if slots == 0 || !slots.is_power_of_two() {
            return Err(format!("ttstore file slot count {} is not a power of two", slots));
        }
        let mut s = Store::with_slots(slots as usize);
        s.count = count as usize;
        f.read_exact(as_bytes_mut(&mut s.entries)).map_err(|e| e.to_string())?;
        Ok(s)
    }
}
