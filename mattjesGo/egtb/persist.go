package egtb

import (
	"encoding/binary"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"time"
	"unsafe"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/lz"
)

// The cache lives in a directory next to the binary. The base file holds all
// tables up to four pieces in the order of All(); every larger material gets
// its own file "<pieces>-<name>.bin" (never shipped, only generated locally).
//
// File format: 4 bytes magic "MEGT", 8 bytes raw checksum (the constant in the
// code, over the values as generated, dead and illegal entries as 0), then the
// lz container with the values. Before compression every dead or illegal
// entry is replaced by its predecessor: those positions are never looked up,
// so their content is free and the runs get longer.
const (
	CacheDir     = "mattjes-egtb-cache"
	BaseFileName = "4-all.bin"
	magic        = "MEGT"
	headerBytes  = 12
)

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
	// five pieces: to be recorded from egtb-measure runs
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
	return filepath.Join(DefaultCacheDir(), fmt.Sprintf("%d-%s.bin", t.Mat.Pieces(), t.Mat.Name()))
}

// writeFile writes header + compressed values. The tables are filled first.
func writeFile(path string, tables []*Table, rawChecksum uint64, workers int) error {
	raw := make([]byte, 0, sumSize(tables))
	for _, t := range tables {
		if t.Values == nil {
			return fmt.Errorf("egtb: table %s not generated", t.Mat.Name())
		}
		t.fill()
		raw = append(raw, bytesOf(t.Values)...)
	}
	packed := lz.Pack(raw, workers)
	out := make([]byte, headerBytes, headerBytes+len(packed))
	copy(out, magic)
	binary.LittleEndian.PutUint64(out[4:], rawChecksum)
	out = append(out, packed...)
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	return os.WriteFile(path, out, 0o644)
}

func sumSize(tables []*Table) int {
	n := 0
	for _, t := range tables {
		n += t.Size
	}
	return n
}

// readFile reads a cache file into the tables. Returns the raw checksum from
// the header; the entries are filled (dead and illegal ones hold their
// predecessor).
func readFile(path string, tables []*Table, workers int) (rawChecksum uint64, compressed bool, err error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return 0, false, err
	}
	size := sumSize(tables)
	for _, t := range tables {
		t.Values = make([]Value, t.Size)
		t.filled = false
		t.RawChecksum = 0
		t.invalid = nil
	}
	if len(data) < headerBytes || string(data[:4]) != magic {
		return 0, false, fmt.Errorf("egtb: %s is not a cache file", path)
	}
	rawChecksum = binary.LittleEndian.Uint64(data[4:])
	packed := data[headerBytes:]
	if n, err := lz.RawLen(packed); err != nil || n != size {
		return 0, true, fmt.Errorf("egtb: %s holds %d bytes, expected %d", path, n, size)
	}
	// decompress into one buffer, then split (the tables are separate slices)
	var dst []byte
	if len(tables) == 1 {
		dst = bytesOf(tables[0].Values)
	} else {
		dst = make([]byte, size)
	}
	if err := lz.UnpackInto(dst, packed, workers); err != nil {
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
	}
	return rawChecksum, true, nil
}

func clearTables(tables []*Table) {
	for _, t := range tables {
		t.Values = nil
		t.filled = false
		t.RawChecksum = 0
		t.invalid = nil
	}
}

// IOWorkers is the number of threads used to compress and decompress cache
// files (independent 4 MB blocks).
var IOWorkers = 8

// Save writes the base tables to path (compressed).
func (s *Set) Save(path string) error {
	return writeFile(path, s.Base(), s.Checksum(), IOWorkers)
}

// Load reads the base tables from path and checks size and checksum.
func (s *Set) Load(path string) error {
	sum, _, err := readFile(path, s.Base(), IOWorkers)
	if err != nil {
		clearTables(s.Base())
		return err
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

// SaveTable writes one table (a material beyond the base) to path (compressed).
func (s *Set) SaveTable(t *Table, path string) error {
	if t.Values == nil {
		return fmt.Errorf("egtb: table %s not generated", t.Mat.Name())
	}
	return writeFile(path, []*Table{t}, t.rawChecksum(), IOWorkers)
}

// LoadTable reads one table from path and checks size and, when recorded,
// the checksum.
func (s *Set) LoadTable(t *Table, path string) error {
	sum, _, err := readFile(path, []*Table{t}, IOWorkers)
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
// constants and written. progress gets a line per step (nil = silent).
func LoadOrGenerate(path string, workers int, progress Progress) *Set {
	if progress == nil {
		progress = func(string) {}
	}
	s := NewSet()
	start := time.Now()
	if err := s.Load(path); err == nil {
		progress(fmt.Sprintf("egtb: loaded %d MB from %s in %.2f s", s.BaseSize()>>20, path, time.Since(start).Seconds()))
		return s
	} else if !errors.Is(err, os.ErrNotExist) {
		progress("egtb: " + err.Error())
	}
	s.GenerateAll(workers, nil)
	progress(fmt.Sprintf("egtb: generated %d tables in %.1f s", len(s.Base()), time.Since(start).Seconds()))
	s.CheckChecksums(progress)
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
