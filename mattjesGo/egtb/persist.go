package egtb

import (
	"encoding/binary"
	"fmt"
	"os"
	"path/filepath"
	"time"
	"unsafe"
)

// The cache lives in a directory next to the binary. The base file holds all
// tables up to four pieces in the order of All(), raw values, no header; every
// larger material gets its own file "<pieces>-<name>.bin" (never shipped,
// only generated locally). Layouts are fixed in code and the checksums are
// known constants, so loading is read, hash, compare.
const (
	CacheDir     = "mattjes-egtb-cache"
	BaseFileName = "4-all.bin"
)

// FileChecksum is the checksum of the complete base file. It doubles as the
// regression test of the generator: a changed generator that still produces
// this value produces the same tables. 0 means "not yet recorded" (the first
// generation prints the value to put here).
const FileChecksum uint64 = 0x7df825b0564bbe7e

// TableChecksums are the checksums of the single tables (format v2, recorded
// 2026-10-07 from the Rust generator, Go identical) and compared after every
// generation and load. Larger materials are added as they are measured.
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

// Checksum of the base tables in file order (they must be complete): a hash
// of the per-table hashes keeps the tables independent.
func (s *Set) Checksum() uint64 {
	h := uint64(0x9e3779b97f4a7c15)
	for _, t := range s.Base() {
		h = (h ^ Checksum(t.Values)) * 0x100000001b3
		h ^= h >> 31
	}
	return h
}

// BaseSize is the number of bytes of the base file.
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

// TablePath is the cache file of a material beyond the base, e.g. "5-KQKBN.bin".
func (s *Set) TablePath(t *Table) string {
	return filepath.Join(DefaultCacheDir(), fmt.Sprintf("%d-%s.bin", t.Mat.Pieces(), t.Mat.Name()))
}

// Save writes the base tables to path.
func (s *Set) Save(path string) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	for _, t := range s.Base() {
		if t.Values == nil {
			f.Close()
			return fmt.Errorf("egtb: table %s not generated", t.Mat.Name())
		}
		if _, err := f.Write(bytesOf(t.Values)); err != nil {
			f.Close()
			return err
		}
	}
	return f.Close()
}

// Load reads the base tables from path and checks size and checksum.
func (s *Set) Load(path string) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	if len(data) != s.BaseSize() {
		return fmt.Errorf("egtb: %s has %d bytes, expected %d", path, len(data), s.BaseSize())
	}
	off := 0
	for _, t := range s.Base() {
		t.Values = make([]Value, t.Size)
		copy(bytesOf(t.Values), data[off:off+t.Size])
		off += t.Size
	}
	if sum := s.Checksum(); FileChecksum != 0 && sum != FileChecksum {
		for _, t := range s.Base() {
			t.Values = nil
		}
		return fmt.Errorf("egtb: %s has checksum %016x, expected %016x", path, sum, FileChecksum)
	}
	return nil
}

// SaveTable writes one table (a material beyond the base) to path.
func (s *Set) SaveTable(t *Table, path string) error {
	if t.Values == nil {
		return fmt.Errorf("egtb: table %s not generated", t.Mat.Name())
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	return os.WriteFile(path, bytesOf(t.Values), 0o644)
}

// LoadTable reads one table from path and checks size and, when recorded,
// the checksum.
func (s *Set) LoadTable(t *Table, path string) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	if len(data) != t.Size {
		return fmt.Errorf("egtb: %s has %d bytes, expected %d", path, len(data), t.Size)
	}
	t.Values = make([]Value, t.Size)
	copy(bytesOf(t.Values), data)
	if want, known := TableChecksums[t.Mat.Name()]; known {
		if sum := Checksum(t.Values); sum != want {
			t.Values = nil
			return fmt.Errorf("egtb: %s has checksum %016x, expected %016x", path, sum, want)
		}
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
	} else if !os.IsNotExist(err) {
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
	} else if !os.IsNotExist(err) {
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
		sum := Checksum(t.Values)
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
