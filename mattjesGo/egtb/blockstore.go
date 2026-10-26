package egtb

import (
	"fmt"
	"os"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/lz"
)

// CacheBytes is the budget of decoded blocks a packed table keeps in memory
// (docs/m5-endgame-tables.md, "Blockformat"). Below the budget the table is
// decoded lazily, block by block on first touch, and nothing is ever
// evicted: a five-piece table (up to 242 MB) costs only the blocks a search
// touches (a quarter of them for one KRR-KN solve). Above the budget (a
// six-piece table, 15 GB) blocks are evicted direct-mapped, and a miss costs
// a block read and decode (about 18 µs for 16 KB); df-pn touches a table
// scattered, so a budget well below the working set slows a search down
// (KRRKN with 32 MB: 2.8 times).
var CacheBytes = 2 << 30

// blockStore serves the values of a table that stays compressed in its cache
// file: a direct-mapped cache of decoded blocks (block number modulo slots,
// as many slots as the budget allows, at most one per block), a miss reads
// and decodes one compressed block. Used by the search's lookups only, from
// one thread at a time (the generator works on full arrays, a generation job
// forks the set without packed tables).
type blockStore struct {
	file     *os.File
	index    *lz.Index
	slots    [][]byte // decoded blocks, allocated on first use
	tags     []int32  // block number held in each slot, -1 = empty
	scratch  []byte   // one compressed block
	hits     uint64
	misses   uint64
	seen     []uint64 // bitmap of the blocks ever decoded: the working set of a search
	distinct uint64
	err      error // the first read error (the table then answers Draw, reported once)
	// wideEntries is the size of a wide table (0 = a one-byte table): the
	// file holds the low plane, then the high plane (Value.ToPlanes).
	wideEntries int
}

func newBlockStore(f *os.File, ix *lz.Index, wideEntries int) *blockStore {
	slots := max(1, CacheBytes/ix.BlockSize)
	if slots > ix.Blocks() {
		slots = ix.Blocks()
	}
	s := &blockStore{file: f, index: ix, slots: make([][]byte, slots), tags: make([]int32, slots), scratch: make([]byte, ix.MaxPacked()), seen: make([]uint64, (ix.Blocks()+63)/64), wideEntries: wideEntries}
	for i := range s.tags {
		s.tags[i] = -1
	}
	return s
}

// value returns entry idx: one byte, or the two planes of a wide table.
func (s *blockStore) value(idx int) Value {
	if s.wideEntries == 0 {
		return ValueOfByte(s.byteAt(idx))
	}
	lo := s.byteAt(idx)
	return ValueOfPlanes(lo, s.byteAt(s.wideEntries+idx))
}

// byteAt returns the byte at raw position idx, decoding its block on a miss
// (a read error gives 0).
func (s *blockStore) byteAt(idx int) byte {
	block := idx / s.index.BlockSize
	slot := block % len(s.slots)
	if s.tags[slot] != int32(block) {
		s.misses++
		if s.seen[block>>6]&(1<<(block&63)) == 0 {
			s.seen[block>>6] |= 1 << (block & 63)
			s.distinct++
		}
		if s.slots[slot] == nil {
			s.slots[slot] = make([]byte, s.index.BlockSize)
		}
		if err := s.index.ReadBlock(s.file, block, s.scratch, s.slots[slot]); err != nil {
			if s.err == nil {
				s.err = fmt.Errorf("egtb: %s: block %d: %v", s.file.Name(), block, err)
			}
			s.tags[slot] = -1
			return 0
		}
		s.tags[slot] = int32(block)
	} else {
		s.hits++
	}
	return s.slots[slot][idx-block*s.index.BlockSize]
}

func (s *blockStore) close() {
	if s.file != nil {
		s.file.Close()
		s.file = nil
	}
}

// CacheStats sums the block cache hits and misses of every packed table,
// and the blocks ever decoded (the working set) out of all their blocks.
func (s *Set) CacheStats() (hits, misses, distinct, blocks uint64) {
	for _, t := range s.Tables {
		if t.packed != nil {
			hits += t.packed.hits
			misses += t.packed.misses
			distinct += t.packed.distinct
			blocks += uint64(t.packed.index.Blocks())
		}
	}
	return hits, misses, distinct, blocks
}

// ResetCacheStats zeroes the counters of CacheStats (the working set too).
func (s *Set) ResetCacheStats() {
	for _, t := range s.Tables {
		if t.packed != nil {
			t.packed.hits, t.packed.misses, t.packed.distinct = 0, 0, 0
			clear(t.packed.seen)
		}
	}
}

// CacheError returns the first block read error of any packed table, if one
// happened (such a table answers Draw for the affected blocks).
func (s *Set) CacheError() error {
	for _, t := range s.Tables {
		if t.packed != nil && t.packed.err != nil {
			return t.packed.err
		}
	}
	return nil
}
