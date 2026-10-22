package syzygy

import (
	"encoding/binary"
	"fmt"
)

// The compressed value stream of a Syzygy table (a port of Fathom's
// setup_pairs/decompress_pairs): values are run-length "pairs" encoded as
// a canonical Huffman code over symbols, where a symbol expands
// recursively into two others (symPat) down to single values; blocks of
// 2^blockSize bytes hold the code, an index table maps every 2^idxBits-th
// position to its block and offset, a size table gives the number of
// positions per block.

type pairsData struct {
	indexTable []byte   // 6 bytes per index entry: block (u32) and offset (u16)
	sizeTable  []byte   // u16 per block: positions in the block minus one
	data       []byte   // the code blocks
	offset     []byte   // u16 per code length (from minLen): first symbol of that length
	symLen     []uint8  // per symbol: positions it expands to minus one
	symPat     []byte   // 3 bytes per symbol: two sub-symbols, or the value
	base       []uint64 // per code length (from minLen): first code, left-aligned in 64 bits
	blockSize  uint8
	idxBits    uint8
	minLen     uint8
	constValue [2]uint8 // the whole table has one value (idxBits 0)
	size       uint64   // positions in the table (the index range)
	realBlocks int      // code blocks in the file (the size table may list padding blocks)
}

func (d *pairsData) calcSymLen(s uint32, tmp []bool) {
	w := d.symPat[3*s : 3*s+3]
	s2 := uint32(w[2])<<4 | uint32(w[1])>>4
	if s2 == 0x0fff {
		d.symLen[s] = 0
	} else {
		s1 := (uint32(w[1])&0xf)<<8 | uint32(w[0])
		if !tmp[s1] {
			d.calcSymLen(s1, tmp)
		}
		if !tmp[s2] {
			d.calcSymLen(s2, tmp)
		}
		d.symLen[s] = d.symLen[s1] + d.symLen[s2] + 1
	}
	tmp[s] = true
}

// setupPairs reads the pairs header at data[pos:] for a table of tbSize
// positions; returns the pairs data, the position after the header and
// the byte sizes of the index table, the size table and the code blocks.
func setupPairs(data []byte, pos int, tbSize uint64) (*pairsData, int, [3]int) {
	d := &pairsData{size: tbSize}
	var sizes [3]int
	flags := data[pos]
	if flags&0x80 != 0 {
		d.idxBits = 0
		d.constValue[0] = data[pos+1]
		return d, pos + 2, sizes
	}
	d.blockSize = data[pos+1]
	d.idxBits = data[pos+2]
	realNumBlocks := binary.LittleEndian.Uint32(data[pos+4:])
	d.realBlocks = int(realNumBlocks)
	numBlocks := realNumBlocks + uint32(data[pos+3])
	maxLen := int(data[pos+8])
	minLen := int(data[pos+9])
	h := maxLen - minLen + 1
	numSyms := int(binary.LittleEndian.Uint16(data[pos+10+2*h:]))
	d.offset = data[pos+10 : pos+10+2*h]
	d.symLen = make([]uint8, numSyms)
	d.symPat = data[pos+12+2*h : pos+12+2*h+3*numSyms]
	d.minLen = uint8(minLen)
	next := pos + 12 + 2*h + 3*numSyms + (numSyms & 1)

	numIndices := (tbSize + (uint64(1) << d.idxBits) - 1) >> d.idxBits
	sizes[0] = 6 * int(numIndices)
	sizes[1] = 2 * int(numBlocks)
	sizes[2] = int(realNumBlocks) << d.blockSize

	tmp := make([]bool, numSyms)
	for s := 0; s < numSyms; s++ {
		if !tmp[s] {
			d.calcSymLen(uint32(s), tmp)
		}
	}
	d.base = make([]uint64, h)
	d.base[h-1] = 0
	for i := h - 2; i >= 0; i-- {
		d.base[i] = (d.base[i+1] + uint64(binary.LittleEndian.Uint16(d.offset[2*i:])) - uint64(binary.LittleEndian.Uint16(d.offset[2*i+2:]))) / 2
	}
	for i := 0; i < h; i++ {
		d.base[i] <<= 64 - uint(minLen+i)
	}
	return d, next, sizes
}

// decompress returns the three value bytes of the symbol at position idx
// (the value is the first byte).
func (d *pairsData) decompress(idx uint64) []byte {
	if d.idxBits == 0 {
		return d.constValue[:]
	}
	if idx >= d.size {
		panic(fmt.Sprintf("syzygy: index %d beyond the table size %d (an encoding bug)", idx, d.size))
	}
	mainIdx := idx >> d.idxBits
	litIdx := int(idx&((uint64(1)<<d.idxBits)-1)) - (1 << (d.idxBits - 1))
	block := binary.LittleEndian.Uint32(d.indexTable[6*mainIdx:])
	litIdx += int(binary.LittleEndian.Uint16(d.indexTable[6*mainIdx+4:]))
	sizeAt := func(b uint32) int { return int(binary.LittleEndian.Uint16(d.sizeTable[2*b:])) }
	if litIdx < 0 {
		for litIdx < 0 {
			block--
			litIdx += sizeAt(block) + 1
		}
	} else {
		for litIdx > sizeAt(block) {
			litIdx -= sizeAt(block) + 1
			block++
		}
	}
	if int(block) >= d.realBlocks {
		panic(fmt.Sprintf("syzygy: block %d of %d real blocks (%d with padding), index entry %d, litIdx %d, idxBits %d", block, d.realBlocks, len(d.sizeTable)/2, mainIdx, litIdx, d.idxBits))
	}
	// the decoder reads ahead up to four bytes past the block, like Fathom
	// (the data slice runs to the end of the file, which has spare bytes)
	ptr := d.data[uint64(block)<<d.blockSize:]
	m := int(d.minLen)
	code := binary.BigEndian.Uint64(ptr)
	ptr = ptr[8:]
	bitCnt := 0 // empty bits in code
	var sym uint32
	for {
		l := m
		for code < d.base[l-m] {
			l++
		}
		sym = uint32(binary.LittleEndian.Uint16(d.offset[2*(l-m):]))
		sym += uint32((code - d.base[l-m]) >> (64 - uint(l)))
		if litIdx < int(d.symLen[sym])+1 {
			break
		}
		litIdx -= int(d.symLen[sym]) + 1
		code <<= uint(l)
		bitCnt += l
		if bitCnt >= 32 {
			bitCnt -= 32
			var tmp uint32
			if len(ptr) >= 4 {
				tmp = binary.BigEndian.Uint32(ptr)
				ptr = ptr[4:]
			} else {
				// the very end of the file: the bits are never used, pad
				for i := 0; i < len(ptr); i++ {
					tmp |= uint32(ptr[i]) << (24 - 8*uint(i))
				}
				ptr = ptr[len(ptr):]
			}
			code |= uint64(tmp) << uint(bitCnt)
		}
	}
	for d.symLen[sym] != 0 {
		w := d.symPat[3*sym : 3*sym+3]
		s1 := (uint32(w[1])&0xf)<<8 | uint32(w[0])
		if litIdx < int(d.symLen[s1])+1 {
			sym = s1
		} else {
			litIdx -= int(d.symLen[s1]) + 1
			sym = uint32(w[2])<<4 | uint32(w[1])>>4
		}
	}
	return d.symPat[3*sym : 3*sym+3]
}
