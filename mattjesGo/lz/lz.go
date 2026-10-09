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
