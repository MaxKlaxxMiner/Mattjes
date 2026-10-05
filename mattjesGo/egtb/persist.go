package egtb

import (
	"encoding/binary"
	"fmt"
	"os"
	"path/filepath"
	"time"
	"unsafe"
)

// FileName is the cache file next to the binary: all tables in the order of
// All(), raw values, no header. The layout is fixed in code and the checksum
// of the content is a known constant, so loading is read, hash, compare.
const FileName = "mattjes-egtb.bin"

// FileChecksum is the checksum of the complete file. It doubles as the
// regression test of the generator: a changed generator that still produces
// this value produces the same tables. 0 means "not yet recorded" (the first
// generation prints the value to put here).
const FileChecksum uint64 = 0x865d59cc4efc880b

// TableChecksums are the checksums of the single tables, recorded once from
// the Go generator (2026-10-05) and compared by both generators after every
// generation.
var TableChecksums = map[string]uint64{
	"KQK": 0x426e5bcfec52d05a, "KRK": 0x6ebfdb596a00048f, "KBK": 0x87e64ca38cea838f, "KNK": 0x289514f64bac1d0f,
	"KQQK": 0x8508ca59e98829f3, "KQRK": 0x8e05463b5ff01aec, "KQBK": 0xc530420b344a7195, "KQNK": 0x5d69959d3de7dbad,
	"KRRK": 0xdcc21130b554325a, "KRBK": 0x309970b951da8967, "KRNK": 0xc0edd92ed250667b, "KBBK": 0x8958cfb6fd19de80,
	"KBNK": 0xb1a138142ed9cdbe, "KNNK": 0x70171309263830f1,
	"KQKQ": 0x372ac8be4859c45a, "KQKR": 0xcf68ccf411a0e752, "KQKB": 0xe5ded49325b2e363, "KQKN": 0x57701e4d919b3f82,
	"KRKR": 0x895350eb52831767, "KRKB": 0x4f1733d5c4fb6500, "KRKN": 0x4bc8f873d71d6518, "KBKB": 0xce7ed588fe9c163b,
	"KBKN": 0xa4f14c3ecd9f1b9c, "KNKN": 0xa42e774051095f94,
	"KPK":  0x54ca397ab9f573fe,
	"KQPK": 0xf4e064a417fad31b, "KRPK": 0xe338e13091c22c21, "KBPK": 0x69d6ff4cc467b63a, "KNPK": 0xb943826d8ef75909,
	"KQKP": 0xee37cff419c86800, "KRKP": 0x1dadf3c74939058f, "KBKP": 0xac967d51e02db1cd, "KNKP": 0x1f1a6259674a3d69,
	"KPPK": 0xd7cc08bb6c92e81b, "KPKP": 0x4efb9e3c5e9f75fb,
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

// Checksum of all tables in file order (the tables must be complete).
func (s *Set) Checksum() uint64 {
	// hash of the per-table hashes keeps the tables independent
	h := uint64(0x9e3779b97f4a7c15)
	for _, t := range s.Tables {
		h = (h ^ Checksum(t.Values)) * 0x100000001b3
		h ^= h >> 31
	}
	return h
}

// DefaultPath is the cache file next to the running binary.
func DefaultPath() string {
	exe, err := os.Executable()
	if err != nil {
		return FileName
	}
	return filepath.Join(filepath.Dir(exe), FileName)
}

// Save writes all tables to path.
func (s *Set) Save(path string) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	for _, t := range s.Tables {
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

// Load reads all tables from path and checks size and checksum.
func (s *Set) Load(path string) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	if len(data) != s.TotalSize() {
		return fmt.Errorf("egtb: %s has %d bytes, expected %d", path, len(data), s.TotalSize())
	}
	off := 0
	for _, t := range s.Tables {
		t.Values = make([]Value, t.Size)
		copy(bytesOf(t.Values), data[off:off+t.Size])
		off += t.Size
	}
	if sum := s.Checksum(); FileChecksum != 0 && sum != FileChecksum {
		for _, t := range s.Tables {
			t.Values = nil
		}
		return fmt.Errorf("egtb: %s has checksum %016x, expected %016x", path, sum, FileChecksum)
	}
	return nil
}

// LoadOrGenerate returns the complete set: from the cache file when it is
// present and correct, otherwise freshly generated, checked against the
// constants and written. progress gets a line per step (nil = silent).
func LoadOrGenerate(path string, workers int, progress Progress) *Set {
	if progress == nil {
		progress = func(string) {}
	}
	s := NewSet()
	start := time.Now()
	if err := s.Load(path); err == nil {
		progress(fmt.Sprintf("egtb: loaded %d MB from %s in %.2f s", s.TotalSize()>>20, path, time.Since(start).Seconds()))
		return s
	} else if !os.IsNotExist(err) {
		progress("egtb: " + err.Error())
	}
	s.GenerateAll(workers, nil)
	progress(fmt.Sprintf("egtb: generated %d tables in %.1f s", len(s.Tables), time.Since(start).Seconds()))
	s.CheckChecksums(progress)
	if err := s.Save(path); err != nil {
		progress("egtb: " + err.Error())
	} else {
		progress("egtb: written to " + path)
	}
	return s
}

// CheckChecksums compares every table and the whole set with the recorded
// constants and reports differences (or the values to record, when missing).
// Returns the number of mismatches.
func (s *Set) CheckChecksums(progress Progress) int {
	bad := 0
	for _, t := range s.Tables {
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
