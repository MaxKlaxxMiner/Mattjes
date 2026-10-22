// Package pairs compresses a stream of small values the way the Syzygy tables
// do (the scheme of Ronald de Man's tbgen, our own container): the stream is
// rewritten by recursive pair replacement (every new symbol stands for two
// older ones, "Re-Pair"), the resulting symbols get a canonical Huffman code,
// and the code is cut into small fixed-size blocks with a position index, so
// that one value costs the decoding of one block of a few dozen bytes. The
// table stays packed in memory (or mapped from the file) and is read value by
// value (Table.Value), there is no unpacked copy.
//
// The packer is free to choose any tokenisation and any dictionary: the
// decoder only needs the symbol table and the code lengths. What makes the
// endgame tables small is upstream of this package: the index order and the
// don't-care positions (docs/m5-endgame-tables.md, "Eigene WDL-Tabellen").
//
// Container (little endian): a 32-byte header, u16 symbol counts per code
// length (minLen..maxLen), 3 bytes per symbol (two 12-bit symbol numbers, or
// the value and 0xfff for a leaf; symbols are numbered by code length, the
// uncoded ones last), the index (6 bytes per 2^idxBits positions: block u32,
// offset u16), the size table (u16 per block: positions minus one), the
// blocks, 8 zero bytes.
package pairs

import (
	"encoding/binary"
	"errors"
	"fmt"
	"os"
	"sort"
	"sync"
	"sync/atomic"
	"time"
)

// Options tune the packer; zero values take the defaults.
type Options struct {
	// BlockBytes is the size of a code block (a power of two, 16..256).
	// A lookup decodes from the start of its block, so smaller blocks are
	// faster to probe and cost more padding and size-table entries. 0 =
	// 32 and 64 are both tried and the smaller result kept.
	BlockBytes int
	// IndexBits: one index entry (6 bytes) per 1<<IndexBits positions; a
	// lookup walks the size table from the entry's block to the block of
	// its position. 0 = chosen so that an entry covers about 16 blocks.
	IndexBits int
	// MaxSymbols caps the alphabet plus the pair symbols (at most 4095).
	MaxSymbols int
	// MaxSymbolLen caps the positions a symbol expands to.
	MaxSymbolLen int
	// MinCount: a pair is replaced only when it occurs at least this often
	// (a symbol costs its dictionary entry and lengthens the codes).
	MinCount int
	// PairsPerPass: pairs replaced in one pass over the stream (pairwise
	// disjoint in their symbols, so the counts stay exact); 1 is textbook
	// Re-Pair, more is faster.
	PairsPerPass int
	Workers      int
}

func (o Options) withDefaults() Options {
	if o.MaxSymbols <= 0 {
		o.MaxSymbols = 4095
	}
	if o.MaxSymbolLen <= 0 {
		o.MaxSymbolLen = 256
	}
	if o.MinCount <= 0 {
		o.MinCount = 8
	}
	if o.PairsPerPass <= 0 {
		o.PairsPerPass = 64
	}
	if o.Workers <= 0 {
		o.Workers = 1
	}
	o.MaxSymbols = min(o.MaxSymbols, leafMark-1)
	o.MaxSymbolLen = min(o.MaxSymbolLen, positionsPerBlock-1)
	return o
}

const (
	magic       = "MPR2"
	headerBytes = 32
	// MaxCodeLen is the longest Huffman code (the decoder refills its 64-bit
	// register 32 bits at a time).
	MaxCodeLen = 32
	// positionsPerBlock is the most positions a block may hold (its size
	// table entry is a u16 holding the count minus one).
	positionsPerBlock = 1 << 16
	tailBytes         = 8     // zero padding after the last block: the decoder reads ahead
	leafMark          = 0xfff // the right half of a leaf's symbol entry
)

// Stats describes a packed stream.
type Stats struct {
	Positions  int
	Symbols    int // alphabet plus pair symbols
	Tokens     int // symbols in the stream after the replacements
	Blocks     int
	BlockBytes int
	IndexBytes int
	SizeBytes  int
	CodeBytes  int
	Bytes      int // the whole container
	MinCodeLen int
	MaxCodeLen int
	IndexBits  int
	Passes     int
}

func (s Stats) String() string {
	return fmt.Sprintf("%d bytes: %d positions in %d tokens, %d symbols, %d blocks of %d, index %d (2^%d), sizes %d, code %d, codes %d..%d bits, %d passes",
		s.Bytes, s.Positions, s.Tokens, s.Symbols, s.Blocks, s.BlockBytes, s.IndexBytes, s.IndexBits, s.SizeBytes, s.CodeBytes, s.MinCodeLen, s.MaxCodeLen, s.Passes)
}

// --- packing ---

type encoder struct {
	alphabet int
	syms     []uint16    // the stream, shrinking with every pass
	pat      [][2]uint16 // per symbol: the two it stands for (unused below alphabet)
	symLen   []int       // per symbol: positions it expands to
	passes   int
}

// Pack compresses values (each below alphabet, at most 256) and returns the
// container.
func Pack(values []byte, alphabet int, opt Options) []byte {
	out, _ := PackStats(values, alphabet, opt)
	return out
}

// PackStats is Pack with the statistics of the result.
func PackStats(values []byte, alphabet int, opt Options) ([]byte, Stats) {
	opt = opt.withDefaults()
	if alphabet <= 0 || alphabet > 256 {
		panic("pairs: alphabet must be 1..256")
	}
	e := &encoder{alphabet: alphabet, syms: make([]uint16, len(values))}
	for i, v := range values {
		if int(v) >= alphabet {
			panic(fmt.Sprintf("pairs: value %d at %d beyond the alphabet %d", v, i, alphabet))
		}
		e.syms[i] = uint16(v)
	}
	e.pat = make([][2]uint16, alphabet, opt.MaxSymbols)
	e.symLen = make([]int, alphabet, opt.MaxSymbols)
	for s := range e.symLen {
		e.symLen[s] = 1
	}
	var tCount, tChoose, tReplace time.Duration
	for len(e.symLen) < opt.MaxSymbols {
		t0 := time.Now()
		counts := e.countPairs(opt.Workers)
		t1 := time.Now()
		chosen := e.choose(counts, opt)
		t2 := time.Now()
		if len(chosen) == 0 {
			break
		}
		e.replace(chosen, opt.Workers)
		t3 := time.Now()
		tCount += t1.Sub(t0)
		tChoose += t2.Sub(t1)
		tReplace += t3.Sub(t2)
		e.passes++
	}
	if os.Getenv("MATTJES_PAIRS_PROFILE") != "" {
		fmt.Printf("pairs: %d passes, count %v, choose %v, replace %v\n", e.passes, tCount, tChoose, tReplace)
	}
	c := e.code()
	if opt.BlockBytes > 0 {
		return c.container(opt.BlockBytes, opt.IndexBits)
	}
	out32, st32 := c.container(32, opt.IndexBits)
	out64, st64 := c.container(64, opt.IndexBits)
	if len(out64) < len(out32) {
		return out64, st64
	}
	return out32, st32
}

// pairCounts is an open-addressing hash from a pair (left<<16 | right) to its
// count; a count of 0 marks an empty slot.
type pairCounts struct {
	keys   []uint32
	counts []uint32
	used   int
	bits   uint
}

func newPairCounts(bits uint) *pairCounts {
	return &pairCounts{keys: make([]uint32, 1<<bits), counts: make([]uint32, 1<<bits), bits: bits}
}

func (c *pairCounts) add(key, n uint32) {
	if c.used*10 > len(c.keys)*6 {
		c.grow()
	}
	mask := uint32(len(c.keys) - 1)
	h := (key * 0x9E3779B1) >> (32 - c.bits)
	for {
		if c.counts[h] == 0 {
			c.keys[h], c.counts[h] = key, n
			c.used++
			return
		}
		if c.keys[h] == key {
			c.counts[h] += n
			return
		}
		h = (h + 1) & mask
	}
}

func (c *pairCounts) grow() {
	old := *c
	c.bits++
	c.keys = make([]uint32, 1<<c.bits)
	c.counts = make([]uint32, 1<<c.bits)
	c.used = 0
	for i, n := range old.counts {
		if n != 0 {
			c.add(old.keys[i], n)
		}
	}
}

// chunkSyms is the fixed chunk of the replacement pass: the result must not
// depend on the number of workers (Go and Rust write identical files on any
// machine), so the chunk borders are fixed and the workers take chunks from
// a counter.
const chunkSyms = 1 << 16

// chunks runs fn over the fixed chunks of the stream on the workers.
func (e *encoder) chunks(workers int, fn func(w, lo, hi int)) {
	n := len(e.syms)
	numChunks := (n + chunkSyms - 1) / chunkSyms
	workers = max(1, min(workers, numChunks))
	var next atomic.Int64
	var wg sync.WaitGroup
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func(w int) {
			defer wg.Done()
			for {
				c := int(next.Add(1) - 1)
				if c >= numChunks {
					return
				}
				fn(w, c*chunkSyms, min(n, (c+1)*chunkSyms))
			}
		}(w)
	}
	wg.Wait()
}

// countPairs counts every adjacent pair of the stream. Every worker scans
// the whole stream and counts the pairs whose left symbol falls into its
// share, so the tables hold disjoint keys and nothing has to be merged
// (merging per-chunk tables cost more than the counting: the distinct pairs
// of every chunk are nearly those of the whole stream).
func (e *encoder) countPairs(workers int) []*pairCounts {
	n := len(e.syms)
	workers = max(1, min(workers, n/(1<<16)+1))
	tables := make([]*pairCounts, workers)
	var wg sync.WaitGroup
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func(w int) {
			defer wg.Done()
			t := newPairCounts(12)
			s := e.syms
			for i := 0; i+1 < n; i++ {
				if int(s[i])%workers == w {
					t.add(uint32(s[i])<<16|uint32(s[i+1]), 1)
				}
			}
			tables[w] = t
		}(w)
	}
	wg.Wait()
	return tables
}

type pairChoice struct {
	left, right uint16
	count       uint32
}

// choose picks the most frequent pairs for one pass: at least MinCount
// occurrences, within MaxSymbolLen, pairwise disjoint in their symbols so
// that one replacement cannot touch another's count.
func (e *encoder) choose(tables []*pairCounts, opt Options) []pairChoice {
	var cands []pairChoice
	for _, counts := range tables {
		for i, n := range counts.counts {
			if n < uint32(opt.MinCount) {
				continue
			}
			k := counts.keys[i]
			l, r := uint16(k>>16), uint16(k)
			if e.symLen[l]+e.symLen[r] > opt.MaxSymbolLen {
				continue
			}
			cands = append(cands, pairChoice{l, r, n})
		}
	}
	sort.Slice(cands, func(a, b int) bool {
		if cands[a].count != cands[b].count {
			return cands[a].count > cands[b].count
		}
		if cands[a].left != cands[b].left {
			return cands[a].left < cands[b].left
		}
		return cands[a].right < cands[b].right
	})
	used := make([]bool, len(e.symLen))
	var chosen []pairChoice
	room := opt.MaxSymbols - len(e.symLen)
	for _, c := range cands {
		if len(chosen) >= opt.PairsPerPass || len(chosen) >= room {
			break
		}
		if used[c.left] || used[c.right] {
			continue
		}
		used[c.left], used[c.right] = true, true
		chosen = append(chosen, c)
	}
	return chosen
}

// replace rewrites every occurrence of the chosen pairs by a new symbol, in
// parallel chunks compacted in place (a pair across a chunk border stays).
func (e *encoder) replace(chosen []pairChoice, workers int) {
	right := make([]int32, len(e.symLen))
	newSym := make([]uint16, len(e.symLen))
	for i := range right {
		right[i] = -1
	}
	for _, c := range chosen {
		s := uint16(len(e.symLen))
		e.pat = append(e.pat, [2]uint16{c.left, c.right})
		e.symLen = append(e.symLen, e.symLen[c.left]+e.symLen[c.right])
		right[c.left] = int32(c.right)
		newSym[c.left] = s
	}
	n := len(e.syms)
	numChunks := (n + chunkSyms - 1) / chunkSyms
	ends := make([]int, numChunks)
	e.chunks(workers, func(_, lo, hi int) {
		s := e.syms
		out := lo
		for i := lo; i < hi; {
			c := s[i]
			if r := right[c]; r >= 0 && i+1 < hi && s[i+1] == uint16(r) {
				s[out] = newSym[c]
				i += 2
			} else {
				s[out] = c
				i++
			}
			out++
		}
		ends[lo/chunkSyms] = out
	})
	// stitch the compacted chunks together
	out := 0
	for c := 0; c < numChunks; c++ {
		out += copy(e.syms[out:], e.syms[c*chunkSyms:ends[c]])
	}
	e.syms = e.syms[:out]
}

// coded is the stream after the replacements with its Huffman code, the
// symbols renumbered by code length (uncoded symbols last).
type coded struct {
	tokens  []uint16    // the stream in new symbol numbers
	table   []byte      // 3 bytes per symbol in new numbers
	symLen  []int       // per new symbol
	lengths []uint8     // code length per new symbol (0 = uncoded)
	codes   []uint32    // canonical code per new symbol
	count   []int       // symbols per code length
	minLen  int
	maxLen  int
	passes  int
	symbols int
}

// code assigns the Huffman code and renumbers the symbols.
func (e *encoder) code() *coded {
	numSyms := len(e.symLen)
	freq := make([]uint64, numSyms)
	for _, s := range e.syms {
		freq[s]++
	}
	oldLengths := huffmanLengths(freq, MaxCodeLen)
	order := make([]int, numSyms)
	for i := range order {
		order[i] = i
	}
	sort.SliceStable(order, func(a, b int) bool {
		la, lb := oldLengths[order[a]], oldLengths[order[b]]
		if la == 0 {
			la = 255 // uncoded symbols go last
		}
		if lb == 0 {
			lb = 255
		}
		return la < lb
	})
	newID := make([]uint16, numSyms)
	for i, s := range order {
		newID[s] = uint16(i)
	}
	c := &coded{passes: e.passes, symbols: numSyms}
	c.lengths = make([]uint8, numSyms)
	c.symLen = make([]int, numSyms)
	c.table = make([]byte, 3*numSyms)
	for i, s := range order {
		c.lengths[i] = oldLengths[s]
		c.symLen[i] = e.symLen[s]
		var l, r uint32
		if s < e.alphabet {
			l, r = uint32(s), leafMark
		} else {
			l, r = uint32(newID[e.pat[s][0]]), uint32(newID[e.pat[s][1]])
		}
		w := l | r<<12
		c.table[3*i], c.table[3*i+1], c.table[3*i+2] = byte(w), byte(w>>8), byte(w>>16)
	}
	c.codes, c.count = canonicalCodes(c.lengths)
	c.minLen, c.maxLen = MaxCodeLen, 0
	for _, l := range c.lengths {
		if l > 0 {
			c.minLen, c.maxLen = min(c.minLen, int(l)), max(c.maxLen, int(l))
		}
	}
	if c.maxLen == 0 {
		c.minLen = 0
	}
	c.tokens = make([]uint16, len(e.syms))
	for i, s := range e.syms {
		c.tokens[i] = newID[s]
	}
	return c
}

// container cuts the code into blocks of blockBytes and writes the
// container.
func (c *coded) container(blockBytes int, idxBits int) ([]byte, Stats) {
	blockBits := 0
	for 1<<blockBits < blockBytes {
		blockBits++
	}
	blockBytes = 1 << blockBits
	var code []byte
	var sizes []uint16 // positions per block minus one
	var starts []int   // first position of every block
	block := make([]byte, 0, blockBytes)
	var acc uint64      // pending bits, MSB first
	accBits := 0        // valid bits in acc
	blockBitsUsed := 0  // bits written to the current block, acc included
	blockPositions := 0 // positions covered by the current block
	position := 0
	flushBlock := func() {
		for accBits > 0 {
			block = append(block, byte(acc>>56))
			acc <<= 8
			accBits -= 8
		}
		accBits, acc = 0, 0
		for len(block) < blockBytes {
			block = append(block, 0)
		}
		code = append(code, block...)
		sizes = append(sizes, uint16(blockPositions-1))
		starts = append(starts, position-blockPositions)
		block = block[:0]
		blockBitsUsed, blockPositions = 0, 0
	}
	for _, s := range c.tokens {
		l := int(c.lengths[s])
		n := c.symLen[s]
		if blockPositions > 0 && (blockBitsUsed+l > blockBytes*8 || blockPositions+n > positionsPerBlock) {
			flushBlock()
		}
		acc |= uint64(c.codes[s]) << (64 - accBits - l)
		accBits += l
		for accBits >= 8 {
			block = append(block, byte(acc>>56))
			acc <<= 8
			accBits -= 8
		}
		blockBitsUsed += l
		blockPositions += n
		position += n
	}
	if blockPositions > 0 {
		flushBlock()
	}
	numBlocks := len(sizes)

	// the index: one entry per 1<<idxBits positions, about one per 16 blocks
	if idxBits <= 0 {
		idxBits = 1
		for (position>>idxBits)*16 > numBlocks && idxBits < 40 {
			idxBits++
		}
	}
	numIndex := (position + (1 << idxBits) - 1) >> idxBits
	index := make([]byte, 6*numIndex)
	b := 0
	for k := 0; k < numIndex; k++ {
		p := k << idxBits
		for b+1 < numBlocks && starts[b+1] <= p {
			b++
		}
		binary.LittleEndian.PutUint32(index[6*k:], uint32(b))
		binary.LittleEndian.PutUint16(index[6*k+4:], uint16(p-starts[b]))
	}

	// the container
	h := 0
	if c.maxLen > 0 {
		h = c.maxLen - c.minLen + 1
	}
	out := make([]byte, headerBytes, headerBytes+2*h+len(c.table)+len(index)+2*numBlocks+len(code)+tailBytes)
	copy(out, magic)
	out[5] = byte(blockBits)
	out[6] = byte(idxBits)
	out[7] = byte(c.maxLen)
	binary.LittleEndian.PutUint64(out[8:], uint64(position))
	binary.LittleEndian.PutUint32(out[16:], uint32(c.symbols))
	binary.LittleEndian.PutUint32(out[20:], uint32(numBlocks))
	out[24] = byte(c.minLen)
	if c.maxLen > 0 {
		for l := c.minLen; l <= c.maxLen; l++ {
			out = binary.LittleEndian.AppendUint16(out, uint16(c.count[l]))
		}
	}
	out = append(out, c.table...)
	out = append(out, index...)
	for _, sz := range sizes {
		out = binary.LittleEndian.AppendUint16(out, sz)
	}
	out = append(out, code...)
	out = append(out, make([]byte, tailBytes)...)
	st := Stats{Positions: position, Symbols: c.symbols, Tokens: len(c.tokens), Blocks: numBlocks, BlockBytes: blockBytes, IndexBytes: len(index), SizeBytes: 2 * numBlocks, CodeBytes: len(code), Bytes: len(out), MinCodeLen: c.minLen, MaxCodeLen: c.maxLen, IndexBits: idxBits, Passes: c.passes}
	return out, st
}

// huffmanLengths computes code lengths for the frequencies (0 = unused
// symbol), limited to maxLen with zlib's overflow repair.
func huffmanLengths(freq []uint64, maxLen int) []uint8 {
	type node struct {
		weight uint64
		parent int32
	}
	var leaves []int
	for s, f := range freq {
		if f > 0 {
			leaves = append(leaves, s)
		}
	}
	lengths := make([]uint8, len(freq))
	switch len(leaves) {
	case 0:
		return lengths
	case 1:
		lengths[leaves[0]] = 1
		return lengths
	}
	sort.Slice(leaves, func(a, b int) bool {
		if freq[leaves[a]] != freq[leaves[b]] {
			return freq[leaves[a]] < freq[leaves[b]]
		}
		return leaves[a] < leaves[b]
	})
	// two-queue Huffman: leaves sorted ascending, internal nodes are created
	// in ascending weight order, so both queues stay sorted
	nodes := make([]node, 0, 2*len(leaves))
	for _, s := range leaves {
		nodes = append(nodes, node{freq[s], -1})
	}
	li, ii := 0, len(leaves) // next leaf, next internal node
	takeMin := func() int {
		if li < len(leaves) && (ii >= len(nodes) || nodes[li].weight <= nodes[ii].weight) {
			li++
			return li - 1
		}
		ii++
		return ii - 1
	}
	for k := 0; k < len(leaves)-1; k++ { // every merge creates one internal node, the last is the root
		a := takeMin()
		b := takeMin()
		nodes = append(nodes, node{nodes[a].weight + nodes[b].weight, -1})
		nodes[a].parent = int32(len(nodes) - 1)
		nodes[b].parent = int32(len(nodes) - 1)
	}
	depth := make([]int, len(nodes))
	for i := len(nodes) - 2; i >= 0; i-- {
		depth[i] = depth[nodes[i].parent] + 1
	}
	// zlib's repair: codes beyond maxLen are cut to maxLen, then the Kraft
	// sum is restored by lengthening codes of shorter lengths
	blCount := make([]int, maxLen+2)
	overflow := 0
	for i := range leaves {
		d := depth[i]
		if d > maxLen {
			overflow++
			d = maxLen
		}
		blCount[d]++
	}
	for overflow > 0 {
		bits := maxLen - 1
		for blCount[bits] == 0 {
			bits--
		}
		blCount[bits]--
		blCount[bits+1] += 2
		blCount[maxLen]--
		overflow -= 2
	}
	// assign the lengths: the rarest symbols get the longest codes
	i := 0
	for bits := maxLen; bits >= 1; bits-- {
		for n := blCount[bits]; n > 0; n-- {
			lengths[leaves[i]] = uint8(bits)
			i++
		}
	}
	if i != len(leaves) {
		panic("pairs: huffman length assignment is inconsistent")
	}
	kraft := uint64(0) // in units of 2^-maxLen: a complete prefix code sums to exactly one
	for _, l := range lengths {
		if l > 0 {
			kraft += 1 << (uint(maxLen) - uint(l))
		}
	}
	if kraft != 1<<uint(maxLen) {
		panic(fmt.Sprintf("pairs: huffman code is not complete (kraft %d of %d)", kraft, 1<<uint(maxLen)))
	}
	return lengths
}

// canonicalCodes assigns deflate-style canonical codes: codes of one length
// are consecutive in symbol order, longer codes continue numerically after
// the shorter ones (shifted). Returns the codes and the count per length.
func canonicalCodes(lengths []uint8) ([]uint32, []int) {
	count := make([]int, MaxCodeLen+2)
	for _, l := range lengths {
		count[l]++
	}
	count[0] = 0
	next := make([]uint32, MaxCodeLen+2)
	code := uint32(0)
	for bits := 1; bits <= MaxCodeLen; bits++ {
		code = (code + uint32(count[bits-1])) << 1
		next[bits] = code
	}
	codes := make([]uint32, len(lengths))
	for s, l := range lengths {
		if l > 0 {
			codes[s] = next[l]
			next[l]++
		}
	}
	return codes, count
}

// --- reading ---

// Table reads values from a packed container without unpacking it.
type Table struct {
	blockBits uint
	idxBits   uint
	minLen    int
	positions int
	bytes     int
	table     []byte   // 3 bytes per symbol: two 12-bit symbols, or the value and leafMark
	symLen    []uint16 // per symbol: positions it expands to
	index     []byte   // 6 bytes per entry: block u32, offset u16
	sizes     []byte   // u16 per block: positions minus one
	code      []byte   // the blocks, followed by the tail padding
	// canonical decoding: for every length l, last[l] is the largest 64-bit
	// window that starts with a code of length at most l (the codes of one
	// length are consecutive, longer codes continue numerically after them);
	// the symbols of length l are numbered from firstSym[l] and carry codes
	// from firstCode[l]
	last      [MaxCodeLen + 2]uint64
	firstCode [MaxCodeLen + 2]uint32
	firstSym  [MaxCodeLen + 2]uint32
	// fast: a 2^fastBits lookup from the top bits of the window to the symbol
	// (<<8) and its length, 0 when the code is longer than fastBits
	fast []uint32
}

const fastBits = 11

// Open parses a container (the slice is kept, nothing is copied).
func Open(data []byte) (*Table, error) {
	if len(data) < headerBytes || string(data[:4]) != magic {
		return nil, errors.New("pairs: not a pairs container")
	}
	t := &Table{blockBits: uint(data[5]), idxBits: uint(data[6])}
	maxLen := int(data[7])
	t.positions = int(binary.LittleEndian.Uint64(data[8:]))
	numSyms := int(binary.LittleEndian.Uint32(data[16:]))
	numBlocks := int(binary.LittleEndian.Uint32(data[20:]))
	t.minLen = int(data[24])
	numIndex := (t.positions + (1 << t.idxBits) - 1) >> t.idxBits
	h := 0
	if maxLen > 0 {
		h = maxLen - t.minLen + 1
	}
	pos := headerBytes
	need := pos + 2*h + 3*numSyms + 6*numIndex + 2*numBlocks + numBlocks<<t.blockBits + tailBytes
	if maxLen > MaxCodeLen || len(data) < need {
		return nil, fmt.Errorf("pairs: container truncated or corrupt (%d of %d bytes)", len(data), need)
	}
	t.bytes = need
	var count [MaxCodeLen + 2]int
	if maxLen > 0 {
		for l := t.minLen; l <= maxLen; l++ {
			count[l] = int(binary.LittleEndian.Uint16(data[pos:]))
			pos += 2
		}
	}
	t.table = data[pos : pos+3*numSyms]
	pos += 3 * numSyms
	t.index = data[pos : pos+6*numIndex]
	pos += len(t.index)
	t.sizes = data[pos : pos+2*numBlocks]
	pos += len(t.sizes)
	t.code = data[pos:]
	// the expansion lengths: symbols are numbered by code length, so a pair
	// may refer to higher numbers; a depth counter catches cycles
	t.symLen = make([]uint16, numSyms)
	var lengthOf func(s uint32, depth int) (uint16, error)
	lengthOf = func(s uint32, depth int) (uint16, error) {
		if int(s) >= numSyms {
			return 0, fmt.Errorf("pairs: symbol %d beyond the table", s)
		}
		if t.symLen[s] != 0 {
			return t.symLen[s], nil
		}
		if depth > numSyms {
			return 0, errors.New("pairs: the symbol table has a cycle")
		}
		l, r := t.pair(int(s))
		if r == leafMark {
			t.symLen[s] = 1
			return 1, nil
		}
		a, err := lengthOf(l, depth+1)
		if err != nil {
			return 0, err
		}
		b, err := lengthOf(r, depth+1)
		if err != nil {
			return 0, err
		}
		if int(a)+int(b) >= positionsPerBlock {
			return 0, fmt.Errorf("pairs: symbol %d is too long", s)
		}
		t.symLen[s] = a + b
		return a + b, nil
	}
	for s := 0; s < numSyms; s++ {
		if _, err := lengthOf(uint32(s), 0); err != nil {
			return nil, err
		}
	}
	// the canonical code: symbols are numbered by length
	code, sym := uint32(0), uint32(0)
	for l := 1; l <= MaxCodeLen; l++ {
		code = (code + uint32(count[l-1])) << 1
		t.firstCode[l] = code
		t.firstSym[l] = sym
		sym += uint32(count[l])
		switch end := uint64(code) + uint64(count[l]); {
		case count[l] == 0:
			// no code of this length: every window is longer (longer codes
			// continue numerically after the shorter ones)
			t.last[l] = 0
		case end >= 1<<uint(l):
			t.last[l] = ^uint64(0) // the codes of this length reach the top
		default:
			t.last[l] = end<<(64-uint(l)) - 1
		}
	}
	t.last[MaxCodeLen+1] = ^uint64(0)
	if sym > uint32(numSyms) {
		return nil, errors.New("pairs: more codes than symbols")
	}
	t.fast = make([]uint32, 1<<fastBits)
	if maxLen > 0 {
		for w := 0; w < 1<<fastBits; w++ {
			window := uint64(w) << (64 - fastBits)
			l := t.minLen
			for window > t.last[l] {
				l++
			}
			if l <= fastBits {
				t.fast[w] = t.symbolAt(window, l)<<8 | uint32(l)
			}
		}
	}
	return t, nil
}

func (t *Table) pair(s int) (uint32, uint32) {
	p := t.table[3*s:]
	w := uint32(p[0]) | uint32(p[1])<<8 | uint32(p[2])<<16
	return w & 0xfff, w >> 12
}

func (t *Table) symbolAt(window uint64, l int) uint32 {
	return t.firstSym[l] + uint32(window>>(64-uint(l))) - t.firstCode[l]
}

// Len is the number of positions.
func (t *Table) Len() int { return t.positions }

// Value decodes the value at a position.
func (t *Table) Value(idx int) byte {
	if idx < 0 || idx >= t.positions {
		panic(fmt.Sprintf("pairs: position %d beyond %d", idx, t.positions))
	}
	entry := t.index[6*(idx>>t.idxBits):]
	block := int(binary.LittleEndian.Uint32(entry))
	lit := idx&(1<<t.idxBits-1) + int(binary.LittleEndian.Uint16(entry[4:]))
	for {
		n := int(binary.LittleEndian.Uint16(t.sizes[2*block:])) + 1
		if lit < n {
			break
		}
		lit -= n
		block++
	}
	ptr := t.code[block<<t.blockBits:]
	window := binary.BigEndian.Uint64(ptr)
	ptr = ptr[8:]
	empty := 0 // consumed bits of the window not refilled yet
	var sym uint32
	for {
		var l int
		if f := t.fast[window>>(64-fastBits)]; f != 0 {
			sym, l = f>>8, int(f&0xff)
		} else {
			l = fastBits + 1
			for window > t.last[l] {
				l++
			}
			sym = t.symbolAt(window, l)
		}
		n := int(t.symLen[sym])
		if lit < n {
			break
		}
		lit -= n
		window <<= uint(l)
		empty += l
		if empty >= 32 {
			empty -= 32
			var next uint32
			if len(ptr) >= 4 {
				next = binary.BigEndian.Uint32(ptr)
				ptr = ptr[4:]
			}
			window |= uint64(next) << uint(empty)
		}
	}
	for {
		l, r := t.pair(int(sym))
		if r == leafMark {
			return byte(l)
		}
		if n := int(t.symLen[l]); lit < n {
			sym = l
		} else {
			lit -= n
			sym = r
		}
	}
}

// Bytes is the size of the container.
func (t *Table) Bytes() int { return t.bytes }
