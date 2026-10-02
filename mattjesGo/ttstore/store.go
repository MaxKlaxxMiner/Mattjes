// Package ttstore is the transposition table of the "store" kind: nothing is
// ever replaced or lost. This is what a mate search needs for proven results
// and what a breadth-first search needs to recognise positions it has already
// expanded. Package tt is the lossy counterpart.
//
// Same entry layout as tt (16 bytes: key word 0 with the low index bits replaced
// by the value, key word 1 complete), same fixed power-of-two size chosen by the
// caller. Open addressing with linear probing; the table never grows, Put
// reports failure once the fill limit is reached. Because a probed entry may sit
// a few slots behind its home slot, the index bits are compared only implicitly
// (104 stored bits plus a short probe distance); a false match therefore has a
// probability of 2^-104 per comparison and is ignored, as is the all-zero key
// that marks an empty slot.
package ttstore

import "math/bits"

// Key is the 128-bit position key. bitboard.Key is assignable to it.
type Key = [2]uint64

// EntryBytes is the size of one slot.
const EntryBytes = 16

// MaxLoadPercent is the fill level at which Put starts to fail. Linear probing
// needs about 8 comparisons per miss at 75 % with random keys, 32 at 87 %.
const MaxLoadPercent = 75

// Store maps keys to values of ValueBits width.
type Store struct {
	entries []Key // [0] = key word 0 with the low index bits holding the value, [1] = key word 1
	mask    uint64
	count   int
	limit   int
}

// New allocates the largest power-of-two store that fits into sizeMB.
func New(sizeMB int) *Store {
	n := 1
	for n*2*EntryBytes <= sizeMB<<20 {
		n *= 2
	}
	return NewSlots(n)
}

// NewSlots allocates a store with exactly slots slots (a power of two).
func NewSlots(slots int) *Store {
	if slots <= 0 || slots&(slots-1) != 0 {
		panic("ttstore: slot count must be a power of two")
	}
	return &Store{entries: make([]Key, slots), mask: uint64(slots - 1), limit: slots * MaxLoadPercent / 100}
}

// ValueBits is the width of a stored value (= log2 of the slot count).
func (s *Store) ValueBits() int { return bits.OnesCount64(s.mask) }

// MaxValue is the largest storable value.
func (s *Store) MaxValue() uint64 { return s.mask }

// Capacity is the number of keys the store accepts (MaxLoadPercent of the slots).
func (s *Store) Capacity() int { return s.limit }

// find returns the slot of k, or the empty slot where k would be inserted.
func (s *Store) find(k Key) (uint64, bool) {
	i := k[0] & s.mask
	for {
		e := &s.entries[i]
		if e[1] == k[1] && (e[0]^k[0])&^s.mask == 0 {
			return i, true
		}
		if e[0]|e[1] == 0 {
			return i, false
		}
		i = (i + 1) & s.mask
	}
}

// Get returns the value stored for k.
func (s *Store) Get(k Key) (uint64, bool) {
	i, found := s.find(k)
	if !found {
		return 0, false
	}
	return s.entries[i][0] & s.mask, true
}

// Put stores value for k (replacing an existing value). isNew reports whether k
// was not present before; ok is false if the store is full and k was not stored.
func (s *Store) Put(k Key, value uint64) (isNew, ok bool) {
	if value > s.mask {
		panic("ttstore: value exceeds ValueBits")
	}
	i, found := s.find(k)
	if found {
		s.entries[i][0] = k[0]&^s.mask | value
		return false, true
	}
	if s.count >= s.limit {
		return true, false
	}
	s.entries[i] = Key{k[0]&^s.mask | value, k[1]}
	s.count++
	return true, true
}

// Insert adds k with value 0 (set semantics); see Put for the results.
func (s *Store) Insert(k Key) (isNew, ok bool) { return s.Put(k, 0) }

func (s *Store) Len() int   { return s.count }
func (s *Store) Slots() int { return len(s.entries) }
func (s *Store) Bytes() int { return len(s.entries) * EntryBytes }
