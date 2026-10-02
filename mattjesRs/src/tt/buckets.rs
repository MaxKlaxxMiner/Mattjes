use super::table::{slots_for, Entry, Stats, TransTable, ENTRY_BYTES};
use super::Key;

/// Entries of 16 bytes forming one 64-byte bucket = one cache line, so a probe
/// that scans the whole bucket costs the same memory access as a single entry.
pub const BUCKET_ENTRIES: usize = 4;

/// `align(64)` makes the allocator place every bucket on its own cache line (the
/// Go version has to over-allocate and align by hand).
#[derive(Clone, Copy, Default)]
#[repr(C, align(64))]
pub(super) struct Bucket(pub [Entry; BUCKET_ENTRIES]);

pub(super) const BUCKET_BYTES: usize = std::mem::size_of::<Bucket>();
const _: () = assert!(BUCKET_BYTES == BUCKET_ENTRIES * ENTRY_BYTES);

/// The set-associative layout: the key selects a bucket, the position may sit in
/// any of its entries. The index (and value) width is log2 of the bucket count,
/// two bits less than a direct-mapped table of the same size. Replacement evicts
/// the entry with the smallest value (perft: the smallest subtree, which is the
/// cheapest to recompute); callers that need another priority put it into the
/// high value bits. Entries of a bucket are always packed from the front,
/// nothing is ever deleted.
pub struct Buckets {
    pub(super) buckets: Vec<Bucket>,
    mask: u64,
    pub stats: Stats,
}

impl Buckets {
    /// Allocates the largest power-of-two number of buckets that fits into `size_mb`.
    pub fn new(size_mb: usize) -> Buckets {
        Buckets::with_buckets(slots_for(size_mb, BUCKET_BYTES))
    }

    pub(super) fn with_buckets(n: usize) -> Buckets {
        Buckets { buckets: vec![Bucket::default(); n], mask: n as u64 - 1, stats: Stats::default() }
    }

    pub fn reset_stats(&mut self) {
        self.stats = Stats::default();
    }

    pub fn bytes(&self) -> usize {
        self.buckets.len() * BUCKET_BYTES
    }
}

impl TransTable for Buckets {
    #[inline(always)]
    fn probe(&mut self, k: Key) -> Option<u64> {
        self.stats.probes += 1;
        let mask = self.mask;
        let b = &self.buckets[(k[0] & mask) as usize].0;
        for e in b {
            if e.empty() {
                break;
            }
            if e.matches(k, mask) {
                self.stats.hits += 1;
                return Some(e.value(mask));
            }
            self.stats.foreign(e.word1, k[1]);
        }
        None
    }

    /// Writes into the entry that already holds k, else into the first empty
    /// entry, else over the entry with the smallest value.
    #[inline(always)]
    fn store(&mut self, k: Key, value: u64) {
        assert!(value <= self.mask, "tt: value exceeds value_bits");
        self.stats.stores += 1;
        let mask = self.mask;
        let b = &mut self.buckets[(k[0] & mask) as usize].0;
        let mut victim = 0;
        for (i, e) in b.iter().enumerate() {
            if e.empty() || e.matches(k, mask) {
                victim = i;
                break;
            }
            if e.value(mask) < b[victim].value(mask) {
                victim = i;
            }
        }
        let v = &mut b[victim];
        if !v.empty() && !v.matches(k, mask) {
            self.stats.replaced += 1;
        }
        *v = Entry::new(k, mask, value);
    }

    fn max_value(&self) -> u64 {
        self.mask
    }

    fn clear(&mut self) {
        self.buckets.fill(Bucket::default());
    }

    fn slots(&self) -> usize {
        self.buckets.len() * BUCKET_ENTRIES
    }

    fn used(&self) -> usize {
        self.buckets.iter().flat_map(|b| b.0.iter()).filter(|e| !e.empty()).count()
    }

    fn stats(&self) -> &Stats {
        &self.stats
    }
}
