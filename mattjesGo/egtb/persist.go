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

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/lz"
)

// The cache lives in a directory next to the binary. The base file holds all
// tables up to four pieces in the order of All(); every larger material gets
// its own file "<pieces>-<name>.bin" (never shipped, only generated locally).
//
// File format (v3, 2026-10-16): 4 bytes magic "MEG3", 1 byte step (the
// resolution of the values in moves, Table.Step: 1 = exact, n = buckets of n
// moves), 1 byte value width (0 or 1 = one byte per entry, 2 = a wide table,
// since 2026-10-26: the container then holds 2n bytes, the low plane of all
// entries, then the high plane, Value.ToPlanes), 2 bytes zero, 8 bytes raw
// checksum (the constant in the code, over
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
	// five pieces, all 110 (format v3; from the sweep of 2026-10-23 on the home machine, Rust,
	// measure.log; KBBBK, KBNKQ and KRRKN agree with the earlier Go and Rust runs)
	"KBBBK": 0xa552a70f5be1c3db, "KBBKB": 0x72234cc21a7defaf, "KBBKN": 0xcac529d6b477774b, "KBBKQ": 0xd12a7747cc7d9f45,
	"KBBKR": 0x25fcfbb6a10810e8, "KBBNK": 0x234d18bcb557ad7f, "KBNKB": 0xfd2f437a0a54b862, "KBNKN": 0x12113e9a5e7c4e48,
	"KBNKQ": 0xa03df227d9f5eb72, "KBNKR": 0x187c046383539781, "KBNNK": 0xda5cd512f702000a, "KNNKB": 0xdaa5176ca837e544,
	"KNNKN": 0x2cadd8a85308b7cf, "KNNKQ": 0xb723b4fae7715b91, "KNNKR": 0x3b77145d4f6957c5, "KNNNK": 0xa44c31959604c935,
	"KQBBK": 0x54972d4b84afa912, "KQBKB": 0x08c8342dc0e58d3e, "KQBKN": 0xd1fbb25d97b6f3a3, "KQBKQ": 0x3aa37ee109fb585f,
	"KQBKR": 0x0be7752be640c86c, "KQBNK": 0x81586b6e6f1ccc6d, "KQNKB": 0xa342715277bab8c6, "KQNKN": 0x10749f36888b538a,
	"KQNKQ": 0x878542d0f0bdfe00, "KQNKR": 0x9f4a7fdb8db76e86, "KQNNK": 0xfafc1f102f4d3edb, "KQQBK": 0x6b79d5e753baa689,
	"KQQKB": 0xfed65a59f7c286cf, "KQQKN": 0x5eaeaf8457ca0e25, "KQQKQ": 0x32f36f5b986ac0de, "KQQKR": 0x67d79a19f27e30ad,
	"KQQNK": 0xd7696e03b378e695, "KQQQK": 0x26ccd333c5249437, "KQQRK": 0x46018430785e3443, "KQRBK": 0x24bdc7ea83a73e37,
	"KQRKB": 0xaa14ca6028d40b9a, "KQRKN": 0xc67bd8787b1cb2d4, "KQRKQ": 0x9ace8ab5334edd2f, "KQRKR": 0x280529af63d896c6,
	"KQRNK": 0x2e0b74455ddcdb70, "KQRRK": 0x5925eaafa071bf0d, "KRBBK": 0x8973a57db42adaae, "KRBKB": 0xec0971f832a3ec14,
	"KRBKN": 0x8a0c296adea78498, "KRBKQ": 0x37b8a37e2578a4c4, "KRBKR": 0xf1eeeb2878de49a7, "KRBNK": 0x777caf348dd076b9,
	"KRNKB": 0x74b159efc1a16d9c, "KRNKN": 0xf8abb5381fb82d8a, "KRNKQ": 0x5ebac3e35ae6129d, "KRNKR": 0xf176012ec8aa7c0f,
	"KRNNK": 0x8067f92cf8f05172, "KRRBK": 0x955121c5e7f34b2a, "KRRKB": 0x1b2da1009c1b09ab, "KRRKN": 0x443a1b3b48954ec3,
	"KRRKQ": 0xd7a15047fa0ad275, "KRRKR": 0xbe96f115eba44051, "KRRNK": 0xa75e70e701bbf8ac, "KRRRK": 0x38031bfa0e98bbee,
	"KBBKP": 0x11ded18695795d6c, "KBBPK": 0x1e90b35dd894b11e, "KBNKP": 0xfe0d3bb9d3e5667f, "KBNPK": 0xcc3f466efcc94af1,
	"KBPKB": 0x3d83b0fd492bc18e, "KBPKN": 0x4000947a38cd27ea, "KBPKQ": 0x5365d229c58e74e2, "KBPKR": 0xc9718c864cbcc955,
	"KNNKP": 0x2fe018737a9a2387, "KNNPK": 0xd83c464672e30fe1, "KNPKB": 0x71772e64bd51012a, "KNPKN": 0x5e0bfb8e51dd84e5,
	"KNPKQ": 0x42deff8d914c91da, "KNPKR": 0xc48002f00bdb4b09, "KQBKP": 0x5fe49081bd07c0d9, "KQBPK": 0xca397e89179e8f73,
	"KQNKP": 0x835f7e19d227df05, "KQNPK": 0x36e0f2e1db352887, "KQPKB": 0x4828f117c85b5f39, "KQPKN": 0xb70e06a378558b50,
	"KQPKQ": 0x629bb099a6a90e07, "KQPKR": 0xce224ecf69d3fdb4, "KQQKP": 0xa5674b645afe063b, "KQQPK": 0x15aa245d511d4a3b,
	"KQRKP": 0xc3f60f7b8c6ee78a, "KQRPK": 0x6a3d308b1f468e66, "KRBKP": 0x23b40adbdf6c4fed, "KRBPK": 0x01df5843fde7058a,
	"KRNKP": 0x97ff32bf6d501a1e, "KRNPK": 0x77e0598d862784ac, "KRPKB": 0x0d3e7a4b90d9e182, "KRPKN": 0x57d74af89ec77093,
	"KRPKQ": 0x3055982894f56ddc, "KRPKR": 0x7cc6ba239baf5da9, "KRRKP": 0x3003050b376b4546, "KRRPK": 0x8a1494460fb7f479,
	"KBPKP": 0x75ea61ed8bf95531, "KBPPK": 0xecc3de59fb86dff7, "KNPKP": 0x7c2e668122934016, "KNPPK": 0xba0f257aa2ecc2ed,
	"KPPKB": 0xcc9a627ad3f55921, "KPPKN": 0x5c456acab2bbab19, "KPPKQ": 0x902875aaa6992709, "KPPKR": 0x4c92580848463cd6,
	"KQPKP": 0xfeeca92d9b94747d, "KQPPK": 0x0826d0219e8be912, "KRPKP": 0x62929c193fa9a46d, "KRPPK": 0x71e1d1f22a9e53e8,
	"KPPKP": 0x87cb3b021b3416b0, "KPPPK": 0x9b0c4f0c85c11d02,
	// six pieces (format v3, Rust, home machine): KRRKBN 1294 s, 35 GiB committed
	"KRRKBN": 0x11049fde56c4b77c,
}

// Checksum is a 64-bit hash over 8-byte little-endian words (FNV-1a style
// with a final mix), fast enough for 170 MB and trivial to reproduce in Rust.
// A table's checksum covers Table.bytes.
func Checksum(data []byte) uint64 {
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

// rawChecksum returns the checksum of the generated values, computing it when
// the table was never saved or loaded.
func (t *Table) rawChecksum() uint64 {
	if t.RawChecksum == 0 && !t.filled && t.Generated() {
		t.RawChecksum = Checksum(t.bytes())
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
		var prev byte
		for i, v := range t.Values {
			if t.invalid.get(i) {
				t.Values[i] = prev
			} else {
				prev = v
			}
		}
		var prevWide uint16
		for i, v := range t.Wide {
			if t.invalid.get(i) {
				t.Wide[i] = prevWide
			} else {
				prevWide = v
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

// The cache folder holds the base file at its root; the tables beyond the
// base live in two subfolders: DtmDir for the (quantised) distance tables
// and WdlDir for our own WDL tables in Syzygy's structure (package syzygy).
const (
	DtmDir = "dtm"
	WdlDir = "wdl"
)

// TablePath is the cache file of a material beyond the base, e.g. "dtm/5-KBNKQ.bin".
func (s *Set) TablePath(t *Table) string {
	return filepath.Join(s.CacheDir, DtmDir, fmt.Sprintf("%d-%s.bin", t.Mat.Pieces(), t.Mat.Name()))
}

// DtmPath is the folder of the distance tables beyond the base.
func (s *Set) DtmPath() string { return filepath.Join(s.CacheDir, DtmDir) }

// WdlPath is the folder of our own WDL tables.
func (s *Set) WdlPath() string { return filepath.Join(s.CacheDir, WdlDir) }

// WdlFile is the own WDL table of a material, e.g. "wdl/5-KBNKQ.wdl".
func (s *Set) WdlFile(m Material) string {
	return filepath.Join(s.WdlPath(), fmt.Sprintf("%d-%s.wdl", m.Pieces(), m.Name()))
}

// DtmFile is the own DTM table of a material, e.g. "dtm/5-KBNKQ.dtm".
func (s *Set) DtmFile(m Material) string {
	return filepath.Join(s.DtmPath(), fmt.Sprintf("%d-%s.dtm", m.Pieces(), m.Name()))
}

// Release drops the values of a table beyond the base from memory (and
// closes its file, when it was loaded packed); the table stays registered
// and can be loaded or generated again.
func (s *Set) Release(t *Table) { clearTables([]*Table{t}) }

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
		if !t.Generated() {
			return fmt.Errorf("egtb: table %s not generated", t.Mat.Name())
		}
		if t.Step != step {
			return fmt.Errorf("egtb: table %s has step %d, the file needs %d", t.Mat.Name(), t.Step, step)
		}
		t.fill()
	}
	// a single table (every material beyond the base) is packed in place: no
	// copy of 15 GB for a six-piece table; only the base file concatenates
	wide := len(tables) == 1 && tables[0].IsWide()
	var raw []byte
	if len(tables) == 1 {
		raw = tables[0].Values
	} else {
		raw = make([]byte, 0, sumSize(tables))
		for _, t := range tables {
			if t.IsWide() {
				return fmt.Errorf("egtb: table %s is wide, only single tables can be", t.Mat.Name())
			}
			raw = append(raw, t.Values...)
		}
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	// written beside the target and renamed when complete: a reader never
	// sees a partial file (a sweep may be killed while writing)
	tmp := TmpPath(path)
	f, err := os.Create(tmp)
	if err != nil {
		return err
	}
	w := bufio.NewWriterSize(f, 1<<20)
	var header [headerBytes]byte
	copy(header[:], magic)
	header[4] = byte(step)
	header[5] = 1
	if wide {
		header[5] = 2
	}
	binary.LittleEndian.PutUint64(header[8:], rawChecksum)
	if _, err := w.Write(header[:]); err != nil {
		f.Close()
		return err
	}
	if wide {
		// the two planes, built block by block from the entries
		v := tables[0].Wide
		n := len(v)
		err = lz.PackIndexedWith(w, 2*n, blockSize, workers, func(off int, dst []byte) {
			for j := range dst {
				if pos := off + j; pos < n {
					dst[j], _ = Value(v[pos]).ToPlanes()
				} else {
					_, dst[j] = Value(v[pos-n]).ToPlanes()
				}
			}
		})
	} else {
		err = lz.PackIndexedTo(w, raw, blockSize, workers)
	}
	if err != nil {
		f.Close()
		return err
	}
	if err := w.Flush(); err != nil {
		f.Close()
		return err
	}
	if err := f.Close(); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

// TmpPath is the temporary twin a file is written to before it is renamed
// into place ("5-KBNKQ.dtm.tmp"): a reader never sees a partial file.
func TmpPath(path string) string { return path + ".tmp" }

func sumSize(tables []*Table) int {
	n := 0
	for _, t := range tables {
		n += t.Size
	}
	return n
}

// openFile opens a cache file and reads its header and block index; the
// caller either unpacks everything (readFile) or keeps the file for block
// reads (openTable). size is the number of entries the tables expect; wide
// reports two planes.
func openFile(path string, size int) (f *os.File, ix *lz.Index, step int, rawChecksum uint64, wide bool, err error) {
	f, err = os.Open(path)
	if err != nil {
		return nil, nil, 0, 0, false, err
	}
	fail := func(err error) (*os.File, *lz.Index, int, uint64, bool, error) {
		f.Close()
		return nil, nil, 0, 0, false, err
	}
	var header [headerBytes]byte
	if n, err := f.ReadAt(header[:], 0); n < headerBytes || string(header[:4]) != magic {
		return fail(fmt.Errorf("egtb: %s is not a cache file (format %q, regenerate; %v)", path, string(header[:4]), err))
	}
	step = max(1, int(header[4]))
	switch header[5] {
	case 0, 1:
	case 2:
		wide = true
	default:
		return fail(fmt.Errorf("egtb: %s has %d bytes per entry", path, header[5]))
	}
	rawChecksum = binary.LittleEndian.Uint64(header[8:])
	ix, err = lz.ReadIndex(f, headerBytes)
	if err != nil {
		return fail(fmt.Errorf("egtb: %s: %v", path, err))
	}
	want := size
	if wide {
		want = 2 * size
	}
	if ix.RawLen != want {
		return fail(fmt.Errorf("egtb: %s holds %d bytes, expected %d", path, ix.RawLen, want))
	}
	return f, ix, step, rawChecksum, wide, nil
}

// readFile reads a cache file into the tables as a whole (the base).
// Returns the raw checksum from the header; the entries are filled (dead and
// illegal ones hold their predecessor) and the tables carry the file's step.
func readFile(path string, tables []*Table, workers int) (rawChecksum uint64, compressed bool, err error) {
	clearTables(tables)
	f, ix, step, rawChecksum, wide, err := openFile(path, sumSize(tables))
	if err != nil {
		return 0, false, err
	}
	defer f.Close()
	if wide && len(tables) != 1 {
		return 0, true, fmt.Errorf("egtb: %s is wide, only single tables can be", path)
	}
	for _, t := range tables {
		t.allocate(wide)
	}
	if wide {
		// each plane block puts its bytes into the entries (low or high
		// byte, distinct bytes per block), then every entry turns from the
		// plane form into its value
		t := tables[0]
		n := t.Size
		raw := t.bytes() // little endian: low byte at 2i, high byte at 2i+1
		err := ix.UnpackEach(f, workers, func(off int, data []byte) {
			for j, b := range data {
				if pos := off + j; pos < n {
					raw[2*pos] = b
				} else {
					raw[2*(pos-n)+1] = b
				}
			}
		})
		if err != nil {
			return 0, true, fmt.Errorf("egtb: %s: %v", path, err)
		}
		for i, w := range t.Wide {
			t.Wide[i] = uint16(ValueOfPlanes(byte(w), byte(w>>8)))
		}
		t.filled, t.Step = true, step
		return rawChecksum, true, nil
	}
	// decompress into one buffer, then split (the tables are separate slices)
	var dst []byte
	if len(tables) == 1 {
		dst = tables[0].Values
	} else {
		dst = make([]byte, ix.RawLen)
	}
	if err := ix.UnpackAll(f, dst, workers); err != nil {
		return 0, true, fmt.Errorf("egtb: %s: %v", path, err)
	}
	if len(tables) > 1 {
		off := 0
		for _, t := range tables {
			copy(t.Values, dst[off:off+t.Size])
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
	f, ix, step, rawChecksum, wide, err := openFile(path, t.Size)
	if err != nil {
		return 0, err
	}
	wideEntries := 0
	if wide {
		wideEntries = t.Size
	}
	t.packed = newBlockStore(f, ix, wideEntries)
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
		t.Values, t.Wide = nil, nil
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
	if !t.Generated() {
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
	if !t.Generated() {
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
	return s.LoadRawTableChecked(t, path, true)
}

// LoadRawTableChecked is LoadRawTable, comparing with the recorded checksum
// only when check is set (a test widens a recorded one-byte table on purpose).
func (s *Set) LoadRawTableChecked(t *Table, path string, check bool) error {
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
	if want, known := TableChecksums[t.Mat.Name()]; known && check && sum != want {
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
	return s.LoadTableChecked(t, path, true)
}

// LoadTableChecked is LoadTable, comparing with the recorded checksum only
// when check is set.
func (s *Set) LoadTableChecked(t *Table, path string, check bool) error {
	sum, err := openTable(path, t)
	if err != nil {
		clearTables([]*Table{t})
		return err
	}
	t.RawChecksum = sum
	if want, known := TableChecksums[t.Mat.Name()]; known && check && sum != want {
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
	if t.Generated() {
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
