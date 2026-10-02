//! Transposition tables of the "cache" kind: a fixed amount of memory, and an
//! entry is lost whenever its slot is needed for another position. This is what
//! every alpha/beta engine uses. The key is the 128-bit Zobrist key of `bitboard`:
//! the first word selects the slot, the second word is stored in full as the check
//! word, so a false hit needs a 64-bit collision inside one slot (effective key
//! length 64 + log2(slots) bits).
//!
//! Two layouts share the entry format and the statistics: `Table` (direct mapped,
//! one entry per slot, always replace) and `Buckets` (four entries per 64-byte
//! cache line, depth-preferred replacement). Module `ttstore` is the opposite kind
//! (exact keys, growing, nothing is ever lost). Direct port of `mattjesGo/tt`.

mod buckets;
mod persist;
mod table;

#[allow(unused_imports)]
pub use buckets::{Buckets, BUCKET_ENTRIES};
#[allow(unused_imports)]
pub use table::{Stats, Table, TransTable, ENTRY_BYTES};

/// The 128-bit position key (same type as `bitboard::Key`).
pub type Key = [u64; 2];
