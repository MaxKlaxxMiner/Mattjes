use super::Key;

/// 16 bytes: `word0` is key word 0 with its low index bits replaced by the value,
/// `word1` is key word 1 complete. Both words 0 marks an empty slot (a stored
/// entry that is all zero has probability 2^-104 per position and is ignored).
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub(super) struct Entry {
    pub word0: u64,
    pub word1: u64,
}

impl Entry {
    #[inline(always)]
    pub fn new(k: Key, mask: u64, value: u64) -> Entry {
        Entry { word0: (k[0] & !mask) | value, word1: k[1] }
    }

    /// Whether the entry holds key k, whose index bits are implied by the slot.
    #[inline(always)]
    pub fn matches(&self, k: Key, mask: u64) -> bool {
        self.word1 == k[1] && (self.word0 ^ k[0]) & !mask == 0
    }

    #[inline(always)]
    pub fn empty(&self) -> bool {
        self.word0 | self.word1 == 0
    }

    #[inline(always)]
    pub fn value(&self, mask: u64) -> u64 {
        self.word0 & mask
    }
}

pub const ENTRY_BYTES: usize = std::mem::size_of::<Entry>();

/// Counts what happened since the last `reset_stats`. `foreign` counts entries of
/// other positions a probe looked at; `near_miss[i]` counts how often the low
/// 16 / 32 / 48 bits of their second key word matched anyway, i.e. how many false
/// hits a table that stored only that many check bits would have produced.
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub probes: u64,
    pub hits: u64,
    pub stores: u64,
    pub replaced: u64,
    pub foreign: u64,
    pub near_miss: [u64; 3],
}

impl Stats {
    #[inline(always)]
    pub(super) fn foreign(&mut self, check: u64, want: u64) {
        self.foreign += 1;
        let x = check ^ want;
        if x & 0xFFFF != 0 {
            return;
        }
        self.near_miss[0] += 1;
        if x & 0xFFFF_FFFF != 0 {
            return;
        }
        self.near_miss[1] += 1;
        if x & 0xFFFF_FFFF_FFFF == 0 {
            self.near_miss[2] += 1;
        }
    }
}

/// What the experiments need from a table; both layouts implement it. Generic
/// callers are monomorphized, so there is no dynamic dispatch in the hot path.
pub trait TransTable {
    fn probe(&mut self, k: Key) -> Option<u64>;
    /// Brings the slot of a key into the cache, so that a probe of several
    /// keys in a row overlaps the misses instead of waiting for them one by
    /// one (the two random accesses per child dominate the df-pn profile).
    fn prefetch(&self, k: Key);
    /// The value must not exceed `max_value`.
    fn store(&mut self, k: Key, value: u64);
    /// The largest storable value (= slots - 1 for a direct table).
    fn max_value(&self) -> u64;
    /// Empties all slots but keeps the statistics.
    fn clear(&mut self);
    fn slots(&self) -> usize;
    /// Occupied slots (full scan).
    fn used(&self) -> usize;
    fn stats(&self) -> &Stats;
    fn reset_stats(&mut self);
    /// The width of a stored value (= log2 of the slot or bucket count).
    fn value_bits(&self) -> u32 {
        self.max_value().count_ones()
    }
}

/// The largest power of two n with n * slot_bytes <= size_mb MB.
pub(super) fn slots_for(size_mb: usize, slot_bytes: usize) -> usize {
    let mut n = 1;
    while n * 2 * slot_bytes <= size_mb << 20 {
        n *= 2;
    }
    n
}

/// The direct-mapped layout: one entry per slot, always replace.
pub struct Table {
    pub(super) entries: Vec<Entry>,
    mask: u64, // index bits = value bits
    pub stats: Stats,
}

impl Table {
    /// Allocates the largest power-of-two table that fits into `size_mb`.
    pub fn new(size_mb: usize) -> Table {
        Table::with_slots(slots_for(size_mb, ENTRY_BYTES))
    }

    pub(super) fn with_slots(slots: usize) -> Table {
        Table { entries: vec![Entry::default(); slots], mask: slots as u64 - 1, stats: Stats::default() }
    }

    pub fn bytes(&self) -> usize {
        self.entries.len() * ENTRY_BYTES
    }
}

/// Issues a prefetch for the cache line at `p` (a no-op off x86-64).
#[inline(always)]
pub(super) fn prefetch_line(p: *const i8) {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: a prefetch never faults, whatever the address.
    unsafe {
        use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
        _mm_prefetch::<_MM_HINT_T0>(p);
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = p;
}

impl TransTable for Table {
    #[inline(always)]
    fn prefetch(&self, k: Key) {
        prefetch_line(self.entries.as_ptr().wrapping_add((k[0] & self.mask) as usize) as *const i8);
    }

    #[inline(always)]
    fn probe(&mut self, k: Key) -> Option<u64> {
        self.stats.probes += 1;
        let e = &self.entries[(k[0] & self.mask) as usize];
        if e.empty() {
            return None;
        }
        if !e.matches(k, self.mask) {
            self.stats.foreign(e.word1, k[1]);
            return None;
        }
        self.stats.hits += 1;
        Some(e.value(self.mask))
    }

    #[inline(always)]
    fn store(&mut self, k: Key, value: u64) {
        assert!(value <= self.mask, "tt: value exceeds value_bits");
        self.stats.stores += 1;
        let mask = self.mask;
        let e = &mut self.entries[(k[0] & mask) as usize];
        if !e.empty() && !e.matches(k, mask) {
            self.stats.replaced += 1;
        }
        *e = Entry::new(k, mask, value);
    }

    fn max_value(&self) -> u64 {
        self.mask
    }

    fn clear(&mut self) {
        self.entries.fill(Entry::default());
    }

    fn slots(&self) -> usize {
        self.entries.len()
    }

    fn used(&self) -> usize {
        self.entries.iter().filter(|e| !e.empty()).count()
    }

    fn stats(&self) -> &Stats {
        &self.stats
    }

    fn reset_stats(&mut self) {
        self.stats = Stats::default();
    }
}
