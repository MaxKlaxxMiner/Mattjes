// Package lz is a minimal LZ77 compressor for the endgame table cache, from the
// author's own tests (webwerfgo/lz.go, 2026-10-06): no dependencies, easy to
// port, and tuned for what the tables look like. Decoding is a plain LZ4 block
// decoder (over 1 GB/s per core), encoding uses hash chains with lazy matching,
// which finds the repetitions between the 64-byte rows of a table (same kings
// and first piece, the last piece on a neighbouring square) and beats the
// order-0 entropy by a factor of two. Blocks are independent, so both
// directions run in parallel.
//
// Block payload = LZ4 block format:
//
//	sequence := token, [litlen ext], literals, offset (u16 LE), [matchlen ext]
//	token    := litlen<<4 | (matchlen-4); nibble 15 = extended by 255-bytes
//	the last sequence has literals only; the last 5 bytes of a block are literals.
//
// Container: rawLen u64, blockSize u32, blockCount u32, then per block
// compLen u32 and the payload (all little endian).
//
// Indexed container (2026-10-16, for files that are read block by block
// instead of unpacked as a whole): the same 16-byte header, then
// blockCount+1 offsets u64 of the compressed blocks relative to the first
// block (the last one is the total compressed size), then the blocks back to
// back. A block decodes alone, so a reader serves any raw position by
// reading one compressed block (Index.ReadBlock), or everything at once
// (Index.UnpackAll).
package lz

import (
	"bytes"
	"encoding/binary"
	"errors"
	"io"
	"sync"
)

const (
	MinMatch  = 4
	HashBits  = 16
	MaxOffset = 1<<16 - 1
	BlockSize = 4 << 20
	// Depth is the number of hash chain candidates tried per position.
	Depth = 256
)

func hash(v uint32) uint32 { return (v * 2654435761) >> (32 - HashBits) }

func le32(b []byte) uint32 { return binary.LittleEndian.Uint32(b) }

// compressBlock encodes one block.
func compressBlock(src []byte, depth int) []byte {
	head := make([]int32, 1<<HashBits)
	for i := range head {
		head[i] = -1
	}
	prev := make([]int32, len(src))
	dst := make([]byte, 0, len(src)/3+16)

	writeLen := func(n int) {
		for n >= 255 {
			dst = append(dst, 255)
			n -= 255
		}
		dst = append(dst, byte(n))
	}
	emit := func(lit []byte, mlen, ofs int) {
		ll := len(lit)
		ml := 0
		if mlen > 0 {
			ml = mlen - MinMatch
		}
		dst = append(dst, byte(min(ll, 15)<<4)|byte(min(ml, 15)))
		if ll >= 15 {
			writeLen(ll - 15)
		}
		dst = append(dst, lit...)
		if mlen > 0 {
			dst = append(dst, byte(ofs), byte(ofs>>8))
			if ml >= 15 {
				writeLen(ml - 15)
			}
		}
	}
	insert := func(p int) {
		h := hash(le32(src[p:]))
		prev[p] = head[h]
		head[h] = int32(p)
	}
	limit := len(src) - 5
	find := func(p int) (bestLen, bestOfs int) {
		cand := int(head[hash(le32(src[p:]))])
		for d := 0; d < depth && cand >= 0 && p-cand <= MaxOffset; d++ {
			if src[cand+bestLen] == src[p+bestLen] && le32(src[cand:]) == le32(src[p:]) {
				l := MinMatch
				for p+l < limit && src[cand+l] == src[p+l] {
					l++
				}
				if l > bestLen {
					bestLen, bestOfs = l, p-cand
				}
			}
			cand = int(prev[cand])
		}
		return
	}

	litStart, p := 0, 0
	for p+MinMatch <= limit {
		bestLen, bestOfs := find(p)
		if bestLen < MinMatch {
			insert(p)
			p++
			continue
		}
		from := 0
		if p+1+MinMatch <= limit { // lazy: prefer a longer match one byte later
			insert(p)
			from = 1
			if l2, _ := find(p + 1); l2 > bestLen {
				p++
				continue
			}
		}
		emit(src[litStart:p], bestLen, bestOfs)
		for i := from; i < bestLen; i++ {
			insert(p + i)
		}
		p += bestLen
		litStart = p
	}
	emit(src[litStart:], 0, 0)
	return dst
}

// decompressBlock decodes src into dst, which must have exactly the raw length.
func decompressBlock(dst, src []byte) error {
	s, d := 0, 0
	readLen := func(n int) int {
		for s < len(src) && src[s] == 255 {
			n += 255
			s++
		}
		if s >= len(src) {
			return -1
		}
		n += int(src[s])
		s++
		return n
	}
	for s < len(src) {
		tok := int(src[s])
		s++
		ll := tok >> 4
		if ll == 15 {
			if ll = readLen(15); ll < 0 {
				return errors.New("lz: truncated literal length")
			}
		}
		if s+ll > len(src) || d+ll > len(dst) {
			return errors.New("lz: literals out of range")
		}
		copy(dst[d:d+ll], src[s:s+ll])
		s += ll
		d += ll
		if s >= len(src) {
			break
		}
		if s+2 > len(src) {
			return errors.New("lz: truncated offset")
		}
		ofs := int(src[s]) | int(src[s+1])<<8
		s += 2
		ml := tok&15 + MinMatch
		if ml == 15+MinMatch {
			if ml = readLen(ml); ml < 0 {
				return errors.New("lz: truncated match length")
			}
		}
		m := d - ofs
		if ofs == 0 || m < 0 || d+ml > len(dst) {
			return errors.New("lz: match out of range")
		}
		if ofs >= ml {
			copy(dst[d:d+ml], dst[m:m+ml])
		} else { // overlapping match (a run), must copy forward byte by byte
			for i := 0; i < ml; i++ {
				dst[d+i] = dst[m+i]
			}
		}
		d += ml
	}
	if d != len(dst) {
		return errors.New("lz: block length mismatch")
	}
	return nil
}

func parallelFor(n, workers int, f func(i int)) {
	if workers < 1 {
		workers = 1
	}
	var wg sync.WaitGroup
	ch := make(chan int, n)
	for i := 0; i < n; i++ {
		ch <- i
	}
	close(ch)
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for i := range ch {
				f(i)
			}
		}()
	}
	wg.Wait()
}

const headerBytes = 16

// Pack compresses data in independent blocks, in parallel.
func Pack(data []byte, workers int) []byte {
	var out bytes.Buffer
	out.Grow(headerBytes + len(data)/3)
	_ = PackTo(&out, data, workers) // a bytes.Buffer never fails
	return out.Bytes()
}

// PackTo compresses data in independent blocks, in parallel, and writes the
// container to w block by block: only the compressed blocks live in memory,
// never a second copy of the container.
func PackTo(w io.Writer, data []byte, workers int) error {
	n := (len(data) + BlockSize - 1) / BlockSize
	blocks := make([][]byte, n)
	parallelFor(n, workers, func(i int) {
		blocks[i] = compressBlock(data[i*BlockSize:min(len(data), (i+1)*BlockSize)], Depth)
	})
	var header [headerBytes]byte
	binary.LittleEndian.PutUint64(header[0:], uint64(len(data)))
	binary.LittleEndian.PutUint32(header[8:], BlockSize)
	binary.LittleEndian.PutUint32(header[12:], uint32(n))
	if _, err := w.Write(header[:]); err != nil {
		return err
	}
	var length [4]byte
	for _, b := range blocks {
		binary.LittleEndian.PutUint32(length[:], uint32(len(b)))
		if _, err := w.Write(length[:]); err != nil {
			return err
		}
		if _, err := w.Write(b); err != nil {
			return err
		}
	}
	return nil
}

// RawLen returns the decompressed length stored in a container.
func RawLen(buf []byte) (int, error) {
	if len(buf) < headerBytes {
		return 0, errors.New("lz: short header")
	}
	return int(binary.LittleEndian.Uint64(buf)), nil
}

// Unpack is the inverse of Pack.
func Unpack(buf []byte, workers int) ([]byte, error) {
	total, err := RawLen(buf)
	if err != nil {
		return nil, err
	}
	dst := make([]byte, total)
	return dst, UnpackInto(dst, buf, workers)
}

// UnpackInto decompresses into dst, which must have exactly the raw length.
func UnpackInto(dst, buf []byte, workers int) error {
	total, err := RawLen(buf)
	if err != nil {
		return err
	}
	block, n := int(binary.LittleEndian.Uint32(buf[8:])), int(binary.LittleEndian.Uint32(buf[12:]))
	if total != len(dst) || block <= 0 || n != (total+block-1)/block {
		return errors.New("lz: bad header")
	}
	blocks := make([][]byte, n)
	p := headerBytes
	for i := range blocks {
		if p+4 > len(buf) {
			return errors.New("lz: truncated")
		}
		l := int(binary.LittleEndian.Uint32(buf[p:]))
		p += 4
		if p+l > len(buf) {
			return errors.New("lz: truncated")
		}
		blocks[i] = buf[p : p+l]
		p += l
	}
	errs := make([]error, n)
	parallelFor(n, workers, func(i int) {
		errs[i] = decompressBlock(dst[i*block:min(total, (i+1)*block)], blocks[i])
	})
	for _, e := range errs {
		if e != nil {
			return e
		}
	}
	return nil
}

// PackIndexedTo compresses data in independent blocks of blockSize bytes,
// in parallel, and writes the indexed container to w (see the package
// comment): only the compressed blocks live in memory.
func PackIndexedTo(w io.Writer, data []byte, blockSize, workers int) error {
	if blockSize <= 0 {
		return errors.New("lz: bad block size")
	}
	n := (len(data) + blockSize - 1) / blockSize
	blocks := make([][]byte, n)
	parallelFor(n, workers, func(i int) {
		blocks[i] = compressBlock(data[i*blockSize:min(len(data), (i+1)*blockSize)], Depth)
	})
	return writeIndexed(w, len(data), blockSize, blocks)
}

// PackIndexedWith is PackIndexedTo for data that does not exist as one
// slice: fill(off, dst) writes the raw bytes [off, off+len(dst)) of the
// length bytes into dst, one block at a time (a wide table's planes are built
// from its two-byte entries this way, without a copy of the table). Same
// container.
func PackIndexedWith(w io.Writer, length, blockSize, workers int, fill func(off int, dst []byte)) error {
	if blockSize <= 0 {
		return errors.New("lz: bad block size")
	}
	n := (length + blockSize - 1) / blockSize
	blocks := make([][]byte, n)
	parallelFor(n, workers, func(i int) {
		raw := make([]byte, min(length, (i+1)*blockSize)-i*blockSize)
		fill(i*blockSize, raw)
		blocks[i] = compressBlock(raw, Depth)
	})
	return writeIndexed(w, length, blockSize, blocks)
}

// writeIndexed writes the header, the offsets and the compressed blocks of an
// indexed container.
func writeIndexed(w io.Writer, length, blockSize int, blocks [][]byte) error {
	n := len(blocks)
	var header [headerBytes]byte
	binary.LittleEndian.PutUint64(header[0:], uint64(length))
	binary.LittleEndian.PutUint32(header[8:], uint32(blockSize))
	binary.LittleEndian.PutUint32(header[12:], uint32(n))
	if _, err := w.Write(header[:]); err != nil {
		return err
	}
	offsets := make([]byte, 8*(n+1))
	var off uint64
	for i, b := range blocks {
		binary.LittleEndian.PutUint64(offsets[8*i:], off)
		off += uint64(len(b))
	}
	binary.LittleEndian.PutUint64(offsets[8*n:], off)
	if _, err := w.Write(offsets); err != nil {
		return err
	}
	for _, b := range blocks {
		if _, err := w.Write(b); err != nil {
			return err
		}
	}
	return nil
}

// Index is the table of contents of an indexed container that starts at
// some offset of a file: enough to read and decode single blocks.
type Index struct {
	RawLen    int
	BlockSize int
	offsets   []uint64 // len Blocks()+1, relative to dataStart
	dataStart int64    // file offset of the first compressed block
	maxPacked int      // the largest compressed block: the scratch size ReadBlock needs
}

// readAt fills buf from r at off; an io.EOF that completes the buffer is fine.
func readAt(r io.ReaderAt, buf []byte, off int64) error {
	n, err := r.ReadAt(buf, off)
	if err == io.EOF && n == len(buf) {
		return nil
	}
	return err
}

// ReadIndex reads the header and the offsets of the indexed container at
// base; the blocks stay in the file.
func ReadIndex(r io.ReaderAt, base int64) (*Index, error) {
	var header [headerBytes]byte
	if err := readAt(r, header[:], base); err != nil {
		return nil, err
	}
	ix := &Index{RawLen: int(binary.LittleEndian.Uint64(header[0:])), BlockSize: int(binary.LittleEndian.Uint32(header[8:]))}
	n := int(binary.LittleEndian.Uint32(header[12:]))
	if ix.BlockSize <= 0 || n != (ix.RawLen+ix.BlockSize-1)/ix.BlockSize {
		return nil, errors.New("lz: bad header")
	}
	buf := make([]byte, 8*(n+1))
	if err := readAt(r, buf, base+headerBytes); err != nil {
		return nil, err
	}
	ix.offsets = make([]uint64, n+1)
	for i := range ix.offsets {
		ix.offsets[i] = binary.LittleEndian.Uint64(buf[8*i:])
		if i > 0 {
			if ix.offsets[i] < ix.offsets[i-1] {
				return nil, errors.New("lz: bad offsets")
			}
			ix.maxPacked = max(ix.maxPacked, int(ix.offsets[i]-ix.offsets[i-1]))
		}
	}
	ix.dataStart = base + headerBytes + int64(len(buf))
	return ix, nil
}

// Blocks is the number of blocks.
func (ix *Index) Blocks() int { return len(ix.offsets) - 1 }

// MaxPacked is the size of the largest compressed block (the scratch buffer
// ReadBlock needs).
func (ix *Index) MaxPacked() int { return ix.maxPacked }

// PackedLen is the total size of the compressed blocks.
func (ix *Index) PackedLen() int64 { return int64(ix.offsets[ix.Blocks()]) }

// BlockLen is the raw length of block i (only the last block can be shorter
// than BlockSize).
func (ix *Index) BlockLen(i int) int { return min(ix.BlockSize, ix.RawLen-i*ix.BlockSize) }

// ReadBlock reads block i from r and decodes it into dst (at least
// BlockLen(i) bytes) using scratch (at least MaxPacked bytes).
func (ix *Index) ReadBlock(r io.ReaderAt, i int, scratch, dst []byte) error {
	a, b := ix.offsets[i], ix.offsets[i+1]
	src := scratch[:b-a]
	if err := readAt(r, src, ix.dataStart+int64(a)); err != nil {
		return err
	}
	return decompressBlock(dst[:ix.BlockLen(i)], src)
}

// UnpackEach reads every block and hands each decoded block to sink(off,
// data) (its raw offset and bytes), blocks in parallel: the counterpart of
// PackIndexedWith for data that is not kept as one slice.
func (ix *Index) UnpackEach(r io.ReaderAt, workers int, sink func(off int, data []byte)) error {
	packed := make([]byte, ix.PackedLen())
	if err := readAt(r, packed, ix.dataStart); err != nil {
		return err
	}
	n := ix.Blocks()
	errs := make([]error, n)
	parallelFor(n, workers, func(i int) {
		raw := make([]byte, ix.BlockLen(i))
		if errs[i] = decompressBlock(raw, packed[ix.offsets[i]:ix.offsets[i+1]]); errs[i] == nil {
			sink(i*ix.BlockSize, raw)
		}
	})
	for _, e := range errs {
		if e != nil {
			return e
		}
	}
	return nil
}

// UnpackAll reads every block and decodes the whole container into dst
// (exactly RawLen bytes), blocks in parallel.
func (ix *Index) UnpackAll(r io.ReaderAt, dst []byte, workers int) error {
	if len(dst) != ix.RawLen {
		return errors.New("lz: bad length")
	}
	packed := make([]byte, ix.PackedLen())
	if err := readAt(r, packed, ix.dataStart); err != nil {
		return err
	}
	n := ix.Blocks()
	errs := make([]error, n)
	parallelFor(n, workers, func(i int) {
		start := i * ix.BlockSize
		errs[i] = decompressBlock(dst[start:start+ix.BlockLen(i)], packed[ix.offsets[i]:ix.offsets[i+1]])
	})
	for _, e := range errs {
		if e != nil {
			return e
		}
	}
	return nil
}
