// Package syzygy reads Syzygy WDL tablebases (files *.rtbw) for the search
// oracle: win, draw or loss for the side to move, no distance. The format
// and the probing follow Fathom (tbprobe.c, MIT licence, Ronald de Man,
// basil00, Jon Dart), ported to the bitboard package: tables are found by
// name in a directory, mapped into memory on first use and decoded per
// probe (index by the piece placement, Huffman-coded run lengths per
// block). DTZ files are ignored; the fifty-move rule is the caller's
// business (cursed wins and blessed losses are reported as such).
package syzygy

import (
	"encoding/binary"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"sync/atomic"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// wdlMagic opens every *.rtbw file.
const wdlMagic = 0x5d23e871

// MaxPieces is the largest table the reader handles (Syzygy goes to 7).
const MaxPieces = 7

// kindLetters are the piece kinds in bitboard order (K Q R B N P), the
// order of the letters in a table name.
const kindLetters = "KQRBNP"

// baseEntry is one table (one material, both colour assignments).
type baseEntry struct {
	name      string // "KQvKRR", the file name without suffix
	path      string
	num       uint8 // pieces on the board
	symmetric bool  // the same material on both sides
	hasPawns  bool
	kkEnc     bool     // pawnless and the kings are the only single pieces: 462 king-pair index
	pawns     [2]uint8 // pawn counts, the leading colour first
	ready     atomic.Bool
	failed    atomic.Bool
	data      []byte
	closer    func()
	ei        []encInfo // 2 (pieces: one per side to move) or 8 (pawns: 4 files, then the other side)
}

// lookup points a material key to its table and tells whether the
// board's colours are the table's (flip = the board's white is the
// table's black).
type lookup struct {
	be   *baseEntry
	flip bool
}

// Tables is the set of WDL tables found in a directory.
type Tables struct {
	dir     string
	entries map[uint64]lookup
	list    []*baseEntry
	mu      sync.Mutex // serialises the lazy initialisation of a table
	Probes  atomic.Uint64
	Hits    atomic.Uint64
}

// materialKey packs the piece counts per colour and kind (white first).
func materialKey(white, black [6]uint8) uint64 {
	var k uint64
	for kind := 0; kind < 6; kind++ {
		k = k<<4 | uint64(white[kind])
	}
	for kind := 0; kind < 6; kind++ {
		k = k<<4 | uint64(black[kind])
	}
	return k
}

// parseName reads "KQvKRR" into the piece counts of the first and the
// second side (false when the name is not a table name).
func parseName(name string) (first, second [6]uint8, ok bool) {
	parts := strings.Split(name, "v")
	if len(parts) != 2 {
		return first, second, false
	}
	for side, part := range parts {
		for _, ch := range part {
			kind := strings.IndexRune(kindLetters, ch)
			if kind < 0 {
				return first, second, false
			}
			if side == 0 {
				first[kind]++
			} else {
				second[kind]++
			}
		}
	}
	return first, second, first[0] == 1 && second[0] == 1
}

// Open scans a directory for *.rtbw files and registers them; the files
// are mapped on first use. Files with an incomplete size are skipped.
func Open(dir string) (*Tables, error) {
	names, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}
	t := &Tables{dir: dir, entries: map[uint64]lookup{}}
	for _, de := range names {
		fn := de.Name()
		if !strings.HasSuffix(fn, ".rtbw") || de.IsDir() {
			continue
		}
		name := strings.TrimSuffix(fn, ".rtbw")
		first, second, ok := parseName(name)
		if !ok {
			continue
		}
		info, err := de.Info()
		if err != nil || info.Size()&63 != 16 {
			continue // incomplete file
		}
		be := newEntry(name, first, second)
		if be == nil {
			continue
		}
		be.path = filepath.Join(dir, fn)
		t.register(be, first, second)
	}
	sort.Slice(t.list, func(a, b int) bool { return t.list[a].name < t.list[b].name })
	return t, nil
}

// newEntry describes the table of a material given as piece counts (the
// first side first); nil beyond MaxPieces.
func newEntry(name string, first, second [6]uint8) *baseEntry {
	be := &baseEntry{name: name}
	for kind := 0; kind < 6; kind++ {
		be.num += first[kind] + second[kind]
	}
	if be.num > MaxPieces {
		return nil
	}
	be.hasPawns = first[5]+second[5] > 0
	be.symmetric = materialKey(first, second) == materialKey(second, first)
	if !be.hasPawns {
		singles := 0
		for kind := 0; kind < 6; kind++ {
			if first[kind] == 1 {
				singles++
			}
			if second[kind] == 1 {
				singles++
			}
		}
		be.kkEnc = singles == 2
	} else {
		be.pawns = [2]uint8{first[5], second[5]}
		if second[5] != 0 && (first[5] == 0 || first[5] > second[5]) {
			be.pawns[0], be.pawns[1] = be.pawns[1], be.pawns[0]
		}
	}
	return be
}

// register files a table under both colour assignments.
func (t *Tables) register(be *baseEntry, first, second [6]uint8) {
	key, key2 := materialKey(first, second), materialKey(second, first)
	t.entries[key] = lookup{be, false}
	if key != key2 {
		t.entries[key2] = lookup{be, true}
	}
	t.list = append(t.list, be)
}

// Count is the number of tables found.
func (t *Tables) Count() int { return len(t.list) }

// Largest is the piece count of the largest table found.
func (t *Tables) Largest() int {
	n := 0
	for _, be := range t.list {
		n = max(n, int(be.num))
	}
	return n
}

// Close unmaps every mapped table.
func (t *Tables) Close() {
	t.mu.Lock()
	defer t.mu.Unlock()
	for _, be := range t.list {
		if be.closer != nil {
			be.closer()
			be.closer, be.data = nil, nil
			be.ready.Store(false)
		}
	}
}

// find returns the table of a material (false when there is none) and
// whether the colours are flipped; the key is built by the caller.
func (t *Tables) find(key uint64) (lookup, bool) {
	l, ok := t.entries[key]
	return l, ok
}

// prepare maps and parses the table on first use (double-checked under
// the mutex); false when the file is unusable.
func (t *Tables) prepare(be *baseEntry) bool {
	if be.ready.Load() {
		return true
	}
	if be.failed.Load() {
		return false
	}
	t.mu.Lock()
	defer t.mu.Unlock()
	if be.ready.Load() {
		return true
	}
	if be.failed.Load() {
		return false
	}
	if err := be.init(); err != nil {
		be.failed.Store(true)
		return false
	}
	be.ready.Store(true)
	return true
}

// init maps the file and reads its header: the piece orders and factors
// of every encoding, the pairs data and the block tables. The layout is
// Fathom's init_table for the WDL type.
func (be *baseEntry) init() error {
	data, closer, err := mapFile(be.path)
	if err != nil {
		return err
	}
	if len(data) < 5 || binary.LittleEndian.Uint32(data) != wdlMagic {
		closer()
		return fmt.Errorf("syzygy: %s is not a WDL table", be.path)
	}
	be.data, be.closer = data, closer
	split := data[4]&1 != 0
	pos := 5
	num := 1
	enc := encPiece
	if be.hasPawns {
		num = 4
		enc = encFile
	}
	be.ei = make([]encInfo, 2*num)
	var tbSize [4][2]uint64
	for t := 0; t < num; t++ {
		tbSize[t][0] = initEncInfo(&be.ei[t], be, data[pos:], 0, t, enc)
		if split {
			tbSize[t][1] = initEncInfo(&be.ei[num+t], be, data[pos:], 4, t, enc)
		}
		pos += int(be.num) + 1
		if be.hasPawns && be.pawns[1] != 0 {
			pos++
		}
	}
	pos += pos & 1
	var sizes [4][2][3]int
	for t := 0; t < num; t++ {
		be.ei[t].precomp, pos, sizes[t][0] = setupPairs(data, pos, tbSize[t][0])
		if split {
			be.ei[num+t].precomp, pos, sizes[t][1] = setupPairs(data, pos, tbSize[t][1])
		}
	}
	for t := 0; t < num; t++ {
		be.ei[t].precomp.indexTable = data[pos : pos+sizes[t][0][0]]
		pos += sizes[t][0][0]
		if split {
			be.ei[num+t].precomp.indexTable = data[pos : pos+sizes[t][1][0]]
			pos += sizes[t][1][0]
		}
	}
	for t := 0; t < num; t++ {
		be.ei[t].precomp.sizeTable = data[pos : pos+sizes[t][0][1]]
		pos += sizes[t][0][1]
		if split {
			be.ei[num+t].precomp.sizeTable = data[pos : pos+sizes[t][1][1]]
			pos += sizes[t][1][1]
		}
	}
	for t := 0; t < num; t++ {
		// the mapping is page aligned, so file offsets align like addresses;
		// the slices run to the end of the file because the decoder reads a
		// few bytes past a block (the file has spare bytes at its end)
		pos = (pos + 0x3f) &^ 0x3f
		be.ei[t].precomp.data = data[pos:]
		pos += sizes[t][0][2]
		if split {
			pos = (pos + 0x3f) &^ 0x3f
			be.ei[num+t].precomp.data = data[pos:]
			pos += sizes[t][1][2]
		}
	}
	if !split {
		// one encoding serves both sides to move
		for t := 0; t < num; t++ {
			be.ei[num+t] = be.ei[t]
		}
	}
	return nil
}

// Describe reports the layout of a table's compressed streams (one per
// encoding and side to move), for comparing our own packer against it; ok
// is false when there is no such table ("KBNvK").
func (t *Tables) Describe(name string) (lines []string, ok bool) {
	for _, be := range t.list {
		if be.name != name {
			continue
		}
		if !t.prepare(be) {
			return nil, false
		}
		num := 1
		if be.hasPawns {
			num = 4
		}
		for i, ei := range be.ei {
			if i >= num && be.ei[i-num].precomp == ei.precomp {
				continue // not split: one stream serves both sides to move
			}
			d := ei.precomp
			if d.idxBits == 0 {
				lines = append(lines, fmt.Sprintf("%s stream %d: constant value %d (%d positions)", name, i, d.constValue[0], d.size))
				continue
			}
			lines = append(lines, fmt.Sprintf("%s stream %d: %d positions, %d symbols, %d blocks of %d bytes (code %d), index 2^%d (%d bytes), sizes %d bytes, codes %d..%d bits",
				name, i, d.size, len(d.symLen), d.realBlocks, 1<<d.blockSize, d.realBlocks<<d.blockSize, d.idxBits, len(d.indexTable), len(d.sizeTable), d.minLen, d.minLen+uint8(len(d.base))-1))
			lines = append(lines, fmt.Sprintf("    pieces %v (Fathom codes: 1 pawn .. 6 king, 8 = black), norm %v, factors %v (1 = fastest)", ei.pieces[:be.num], ei.norm[:be.num], ei.factor[:be.num]))
		}
		return lines, true
	}
	return nil, false
}

// Streams decodes every compressed stream of a table completely (one per
// encoding and side to move, Syzygy's five values 0..4 per position, in
// the table's own index order) with the byte size each stream takes in the
// file (symbol table, index, size table and code blocks); nil when there is
// no such table. A yardstick for our own packer on identical data.
func (t *Tables) Streams(name string) (values [][]byte, bytes []int) {
	for _, be := range t.list {
		if be.name != name || !t.prepare(be) {
			continue
		}
		num := 1
		if be.hasPawns {
			num = 4
		}
		for i, ei := range be.ei {
			if i >= num && be.ei[i-num].precomp == ei.precomp {
				continue
			}
			d := ei.precomp
			v := make([]byte, d.size)
			if d.idxBits == 0 {
				for j := range v {
					v[j] = d.constValue[0]
				}
				values, bytes = append(values, v), append(bytes, 2)
				continue
			}
			for j := range v {
				v[j] = d.decompress(uint64(j))[0]
			}
			values = append(values, v)
			bytes = append(bytes, 3*len(d.symLen)+len(d.indexTable)+len(d.sizeTable)+d.realBlocks<<d.blockSize)
		}
		return values, bytes
	}
	return nil, nil
}

// Name of the table for a material given as piece counts (white first,
// kinds in bitboard order), for messages.
func Name(white, black [6]uint8) string {
	var sb strings.Builder
	for kind := 0; kind < 6; kind++ {
		for i := 0; i < int(white[kind]); i++ {
			sb.WriteByte(kindLetters[kind])
		}
	}
	sb.WriteByte('v')
	for kind := 0; kind < 6; kind++ {
		for i := 0; i < int(black[kind]); i++ {
			sb.WriteByte(kindLetters[kind])
		}
	}
	return sb.String()
}

var _ = chess.None
