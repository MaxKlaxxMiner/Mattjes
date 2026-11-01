//! The block cache of a table that stays compressed in its cache file,
//! mirrors `mattjesGo/egtb/blockstore.go`.

use std::fs::File;

use super::table::Value;
use crate::lz;

/// The budget of decoded blocks a packed table keeps in memory
/// (docs/m5-endgame-tables.md, "Blockformat"). Below the budget the table is
/// decoded lazily, block by block on first touch, and nothing is ever
/// evicted: a five-piece table (up to 242 MB) costs only the blocks a search
/// touches (a quarter of them for one KRR-KN solve). Above the budget (a
/// six-piece table, 15 GB) blocks are evicted direct-mapped, and a miss costs
/// a block read and decode (about 18 µs for 16 KB); df-pn touches a table
/// scattered, so a budget well below the working set slows a search down
/// (KRRKN with 32 MB: 2.8 times).
pub const CACHE_BYTES: usize = 2 << 30;

/// Serves the values of a table that stays compressed in its cache file: a
/// direct-mapped cache of decoded blocks (block number modulo slots), a miss
/// reads and decodes one compressed block. Used by the search's lookups only
/// (the generator works on full arrays, a generation job forks the set
/// without packed tables); the table wraps it in a mutex, because lookups
/// take a shared reference.
pub struct BlockStore {
    file: File,
    index: lz::Index,
    /// Decoded blocks, allocated on first use.
    slots: Vec<Vec<u8>>,
    /// Block number held in each slot, -1 = empty.
    tags: Vec<i32>,
    /// One compressed block.
    scratch: Vec<u8>,
    pub hits: u64,
    pub misses: u64,
    /// Bitmap of the blocks ever decoded: the working set of a search.
    seen: Vec<u64>,
    pub distinct: u64,
    /// The first read error (the table then answers Draw, reported once).
    pub err: Option<String>,
    /// The entries of a wide table (0 = a one-byte table): the file holds
    /// the low plane, then the high plane (`Value::to_planes`).
    wide_entries: usize,
}

impl BlockStore {
    /// `wide_entries` is the table size of a wide table, 0 otherwise.
    pub fn new(file: File, index: lz::Index, wide_entries: usize) -> BlockStore {
        let slots = (CACHE_BYTES / index.block_size).max(1).min(index.blocks());
        let scratch = vec![0u8; index.max_packed()];
        let seen = vec![0u64; index.blocks().div_ceil(64)];
        BlockStore { file, index, slots: (0..slots).map(|_| Vec::new()).collect(), tags: vec![-1; slots], scratch, hits: 0, misses: 0, seen, distinct: 0, err: None, wide_entries }
    }

    pub fn blocks(&self) -> usize {
        self.index.blocks()
    }

    pub fn reset_stats(&mut self) {
        self.hits = 0;
        self.misses = 0;
        self.distinct = 0;
        self.seen.fill(0);
    }

    /// The value of entry idx: one byte, or the two planes of a wide table.
    pub fn value(&mut self, idx: usize) -> Value {
        if self.wide_entries == 0 {
            return Value::from_byte(self.byte(idx));
        }
        let lo = self.byte(idx);
        Value::from_planes(lo, self.byte(self.wide_entries + idx))
    }

    /// The byte at raw position idx, decoding its block on a miss (a read
    /// error gives 0).
    fn byte(&mut self, idx: usize) -> u8 {
        let block = idx / self.index.block_size;
        let slot = block % self.slots.len();
        if self.tags[slot] != block as i32 {
            self.misses += 1;
            if self.seen[block >> 6] & (1 << (block & 63)) == 0 {
                self.seen[block >> 6] |= 1 << (block & 63);
                self.distinct += 1;
            }
            if self.slots[slot].is_empty() {
                self.slots[slot] = vec![0u8; self.index.block_size];
            }
            if let Err(e) = self.index.read_block(&self.file, block, &mut self.scratch, &mut self.slots[slot]) {
                if self.err.is_none() {
                    self.err = Some(format!("egtb: block {}: {}", block, e));
                }
                self.tags[slot] = -1;
                return 0;
            }
            self.tags[slot] = block as i32;
        } else {
            self.hits += 1;
        }
        self.slots[slot][idx - block * self.index.block_size]
    }
}

/// The decoded blocks one thread keeps of the tables held compressed in
/// memory (`MemStore`): 64 MB, 4,096 blocks of 16 KB, two GB for 32 workers.
pub const THREAD_CACHE_BYTES: usize = 64 << 20;

/// The block size a `MemStore` serves (the exact files' `BLOCK_BYTES`).
const MEM_BLOCK: usize = 16 << 10;

/// Hands out the ids that tell the tables apart in the thread caches.
static NEXT_STORE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// A direct-mapped cache of decoded blocks, one per thread: the tag of a
/// slot is (store id << 32 | block + 1), 0 = empty.
struct ThreadCache {
    tags: Vec<u64>,
    data: Vec<u8>,
}

thread_local! {
    static CACHE: std::cell::RefCell<ThreadCache> = const { std::cell::RefCell::new(ThreadCache { tags: Vec::new(), data: Vec::new() }) };
}

/// Serves a dependency the generator cannot hold as an array (stage 2 of
/// the sweep's memory plan, 2026-11-01): the whole compressed file sits in
/// memory, a quarter of the array or less, and every worker decodes the
/// blocks it touches into its own cache (`THREAD_CACHE_BYTES`), so that 32
/// workers look up without a lock. A miss decodes 16 KB from memory.
pub struct MemStore {
    id: u64,
    index: lz::Index,
    packed: Vec<u8>,
    /// The entries of a wide table (0 = a one-byte table), as in `BlockStore`.
    wide_entries: usize,
    /// Blocks decoded by all threads together.
    pub misses: std::sync::atomic::AtomicU64,
}

impl MemStore {
    pub fn new(file: &File, index: lz::Index, wide_entries: usize) -> std::io::Result<MemStore> {
        if index.block_size != MEM_BLOCK {
            return Err(std::io::Error::other(format!("egtb: blocks of {} bytes, the memory store serves {}", index.block_size, MEM_BLOCK)));
        }
        let packed = index.read_packed(file)?;
        let id = NEXT_STORE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(MemStore { id, index, packed, wide_entries, misses: std::sync::atomic::AtomicU64::new(0) })
    }

    /// The bytes the store holds (the compressed blocks).
    pub fn bytes(&self) -> usize {
        self.packed.len()
    }

    /// The value of entry idx: one byte, or the two planes of a wide table.
    pub fn value(&self, idx: usize) -> Value {
        if self.wide_entries == 0 {
            return Value::from_byte(self.byte(idx));
        }
        Value::from_planes(self.byte(idx), self.byte(self.wide_entries + idx))
    }

    /// The byte at raw position idx from this thread's cache, decoding its
    /// block on a miss. A block that does not decode is a damaged file: the
    /// generator must not go on with wrong values.
    fn byte(&self, idx: usize) -> u8 {
        let block = idx / MEM_BLOCK;
        let tag = self.id << 32 | (block as u64 + 1);
        CACHE.with(|c| {
            let mut c = c.borrow_mut();
            let slots = THREAD_CACHE_BYTES / MEM_BLOCK;
            if c.tags.is_empty() {
                c.tags = vec![0; slots];
                c.data = vec![0; THREAD_CACHE_BYTES];
            }
            let slot = (block + self.id as usize * 7919) % slots;
            if c.tags[slot] != tag {
                let dst = &mut c.data[slot * MEM_BLOCK..(slot + 1) * MEM_BLOCK];
                if let Err(e) = self.index.decode_block(&self.packed, block, dst) {
                    panic!("egtb: block {} of a table in memory: {}", block, e);
                }
                c.tags[slot] = tag;
                self.misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            c.data[slot * MEM_BLOCK + idx % MEM_BLOCK]
        })
    }
}
