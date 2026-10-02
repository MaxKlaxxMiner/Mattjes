package tt

import (
	"encoding/binary"
	"fmt"
	"io"
	"os"
	"unsafe"
)

// File format: a 32-byte little-endian header followed by the raw entries. The
// fingerprint identifies the Zobrist constants (bitboard.ZobristFingerprint), so
// a file written with other constants is rejected instead of producing silent
// false hits. Go and Rust use the same constants and the same layout, so a table
// saved by one can be loaded by the other.
type header struct {
	Magic, Version, Kind, EntryBytes uint32
	Slots, Fingerprint               uint64
}

const (
	fileMagic   = 0x5454_4A4D // "MJTT"
	fileVersion = 1
	kindTable   = 1
	kindBuckets = 2
)

func entriesAsBytes(e []entry) []byte {
	return unsafe.Slice((*byte)(unsafe.Pointer(&e[0])), len(e)*EntryBytes)
}

func bucketsAsBytes(b []bucket) []byte {
	return unsafe.Slice((*byte)(unsafe.Pointer(&b[0])), len(b)*bucketBytes)
}

func writeFile(path string, h header, body []byte) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	if err := binary.Write(f, binary.LittleEndian, h); err != nil {
		return err
	}
	_, err = f.Write(body)
	return err
}

// readHeader validates magic, version, kind, entry size and fingerprint and
// returns the header; the file position is at the first entry afterwards.
func readHeader(f *os.File, kind, entryBytes uint32, fingerprint uint64) (header, error) {
	var h header
	if err := binary.Read(f, binary.LittleEndian, &h); err != nil {
		return h, err
	}
	switch {
	case h.Magic != fileMagic:
		return h, fmt.Errorf("not a tt file")
	case h.Version != fileVersion:
		return h, fmt.Errorf("tt file version %d, expected %d", h.Version, fileVersion)
	case h.Kind != kind:
		return h, fmt.Errorf("tt file kind %d, expected %d", h.Kind, kind)
	case h.EntryBytes != entryBytes:
		return h, fmt.Errorf("tt file entry size %d, expected %d", h.EntryBytes, entryBytes)
	case h.Fingerprint != fingerprint:
		return h, fmt.Errorf("tt file written with other zobrist constants (%016x, expected %016x)", h.Fingerprint, fingerprint)
	case h.Slots == 0 || h.Slots&(h.Slots-1) != 0:
		return h, fmt.Errorf("tt file slot count %d is not a power of two", h.Slots)
	}
	return h, nil
}

// Save dumps the table to path.
func (t *Table) Save(path string, fingerprint uint64) error {
	return writeFile(path, header{fileMagic, fileVersion, kindTable, EntryBytes, uint64(len(t.entries)), fingerprint}, entriesAsBytes(t.entries))
}

// Load reads a table written by Save. Statistics start at zero.
func Load(path string, fingerprint uint64) (*Table, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	h, err := readHeader(f, kindTable, EntryBytes, fingerprint)
	if err != nil {
		return nil, err
	}
	t := newTable(int(h.Slots))
	if _, err := io.ReadFull(f, entriesAsBytes(t.entries)); err != nil {
		return nil, err
	}
	return t, nil
}

// Save dumps the buckets to path.
func (t *Buckets) Save(path string, fingerprint uint64) error {
	return writeFile(path, header{fileMagic, fileVersion, kindBuckets, bucketBytes, uint64(len(t.buckets)), fingerprint}, bucketsAsBytes(t.buckets))
}

// LoadBuckets reads a table written by Buckets.Save.
func LoadBuckets(path string, fingerprint uint64) (*Buckets, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	h, err := readHeader(f, kindBuckets, bucketBytes, fingerprint)
	if err != nil {
		return nil, err
	}
	t := newBuckets(int(h.Slots))
	if _, err := io.ReadFull(f, bucketsAsBytes(t.buckets)); err != nil {
		return nil, err
	}
	return t, nil
}
