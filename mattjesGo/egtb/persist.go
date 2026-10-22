package egtb

import (
	"bufio"
	"encoding/binary"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
	"unsafe"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/lz"
)

// The cache lives in a directory next to the binary. The base file holds all
// tables up to four pieces in the order of All(); every larger material gets
// its own file "<pieces>-<name>.bin" (never shipped, only generated locally).
//
// File format (v3, 2026-10-16): 4 bytes magic "MEG3", 1 byte step (the
// resolution of the values in moves, Table.Step: 1 = exact, n = buckets of n
// moves), 3 bytes zero, 8 bytes raw checksum (the constant in the code, over
// the exact values as generated, dead and illegal entries as 0), then the
// indexed lz container with the values (lz.PackIndexedTo: block offsets up
// front, so a table beyond the base is read block by block through a block
// cache instead of being unpacked as a whole; the base uses 4 MB blocks and
// is unpacked, the rest BlockBytes). Before compression every dead or illegal
// entry is replaced by its predecessor: those positions are never looked up,
// so their content is free and the runs get longer. Tables beyond the base
// are written quantised to FileStep (the search resolves the exact distance
// from the bounds, docs/m5-endgame-tables.md); the base is always exact.
const (
	CacheDir     = "mattjes-egtb-cache"
	BaseFileName = "4-all.bin"
	magic        = "MEG3"
	headerBytes  = 16
	// BlockBytes is the raw block size of the files beyond the base. Measured
	// on KRRKN (docs/m5-endgame-tables.md, "Blockformat"): 64 KB costs 5 %
	// file size over 4 MB blocks but a search touches 45 % of the table, 16 KB
	// costs 17 % and touches 27 %, 4 KB costs 40 % and touches 15 %; a miss
	// decodes one block, 16 KB in about 18 µs. 16 KB is the compromise.
	BlockBytes = 16 << 10
)

// FileStep is the resolution in moves of every table beyond the base in the
// cache files ("mate in at most 8 moves" instead of the exact count). It
// halves the files twice over against exact values; the search resolves the
// 16-ply window in about 0.3 s at the root (KRR-KN, KQ-KBN), step 4 would
// take 1 ms and save one halving less. The base stays exact (37 MB), and the
// generator needs exact dependencies, so a quantised cache file is never used
// as one (GenerateControlled regenerates it in memory instead).
const FileStep = 8

// FileChecksum is the checksum of the complete base (a hash of the 35 table
// checksums). It doubles as the regression test of the generator: a changed
// generator that still produces this value produces the same tables. 0 means
// "not yet recorded" (the first generation prints the value to put here).
const FileChecksum uint64 = 0x64fe872c7e217f6e // format v3 (2026-10-08): losses up to 254 plies, no invalid value

// TableChecksums are the raw checksums of the single tables (format v3:
// move count per byte, dead and illegal entries 0) and are compared after
// every generation and load. Larger materials are added as they are
// measured; the five-piece values of format v2 (2026-10-07) are in the git
// history and in docs/m5-endgame-tables.md.
var TableChecksums = map[string]uint64{
	"KQK": 0x9260104b203b89c4, "KRK": 0x5387c89faf9e0217, "KBK": 0x73f143eadfe811a8, "KNK": 0x73f143eadfe811a8, // KBK = KNK: all zero
	"KQQK": 0xea8550ab91d6bf04, "KQRK": 0x7e56203083caed07, "KQBK": 0x0daa2fa637ffbf47, "KQNK": 0xfa22d9cbcc5750d2,
	"KRRK": 0x9fa46d416e96e031, "KRBK": 0x0fcf7cb7a35f16b7, "KRNK": 0xf61bbd3f8e767a38, "KBBK": 0x14956d0d156a61b2,
	"KBNK": 0x3c55de9ecf98feb6, "KNNK": 0xf3fe22b4f30f1651,
	"KQKQ": 0xda1249305b769fb7, "KQKR": 0xb7d88c4c91896ef9, "KQKB": 0x2a2334b0f5d15047, "KQKN": 0x2e37a9157d56f895,
	"KRKR": 0xfb8ee6c6d503053c, "KRKB": 0xefa63d9931328cda, "KRKN": 0xc020cf7e4db95b94, "KBKB": 0x5fef424e80401f69,
	"KBKN": 0x5f096e24292f2568, "KNKN": 0x667f4a21cc8aaab2,
	"KPK":  0xb1d19b5318c5076c,
	"KQPK": 0x1c74eb840969f447, "KRPK": 0x05107c598fafd5ec, "KBPK": 0x7943932114500468, "KNPK": 0xa6464dc2cc6a01f8,
	"KQKP": 0x24b8b54772f35819, "KRKP": 0x57b1f53eaee0a89c, "KBKP": 0x74931eaafaa29dd8, "KNKP": 0x627ee5ef42b600f5,
	"KPPK": 0x829f48ba8cf92ce4, "KPKP": 0xae084110af4ab131,
	// five pieces (format v3, 2026-10-09, Go and Rust identical), added as they are measured
	"KBBBK": 0xa552a70f5be1c3db, "KBNKQ": 0xa03df227d9f5eb72,
	"KRRKN": 0x443a1b3b48954ec3, // 2026-10-16, UCI background job, Go and Rust files byte-identical
	// six pieces (format v3, Rust, home machine): KRRKBN 1294 s, 35 GiB committed
	"KRRKBN": 0x11049fde56c4b77c,
}

// Checksum is a 64-bit hash over 8-byte little-endian words (FNV-1a style
// with a final mix), fast enough for 170 MB and trivial to reproduce in Rust.
func Checksum(values []Value) uint64 {
	data := bytesOf(values)
	const prime = 0x100000001b3
	h := uint64(0xcbf29ce484222325)
	i := 0
	for ; i+8 <= len(data); i += 8 {
		w := binary.LittleEndian.Uint64(data[i : i+8])
		h = (h ^ w) * prime
		h ^= h >> 29
	}
	for ; i < len(data); i++ {
		h = (h ^ uint64(data[i])) * prime
	}
	h ^= h >> 32
	h *= 0x9e3779b97f4a7c15
	h ^= h >> 29
	return h
}

// bytesOf views the values as bytes without copying (Value is a byte).
func bytesOf(v []Value) []byte {
	if len(v) == 0 {
		return nil
	}
	return unsafe.Slice((*byte)(unsafe.Pointer(&v[0])), len(v))
}

// rawChecksum returns the checksum of the generated values, computing it when
// the table was never saved or loaded.
func (t *Table) rawChecksum() uint64 {
	if t.RawChecksum == 0 && !t.filled && t.Values != nil {
		t.RawChecksum = Checksum(t.Values)
	}
	return t.RawChecksum
}

// fill replaces dead and illegal entries by their predecessor ("don't care"),
// once, and drops the generator's bitset afterwards.
func (t *Table) fill() {
	if t.filled {
		return
	}
	t.rawChecksum()
	if t.invalid != nil {
		prev := Draw
		for i, v := range t.Values {
			if t.invalid.get(i) {
				t.Values[i] = prev
			} else {
				prev = v
			}
		}
		t.invalid = nil
	}
	t.filled = true
}

// Checksum of the base tables in file order (they must be complete): a hash
// of the per-table checksums keeps the tables independent.
func (s *Set) Checksum() uint64 {
	h := uint64(0x9e3779b97f4a7c15)
	for _, t := range s.Base() {
		h = (h ^ t.rawChecksum()) * 0x100000001b3
		h ^= h >> 31
	}
	return h
}

// BaseSize is the number of raw bytes of the base tables.
func (s *Set) BaseSize() int {
	n := 0
	for _, t := range s.Base() {
		n += t.Size
	}
	return n
}

// DefaultCacheDir is the cache directory next to the running binary.
func DefaultCacheDir() string {
	exe, err := os.Executable()
	if err != nil {
		return CacheDir
	}
	return filepath.Join(filepath.Dir(exe), CacheDir)
}

// DefaultPath is the base cache file next to the running binary.
func DefaultPath() string { return filepath.Join(DefaultCacheDir(), BaseFileName) }

// TablePath is the cache file of a material beyond the base, e.g. "5-KBNKQ.bin".
func (s *Set) TablePath(t *Table) string {
	return filepath.Join(s.CacheDir, fmt.Sprintf("%d-%s.bin", t.Mat.Pieces(), t.Mat.Name()))
}

// RawSuffix marks the exact twin of a cache file ("5-KBNKQ.bin.raw"): the
// same container, but step 1. A generation writes it for every dependency it
// had to compute, so that an interrupted job keeps them and a later
// generation can load them instead of computing them again (the quantised
// file cannot serve as a dependency). The user deletes them by hand.
const RawSuffix = ".raw"

// RawPath is the exact cache file of a material beyond the base.
func (s *Set) RawPath(t *Table) string { return s.TablePath(t) + RawSuffix }

// ParseFileName reads the material of a cache file name as TablePath writes
// it ("5-KRRKN.bin", or its exact twin "5-KRRKN.bin.raw"); ok is false for
// any other name, the base file included.
func ParseFileName(name string) (Material, bool) {
	name = strings.TrimSuffix(name, RawSuffix)
	base, found := strings.CutSuffix(name, ".bin")
	if !found {
		return Material{}, false
	}
	_, matName, found := strings.Cut(base, "-")
	if !found {
		return Material{}, false
	}
	m, err := Parse(matName)
	if err != nil || fmt.Sprintf("%d-%s.bin", m.Pieces(), m.Name()) != name {
		return Material{}, false
	}
	return m, true
}

// BasePath is the base cache file in the set's cache directory.
func (s *Set) BasePath() string { return filepath.Join(s.CacheDir, BaseFileName) }

// Fill replaces dead and illegal entries by their predecessor and drops the
// generator's bitset (what writing the table does; for tables kept in RAM only).
func (t *Table) Fill() { t.fill() }

// FormatBytes renders "35.1 GiB" or "346 MB".
func FormatBytes(b uint64) string {
	if b >= 1<<30 {
		return fmt.Sprintf("%.1f GiB", float64(b)/float64(1<<30))
	}
	return fmt.Sprintf("%d MB", b>>20)
}

// writeFile writes header + compressed values in blocks of blockSize. The
// tables are filled first; step is the resolution they all share (recorded
// in the header).
func writeFile(path string, tables []*Table, rawChecksum uint64, step, blockSize, workers int) error {
	for _, t := range tables {
		if t.Values == nil {
			return fmt.Errorf("egtb: table %s not generated", t.Mat.Name())
		}
		if t.Step != step {
			return fmt.Errorf("egtb: table %s has step %d, the file needs %d", t.Mat.Name(), t.Step, step)
		}
		t.fill()
	}
	// a single table (every material beyond the base) is packed in place: no
	// copy of 15 GB for a six-piece table; only the base file concatenates
	var raw []byte
	if len(tables) == 1 {
		raw = bytesOf(tables[0].Values)
	} else {
		raw = make([]byte, 0, sumSize(tables))
		for _, t := range tables {
			raw = append(raw, bytesOf(t.Values)...)
		}
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	w := bufio.NewWriterSize(f, 1<<20)
	var header [headerBytes]byte
	copy(header[:], magic)
	header[4] = byte(step)
	binary.LittleEndian.PutUint64(header[8:], rawChecksum)
	if _, err := w.Write(header[:]); err != nil {
		f.Close()
		return err
	}
	if err := lz.PackIndexedTo(w, raw, blockSize, workers); err != nil {
		f.Close()
		return err
	}
	if err := w.Flush(); err != nil {
		f.Close()
		return err
	}
	return f.Close()
}

func sumSize(tables []*Table) int {
	n := 0
	for _, t := range tables {
		n += t.Size
	}
	return n
}

// openFile opens a cache file and reads its header and block index; the
// caller either unpacks everything (readFile) or keeps the file for block
// reads (openTable). size is the raw length the tables expect.
func openFile(path string, size int) (f *os.File, ix *lz.Index, step int, rawChecksum uint64, err error) {
	f, err = os.Open(path)
	if err != nil {
		return nil, nil, 0, 0, err
	}
	fail := func(err error) (*os.File, *lz.Index, int, uint64, error) {
		f.Close()
		return nil, nil, 0, 0, err
	}
	var header [headerBytes]byte
	if n, err := f.ReadAt(header[:], 0); n < headerBytes || string(header[:4]) != magic {
		return fail(fmt.Errorf("egtb: %s is not a cache file (format %q, regenerate; %v)", path, string(header[:4]), err))
	}
	step = max(1, int(header[4]))
	rawChecksum = binary.LittleEndian.Uint64(header[8:])
	ix, err = lz.ReadIndex(f, headerBytes)
	if err != nil {
		return fail(fmt.Errorf("egtb: %s: %v", path, err))
	}
	if ix.RawLen != size {
		return fail(fmt.Errorf("egtb: %s holds %d bytes, expected %d", path, ix.RawLen, size))
	}
	return f, ix, step, rawChecksum, nil
}

// readFile reads a cache file into the tables as a whole (the base).
// Returns the raw checksum from the header; the entries are filled (dead and
// illegal ones hold their predecessor) and the tables carry the file's step.
func readFile(path string, tables []*Table, workers int) (rawChecksum uint64, compressed bool, err error) {
	clearTables(tables)
	f, ix, step, rawChecksum, err := openFile(path, sumSize(tables))
	if err != nil {
		return 0, false, err
	}
	defer f.Close()
	for _, t := range tables {
		t.Values = make([]Value, t.Size)
	}
	// decompress into one buffer, then split (the tables are separate slices)
	var dst []byte
	if len(tables) == 1 {
		dst = bytesOf(tables[0].Values)
	} else {
		dst = make([]byte, ix.RawLen)
	}
	if err := ix.UnpackAll(f, dst, workers); err != nil {
		return 0, true, fmt.Errorf("egtb: %s: %v", path, err)
	}
	if len(tables) > 1 {
		off := 0
		for _, t := range tables {
			copy(bytesOf(t.Values), dst[off:off+t.Size])
			off += t.Size
		}
	}
	for _, t := range tables {
		t.filled = true
		t.Step = step
	}
	return rawChecksum, true, nil
}

// openTable opens a cache file for block reads: the table stays compressed
// in the file, a block cache (CacheBytes) serves the lookups. Returns the
// raw checksum from the header.
func openTable(path string, t *Table) (rawChecksum uint64, err error) {
	clearTables([]*Table{t})
	f, ix, step, rawChecksum, err := openFile(path, t.Size)
	if err != nil {
		return 0, err
	}
	t.packed = newBlockStore(f, ix)
	t.filled = true
	t.Step = step
	return rawChecksum, nil
}

func clearTables(tables []*Table) {
	for _, t := range tables {
		if t.packed != nil {
			t.packed.close()
			t.packed = nil
		}
		t.Values = nil
		t.filled = false
		t.RawChecksum = 0
		t.invalid = nil
		t.Step = 1
	}
}

// IOWorkers is the number of threads used to compress and decompress cache
// files (independent 4 MB blocks).
var IOWorkers = 8

// Save writes the base tables to path (compressed, always exact; 4 MB
// blocks, the base is always unpacked as a whole).
func (s *Set) Save(path string) error {
	return writeFile(path, s.Base(), s.Checksum(), 1, lz.BlockSize, IOWorkers)
}

// Load reads the base tables from path and checks size and checksum.
func (s *Set) Load(path string) error {
	sum, _, err := readFile(path, s.Base(), IOWorkers)
	if err != nil {
		clearTables(s.Base())
		return err
	}
	if !s.Base()[0].Exact() {
		clearTables(s.Base())
		return fmt.Errorf("egtb: %s is quantised (step %d), the base must be exact", path, s.Base()[0].Step)
	}
	// the header carries the combined checksum of the generator run; the
	// single table checksums cannot be recomputed from filled values, so the
	// recorded constants stand in for them
	for _, t := range s.Base() {
		t.RawChecksum = TableChecksums[t.Mat.Name()]
	}
	if FileChecksum != 0 && sum != FileChecksum {
		clearTables(s.Base())
		return fmt.Errorf("egtb: %s has checksum %016x, expected %016x", path, sum, FileChecksum)
	}
	return nil
}

// SaveTable writes one table (a material beyond the base) to path
// (compressed). The table is quantised to FileStep **in place** first (no
// copy of 15 GB for a six-piece table): afterwards it is what a reload gives,
// the search resolves its bounds, and it can no longer serve the generator as
// a dependency. The header keeps the checksum of the exact values. Verify
// before saving.
func (s *Set) SaveTable(t *Table, path string) error {
	if t.Values == nil {
		return fmt.Errorf("egtb: table %s not generated", t.Mat.Name())
	}
	sum := t.rawChecksum() // of the exact values, before quantising
	if t.Exact() {
		t.Quantize(FileStep)
	}
	return writeFile(path, []*Table{t}, sum, t.Step, BlockBytes, IOWorkers)
}

// SaveRawTable writes one exact table (a material beyond the base) to path
// unquantised (RawPath): the values stay exact in memory, the file can serve
// a later generation as a dependency. The table must still be exact.
func (s *Set) SaveRawTable(t *Table, path string) error {
	if t.Values == nil {
		return fmt.Errorf("egtb: table %s not generated", t.Mat.Name())
	}
	if !t.Exact() {
		return fmt.Errorf("egtb: table %s is quantised (step %d), no exact file", t.Mat.Name(), t.Step)
	}
	return writeFile(path, []*Table{t}, t.rawChecksum(), 1, BlockBytes, IOWorkers)
}

// LoadRawTable reads an exact table (RawPath) completely into memory, as a
// generation needs its dependencies. Checks the step, the size and, when
// recorded, the checksum.
func (s *Set) LoadRawTable(t *Table, path string) error {
	sum, _, err := readFile(path, []*Table{t}, IOWorkers)
	if err != nil {
		clearTables([]*Table{t})
		return err
	}
	if !t.Exact() {
		step := t.Step
		clearTables([]*Table{t})
		return fmt.Errorf("egtb: %s is quantised (step %d), expected an exact table", path, step)
	}
	t.RawChecksum = sum
	if want, known := TableChecksums[t.Mat.Name()]; known && sum != want {
		clearTables([]*Table{t})
		return fmt.Errorf("egtb: %s has checksum %016x, expected %016x", path, sum, want)
	}
	return nil
}

// SaveBoth writes the exact file (RawPath) and the quantised file (TablePath)
// of a table that is exact in memory, each only when missing, and leaves the
// table exact: SaveTable quantises in place, so the exact file is loaded
// back afterwards (0.4 s for a five-piece table). For a background job's
// dependencies, which later tables of the job still need exactly. progress
// gets a line per written file (nil = silent). Returns the first error.
func (s *Set) SaveBoth(t *Table, progress Progress) error {
	if progress == nil {
		progress = func(string) {}
	}
	raw, quantised := s.RawPath(t), s.TablePath(t)
	if !t.Exact() {
		return fmt.Errorf("egtb: table %s is quantised (step %d), no exact file", t.Mat.Name(), t.Step)
	}
	if _, err := os.Stat(raw); err != nil {
		if err := s.SaveRawTable(t, raw); err != nil {
			return err
		}
		progress("egtb: written exact to " + raw)
	}
	if _, err := os.Stat(quantised); err != nil {
		if err := s.SaveTable(t, quantised); err != nil {
			return err
		}
		progress("egtb: written to " + quantised)
		if err := s.LoadRawTable(t, raw); err != nil {
			return err
		}
	}
	return nil
}

// LoadTable opens one table (a material beyond the base) from path: it
// stays compressed in the file and is read block by block (openTable).
// Checks size and, when recorded, the checksum.
func (s *Set) LoadTable(t *Table, path string) error {
	sum, err := openTable(path, t)
	if err != nil {
		clearTables([]*Table{t})
		return err
	}
	t.RawChecksum = sum
	if want, known := TableChecksums[t.Mat.Name()]; known && sum != want {
		clearTables([]*Table{t})
		return fmt.Errorf("egtb: %s has checksum %016x, expected %016x", path, sum, want)
	}
	return nil
}

// LoadOrGenerate returns the complete base set: from the cache file when it
// is present and correct, otherwise freshly generated, checked against the
// constants and, when write is set, written. progress gets a line per step
// and two per generated table (nil = silent). The set's cache directory is
// the directory of path.
func LoadOrGenerate(path string, workers int, write bool, progress Progress) *Set {
	if progress == nil {
		progress = func(string) {}
	}
	s := NewSet()
	s.CacheDir = filepath.Dir(path)
	start := time.Now()
	if err := s.Load(path); err == nil {
		progress(fmt.Sprintf("egtb: loaded %d MB from %s in %.2f s", s.BaseSize()>>20, path, time.Since(start).Seconds()))
		return s
	} else if !errors.Is(err, os.ErrNotExist) {
		progress("egtb: " + err.Error())
	}
	done := 0
	s.GenerateAll(workers, func(line string) {
		if strings.Contains(line, " wins ") { // one summary line per table, no levels
			done++
			progress(fmt.Sprintf("(%d / %d) %s", done, len(s.Base()), line))
		}
	})
	progress(fmt.Sprintf("egtb: generated %d tables in %.1f s", len(s.Base()), time.Since(start).Seconds()))
	s.CheckChecksums(progress)
	if !write {
		for _, t := range s.Base() {
			t.fill()
		}
		return s
	}
	if err := s.Save(path); err != nil {
		progress("egtb: " + err.Error())
	} else {
		progress("egtb: written to " + path)
	}
	return s
}

// LoadOrGenerateTable returns the table of a material beyond the base: from
// its cache file when present and correct, otherwise generated (with its
// dependencies) and written. loaded reports which way it went; stats are
// only meaningful after a generation.
func (s *Set) LoadOrGenerateTable(m Material, workers int, progress Progress) (t *Table, st Stats, loaded bool) {
	if progress == nil {
		progress = func(string) {}
	}
	t = s.AddMaterial(m)
	if t.Values != nil {
		return t, Stats{}, true
	}
	path := s.TablePath(t)
	if err := s.LoadTable(t, path); err == nil {
		progress(fmt.Sprintf("egtb: loaded %s (%d MB)", path, t.Size>>20))
		return t, Stats{}, true
	} else if !errors.Is(err, os.ErrNotExist) {
		progress("egtb: " + err.Error())
	}
	st = s.Generate(t, workers, progress)
	if err := s.SaveTable(t, path); err != nil {
		progress("egtb: " + err.Error())
	} else {
		progress("egtb: written to " + path)
	}
	return t, st, false
}

// CheckChecksums compares every base table and the whole base with the
// recorded constants and reports differences (or the values to record, when
// missing). Returns the number of mismatches.
func (s *Set) CheckChecksums(progress Progress) int {
	bad := 0
	for _, t := range s.Base() {
		sum := t.rawChecksum()
		want, known := TableChecksums[t.Mat.Name()]
		switch {
		case !known:
			progress(fmt.Sprintf("  \"%s\": 0x%016x,", t.Mat.Name(), sum))
		case sum != want:
			bad++
			progress(fmt.Sprintf("egtb: %s checksum %016x, expected %016x", t.Mat.Name(), sum, want))
		}
	}
	sum := s.Checksum()
	switch {
	case FileChecksum == 0:
		progress(fmt.Sprintf("egtb: file checksum 0x%016x (record it as FileChecksum)", sum))
	case sum != FileChecksum:
		bad++
		progress(fmt.Sprintf("egtb: file checksum %016x, expected %016x", sum, FileChecksum))
	default:
		progress("egtb: checksums ok")
	}
	return bad
}
