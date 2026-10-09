package tt

import (
	"math/bits"
	"unsafe"
)

// BucketEntries entries of 16 bytes form one 64-byte bucket = one cache line, so a
// probe that scans the whole bucket costs the same memory access as a single entry.
const BucketEntries = 4

type bucket [BucketEntries]entry

const bucketBytes = BucketEntries * EntryBytes

// Buckets is the set-associative layout: the key selects a bucket, the position
// may sit in any of its entries. The index (and value) width is log2 of the
// bucket count, two bits less than a direct-mapped table of the same size.
// Replacement evicts the entry with the smallest value (perft: the smallest
// subtree, which is the cheapest to recompute); callers that need another
// priority put it into the high value bits. Entries of a bucket are always
// packed from the front, nothing is ever deleted.
type Buckets struct {
	raw     []entry  // backing memory, over-allocated by one bucket for alignment
	buckets []bucket // 64-byte aligned view into raw
	mask    uint64
	Stats
}

// NewBuckets allocates the largest power-of-two number of buckets that fits into sizeMB.
func NewBuckets(sizeMB int) *Buckets {
	return newBuckets(slotsFor(sizeMB, bucketBytes))
}

func newBuckets(n int) *Buckets {
	raw := make([]entry, n*BucketEntries+BucketEntries)
	off := 0
	for uintptr(unsafe.Pointer(&raw[off]))%bucketBytes != 0 {
		off++
	}
	return &Buckets{
		raw:     raw,
		buckets: unsafe.Slice((*bucket)(unsafe.Pointer(&raw[off])), n),
		mask:    uint64(n - 1),
	}
}

func (t *Buckets) ValueBits() int   { return bits.OnesCount64(t.mask) }
func (t *Buckets) MaxValue() uint64 { return t.mask }

// Probe returns the value stored for k.
func (t *Buckets) Probe(k Key) (uint64, bool) {
	t.Probes++
	b := &t.buckets[k[0]&t.mask]
	for i := range b {
		e := &b[i]
		if e.empty() {
			break
		}
		if e.matches(k, t.mask) {
			t.Hits++
			return e.word0 & t.mask, true
		}
		t.foreign(e.word1, k[1])
	}
	return 0, false
}

// Store writes the value for k: into the entry that already holds k, else into
// the first empty entry, else over the entry with the smallest value.
func (t *Buckets) Store(k Key, value uint64) {
	if value > t.mask {
		panic("tt: value exceeds ValueBits")
	}
	t.Stores++
	b := &t.buckets[k[0]&t.mask]
	victim := &b[0]
	for i := range b {
		e := &b[i]
		if e.empty() || e.matches(k, t.mask) {
			victim = e
			break
		}
		if e.word0&t.mask < victim.word0&t.mask {
			victim = e
		}
	}
	if !victim.empty() && !victim.matches(k, t.mask) {
		t.Replaced++
	}
	*victim = entry{k[0]&^t.mask | value, k[1]}
}

// Clear empties all buckets but keeps the statistics.
func (t *Buckets) Clear() { clear(t.buckets) }

// Prefetch touches the bucket of a key (see Table.Prefetch).
func (t *Buckets) Prefetch(k Key) { prefetchSink += t.buckets[k[0]&t.mask][0].word0 }

func (t *Buckets) ResetStats()      { t.Stats = Stats{} }
func (t *Buckets) Counters() *Stats { return &t.Stats }
func (t *Buckets) Slots() int       { return len(t.buckets) * BucketEntries }
func (t *Buckets) Bytes() int       { return len(t.buckets) * bucketBytes }

// Used counts the occupied entries (full scan).
func (t *Buckets) Used() int {
	n := 0
	for i := range t.buckets {
		for j := range t.buckets[i] {
			if !t.buckets[i][j].empty() {
				n++
			}
		}
	}
	return n
}
