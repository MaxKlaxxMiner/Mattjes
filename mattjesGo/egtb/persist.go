package egtb

import (
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
// File format: 4 bytes magic "MEGT", 8 bytes raw checksum (the constant in the
// code, over the values as generated, Invalid entries included), then the lz
// container with the values. Before compression every Invalid entry is
// replaced by its predecessor: those positions are never looked up, so their
// content is free and the runs get longer. A file whose size equals the raw
// table size is accepted as uncompressed (the layout before compression).
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
const FileChecksum uint64 = 0x7df825b0564bbe7e

// TableChecksums are the raw checksums of the single tables (move count per
// byte, recorded 2026-10-07 from the Rust generator, Go identical) and are
// compared after every generation and load. Larger materials are added as
// they are measured.
var TableChecksums = map[string]uint64{
	"KQK": 0xc46f0039d9183a89, "KRK": 0xb242548554aa6f9b, "KBK": 0x87e64ca38cea838f, "KNK": 0x289514f64bac1d0f,
	"KQQK": 0x924b1b4b52caf42e, "KQRK": 0x6c8a08fc3c1365bf, "KQBK": 0x71626d49da26e26f, "KQNK": 0x5ee4cd974f5fcc68,
	"KRRK": 0x31a4c5182277769c, "KRBK": 0x07352bb55b72623b, "KRNK": 0x5192db5ddc19851b, "KBBK": 0x5366a78b60d00d20,
	"KBNK": 0xc044466bf38479fc, "KNNK": 0x70171309263830f1,
	"KQKQ": 0xbfdacb5f1f211bdc, "KQKR": 0xdcf2a1f4a0ab2684, "KQKB": 0xd201a4bd72abe6ac, "KQKN": 0xe121d7a8d8cb7799,
	"KRKR": 0x45f769817ad860bc, "KRKB": 0x1738e67a374583fd, "KRKN": 0x69f5936f1a695510, "KBKB": 0xce7ed588fe9c163b,
	"KBKN": 0xa4f14c3ecd9f1b9c, "KNKN": 0xa42e774051095f94,
	"KPK":  0xe7bf5853573f7e18,
	"KQPK": 0xc44b15fda79131aa, "KRPK": 0x1f2a1dea38c16ef6, "KBPK": 0x7fdf1834a126aecd, "KNPK": 0xb375a451fa224576,
	"KQKP": 0xdc68d6d275d18fe9, "KRKP": 0xf47d41f96cf7c962, "KBKP": 0xa357723843c99501, "KNKP": 0x6bee17ca085a8e37,
	"KPPK": 0x8e7fbcb01d11a97a, "KPKP": 0x531a4a773b5355b5,
	// five pieces (2026-10-07, Rust 12 threads: KBBBK 4 s, KBNKQ 46 s; Go identical)
	"KBBBK": 0x985cad50bec78a9d, "KBNKQ": 0xb52d6cdf4e9bee6f,
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

// fill replaces Invalid entries by their predecessor ("don't care"), once.
func (t *Table) fill() {
	if t.filled {
		return
	}
	t.rawChecksum()
	prev := Draw
	for i, v := range t.Values {
		if v == Invalid {
			t.Values[i] = prev
		} else {
			prev = v
		}
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

// readFile reads a cache file into the tables (compressed or legacy raw).
// Returns the raw checksum from the header (or computed for a raw file) and
// whether the file was compressed.
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
	}
	if len(data) == size { // legacy: raw values, Invalid entries present
		off := 0
		for _, t := range tables {
			copy(bytesOf(t.Values), data[off:off+t.Size])
			off += t.Size
		}
		if len(tables) == 1 {
			tables[0].rawChecksum()
			return tables[0].RawChecksum, false, nil
		}
		return 0, false, nil
	}
	if len(data) < headerBytes || string(data[:4]) != magic {
		return 0, false, fmt.Errorf("egtb: %s is neither a raw table (%d bytes) nor a cache file", path, size)
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
	sum, compressed, err := readFile(path, s.Base(), IOWorkers)
	if err != nil {
		clearTables(s.Base())
		return err
	}
	if !compressed {
		sum = s.Checksum() // raw file: hash the values
	} else {
		// the header carries the combined checksum of the generator run; the
		// single table checksums cannot be recomputed from filled values, so
		// the recorded constants stand in for them
		for _, t := range s.Base() {
			t.RawChecksum = TableChecksums[t.Mat.Name()]
		}
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

// CompressCacheDir rewrites every raw table file in the cache directory in
// the compressed format (the base file and all materials), one file at a time.
// Returns the number of converted files and the raw and compressed byte totals.
func CompressCacheDir(dir string, workers int, progress Progress) (converted int, rawBytes, packedBytes int64, err error) {
	if progress == nil {
		progress = func(string) {}
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		return 0, 0, 0, err
	}
	for _, e := range entries {
		name := e.Name()
		if !strings.HasSuffix(name, ".bin") {
			continue
		}
		path := filepath.Join(dir, name)
		info, err := e.Info()
		if err != nil {
			return converted, rawBytes, packedBytes, err
		}
		s := NewSet()
		var tables []*Table
		var sum uint64
		if name == BaseFileName {
			if int(info.Size()) != s.BaseSize() {
				progress("egtb: " + name + " already compressed")
				continue
			}
			if err := s.Load(path); err != nil {
				return converted, rawBytes, packedBytes, err
			}
			tables, sum = s.Base(), s.Checksum()
		} else {
			parts := strings.SplitN(strings.TrimSuffix(name, ".bin"), "-", 2)
			if len(parts) != 2 {
				continue
			}
			m, err := Parse(parts[1])
			if err != nil {
				progress("egtb: skipping " + name + ": " + err.Error())
				continue
			}
			t := s.AddMaterial(m)
			if int(info.Size()) != t.Size {
				progress("egtb: " + name + " already compressed")
				continue
			}
			if err := s.LoadTable(t, path); err != nil {
				return converted, rawBytes, packedBytes, err
			}
			tables, sum = []*Table{t}, t.rawChecksum()
		}
		start := time.Now()
		if err := writeFile(path, tables, sum, workers); err != nil {
			return converted, rawBytes, packedBytes, err
		}
		after, _ := os.Stat(path)
		converted++
		rawBytes += info.Size()
		packedBytes += after.Size()
		progress(fmt.Sprintf("egtb: %s %d MB -> %d MB (%.2fx) in %.1f s, checksum %016x", name, info.Size()>>20, after.Size()>>20, float64(info.Size())/float64(after.Size()), time.Since(start).Seconds(), sum))
	}
	return converted, rawBytes, packedBytes, nil
}
