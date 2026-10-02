//! The transposition table of the "store" kind: nothing is ever replaced or lost.
//! This is what a mate search needs for proven results and what a breadth-first
//! search needs to recognise positions it has already expanded. Module `tt` is
//! the lossy counterpart. Direct port of `mattjesGo/ttstore`.
//!
//! Same entry layout as `tt` (16 bytes: key word 0 with the low index bits
//! replaced by the value, key word 1 complete), same fixed power-of-two size
//! chosen by the caller. Open addressing with linear probing; the table never
//! grows, `put` reports failure once the fill limit is reached. Because a probed
//! entry may sit a few slots behind its home slot, the index bits are compared
//! only implicitly (104 stored bits plus a short probe distance); a false match
//! therefore has a probability of 2^-104 per comparison and is ignored, as is
//! the all-zero key that marks an empty slot.

mod persist;

/// The 128-bit position key (same type as `bitboard::Key`).
pub type Key = [u64; 2];

/// The size of one slot.
pub const ENTRY_BYTES: usize = std::mem::size_of::<Key>();

/// The fill level at which `put` starts to fail. Linear probing needs about 8
/// comparisons per miss at 75 % with random keys, 32 at 87 %.
pub const MAX_LOAD_PERCENT: usize = 75;

/// Maps keys to values of `value_bits` width.
pub struct Store {
    entries: Vec<Key>, // [0] = key word 0 with the low index bits holding the value, [1] = key word 1
    mask: u64,
    count: usize,
    limit: usize,
}

impl Store {
    /// Allocates the largest power-of-two store that fits into `size_mb`.
    pub fn new(size_mb: usize) -> Store {
        let mut n = 1;
        while n * 2 * ENTRY_BYTES <= size_mb << 20 {
            n *= 2;
        }
        Store::with_slots(n)
    }

    /// Allocates a store with exactly `slots` slots (a power of two).
    pub fn with_slots(slots: usize) -> Store {
        assert!(slots > 0 && slots.is_power_of_two(), "ttstore: slot count must be a power of two");
        Store { entries: vec![[0, 0]; slots], mask: slots as u64 - 1, count: 0, limit: slots * MAX_LOAD_PERCENT / 100 }
    }

    /// The width of a stored value (= log2 of the slot count).
    pub fn value_bits(&self) -> u32 {
        self.mask.count_ones()
    }

    /// The largest storable value.
    pub fn max_value(&self) -> u64 {
        self.mask
    }

    /// The number of keys the store accepts (`MAX_LOAD_PERCENT` of the slots).
    pub fn capacity(&self) -> usize {
        self.limit
    }

    /// The slot of k, or the empty slot where k would be inserted.
    #[inline(always)]
    fn find(&self, k: Key) -> (usize, bool) {
        let mut i = (k[0] & self.mask) as usize;
        loop {
            let e = self.entries[i];
            if e[1] == k[1] && (e[0] ^ k[0]) & !self.mask == 0 {
                return (i, true);
            }
            if e[0] | e[1] == 0 {
                return (i, false);
            }
            i = (i + 1) & self.mask as usize;
        }
    }

    pub fn get(&self, k: Key) -> Option<u64> {
        let (i, found) = self.find(k);
        if found {
            Some(self.entries[i][0] & self.mask)
        } else {
            None
        }
    }

    /// Stores value for k (replacing an existing value). Returns (is_new, ok):
    /// is_new reports whether k was not present before, ok is false if the store
    /// is full and k was not stored.
    pub fn put(&mut self, k: Key, value: u64) -> (bool, bool) {
        assert!(value <= self.mask, "ttstore: value exceeds value_bits");
        let (i, found) = self.find(k);
        if found {
            self.entries[i][0] = (k[0] & !self.mask) | value;
            return (false, true);
        }
        if self.count >= self.limit {
            return (true, false);
        }
        self.entries[i] = [(k[0] & !self.mask) | value, k[1]];
        self.count += 1;
        (true, true)
    }

    /// Adds k with value 0 (set semantics); see `put` for the results.
    pub fn insert(&mut self, k: Key) -> (bool, bool) {
        self.put(k, 0)
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn slots(&self) -> usize {
        self.entries.len()
    }

    pub fn bytes(&self) -> usize {
        self.entries.len() * ENTRY_BYTES
    }
}
