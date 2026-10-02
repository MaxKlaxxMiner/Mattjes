package ttstore

import (
	"encoding/binary"
	"fmt"
	"io"
	"os"
	"unsafe"
)

// File format: 40-byte little-endian header, then the raw slots. Same
// fingerprint rule as package tt.
type header struct {
	Magic, Version, Kind, EntryBytes uint32
	Slots, Count, Fingerprint        uint64
}

const (
	fileMagic   = 0x5354_4A4D // "MJTS"
	fileVersion = 1
	kindStore   = 3
)

func asBytes(s []Key) []byte {
	return unsafe.Slice((*byte)(unsafe.Pointer(&s[0])), len(s)*EntryBytes)
}

// Save dumps the store to path.
func (s *Store) Save(path string, fingerprint uint64) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	h := header{fileMagic, fileVersion, kindStore, EntryBytes, uint64(len(s.entries)), uint64(s.count), fingerprint}
	if err := binary.Write(f, binary.LittleEndian, h); err != nil {
		return err
	}
	_, err = f.Write(asBytes(s.entries))
	return err
}

// Load reads a store written by Save.
func Load(path string, fingerprint uint64) (*Store, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	var h header
	if err := binary.Read(f, binary.LittleEndian, &h); err != nil {
		return nil, err
	}
	switch {
	case h.Magic != fileMagic:
		return nil, fmt.Errorf("not a ttstore file")
	case h.Version != fileVersion:
		return nil, fmt.Errorf("ttstore file version %d, expected %d", h.Version, fileVersion)
	case h.Kind != kindStore:
		return nil, fmt.Errorf("ttstore file kind %d, expected %d", h.Kind, kindStore)
	case h.EntryBytes != EntryBytes:
		return nil, fmt.Errorf("ttstore file entry size %d, expected %d", h.EntryBytes, EntryBytes)
	case h.Fingerprint != fingerprint:
		return nil, fmt.Errorf("ttstore file written with other zobrist constants (%016x, expected %016x)", h.Fingerprint, fingerprint)
	case h.Slots == 0 || h.Slots&(h.Slots-1) != 0:
		return nil, fmt.Errorf("ttstore file slot count %d is not a power of two", h.Slots)
	}
	s := NewSlots(int(h.Slots))
	s.count = int(h.Count)
	if _, err := io.ReadFull(f, asBytes(s.entries)); err != nil {
		return nil, err
	}
	return s, nil
}
