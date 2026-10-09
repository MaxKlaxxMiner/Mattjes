// Package tt holds transposition tables of the "cache" kind: a fixed amount of
// memory, and an entry is lost whenever its slot is needed for another position.
// This is what every alpha/beta engine uses.
//
// Entry layout (16 bytes, the author's design): the slot index is taken from the
// low b bits of the first key word (b = log2(slots), 24 for 16 Mi slots). Those
// bits are implied by the slot position and need not be stored, so the entry
// keeps the remaining 64-b bits of word 0, the complete word 1 and uses the freed
// b bits for the value. The full 128-bit identity is preserved, a false hit is
// impossible; the price is that the value width equals the index width.
//
// Two layouts share the entry format and the statistics: Table (direct mapped,
// one entry per slot, always replace) and Buckets (four entries per 64-byte
// cache line, the smallest value is replaced). Package ttstore is the opposite
// kind (fixed size too, but nothing is ever replaced).
package tt

import "math/bits"

// Key is the 128-bit position key. bitboard.Key is assignable to it.
type Key = [2]uint64

// entry: word0 = key word 0 with its low index bits replaced by the value,
// word1 = key word 1. Both words 0 marks an empty slot (a stored entry that is
// all zero has probability 2^-104 per position and is ignored).
type entry struct{ word0, word1 uint64 }

// EntryBytes is the size of one entry.
const EntryBytes = 16

// Stats counts what happened since the last ResetStats. Foreign counts entries
// of other positions a probe looked at; NearMiss[i] counts how often the low
// 16 / 32 / 48 bits of their second key word matched anyway, i.e. how many false
// hits a table that stored only that many check bits would have produced.
type Stats struct {
	Probes, Hits, Stores, Replaced uint64
	Foreign                        uint64
	NearMiss                       [3]uint64
}

func (s *Stats) foreign(check, want uint64) {
	s.Foreign++
	x := check ^ want
	if x&0xFFFF != 0 {
		return
	}
	s.NearMiss[0]++
	if x&0xFFFFFFFF != 0 {
		return
	}
	s.NearMiss[1]++
	if x&0xFFFFFFFFFFFF == 0 {
		s.NearMiss[2]++
	}
}

// Table is the direct-mapped layout: one entry per slot, always replace.
type Table struct {
	entries []entry
	mask    uint64 // index bits = value bits
	Stats
}

// New allocates the largest power-of-two table that fits into sizeMB.
func New(sizeMB int) *Table {
	return newTable(slotsFor(sizeMB, EntryBytes))
}

func newTable(slots int) *Table {
	return &Table{entries: make([]entry, slots), mask: uint64(slots - 1)}
}

// slotsFor returns the largest power of two n with n*slotBytes <= sizeMB MB.
func slotsFor(sizeMB int, slotBytes int) int {
	n := 1
	for n*2*slotBytes <= sizeMB<<20 {
		n *= 2
	}
	return n
}

// matches reports whether e holds key k whose index bits are implied by the slot.
func (e *entry) matches(k Key, mask uint64) bool {
	return e.word1 == k[1] && (e.word0^k[0])&^mask == 0
}

func (e *entry) empty() bool { return e.word0|e.word1 == 0 }

// ValueBits is the width of a stored value (= log2 of the slot count).
func (t *Table) ValueBits() int { return bits.OnesCount64(t.mask) }

// MaxValue is the largest storable value.
func (t *Table) MaxValue() uint64 { return t.mask }

// Probe returns the value stored for k.
func (t *Table) Probe(k Key) (uint64, bool) {
	t.Probes++
	e := &t.entries[k[0]&t.mask]
	if e.empty() {
		return 0, false
	}
	if !e.matches(k, t.mask) {
		t.foreign(e.word1, k[1])
		return 0, false
	}
	t.Hits++
	return e.word0 & t.mask, true
}

// Store writes the value for k, replacing whatever occupied the slot.
// The value must not exceed MaxValue.
func (t *Table) Store(k Key, value uint64) {
	if value > t.mask {
		panic("tt: value exceeds ValueBits")
	}
	t.Stores++
	e := &t.entries[k[0]&t.mask]
	if !e.empty() && !e.matches(k, t.mask) {
		t.Replaced++
	}
	*e = entry{k[0]&^t.mask | value, k[1]}
}

// Clear empties all slots but keeps the statistics.
func (t *Table) Clear() { clear(t.entries) }

// Prefetch touches the slot of a key, so that a Probe of several keys in a
// row overlaps the cache misses instead of waiting for them one by one. Go
// has no prefetch instruction; a load whose result feeds a global keeps the
// compiler from dropping it and lets the CPU run on while it is in flight.
func (t *Table) Prefetch(k Key) { prefetchSink += t.entries[k[0]&t.mask].word0 }

var prefetchSink uint64

func (t *Table) ResetStats()      { t.Stats = Stats{} }
func (t *Table) Counters() *Stats { return &t.Stats }
func (t *Table) Slots() int       { return len(t.entries) }
func (t *Table) Bytes() int       { return len(t.entries) * EntryBytes }

// Used counts the occupied slots (full scan).
func (t *Table) Used() int {
	n := 0
	for i := range t.entries {
		if !t.entries[i].empty() {
			n++
		}
	}
	return n
}
