package bitboard

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// Non-incremental keys for comparison with Zobrist.

// CRC64 start value and multiplier as used by yacboard and the old C# code
// (this is FNV-1a: xor the byte, then multiply by the FNV prime).
const (
	Crc64Start uint64 = 0xcbf29ce484222325
	Crc64Mul   uint64 = 0x100000001b3
)

// CRC64 hashes the 64 squares, side to move, castling rights and en passant
// square from scratch, exactly like yacboard's Checksum (without move counters).
func (b *Board) CRC64() uint64 {
	h := Crc64Start
	for _, p := range b.Squares {
		h = (h ^ uint64(p)) * Crc64Mul
	}
	side := uint64(0)
	if b.WhiteMove {
		side = 1
	}
	h = (h ^ side) * Crc64Mul
	h = (h ^ uint64(b.Castling)) * Crc64Mul
	h = (h ^ uint64(uint8(b.EnPassant))) * Crc64Mul
	return h
}

// ExactKey is a collision-free 32-byte identity of the position: the PackedFixed
// record with halfmove clock and move number zeroed.
type ExactKey [PackedFixedBytes]byte

func (b *Board) ExactKey() ExactKey {
	var k ExactKey
	tmp := b
	if b.HalfmoveClock != 0 || b.MoveNumber != 0 {
		c := *b
		c.HalfmoveClock, c.MoveNumber = 0, 0
		tmp = &c
	}
	tmp.AppendPackedFixed(k[:0])
	return k
}

// BoardFromExactKey restores the position (move counters are zero).
func BoardFromExactKey(k *ExactKey) Board {
	b, _ := DecodePackedFixed(k[:])
	return b
}

// Mix64 is a finalizer (splitmix64) that spreads the bits of x. Used to turn a
// key with poor low-bit distribution into a table index.
func Mix64(x uint64) uint64 {
	x ^= x >> 30
	x *= 0xbf58476d1ce4e5b9
	x ^= x >> 27
	x *= 0x94d049bb133111eb
	x ^= x >> 31
	return x
}

// ExactKeyHash folds the 32 exact bytes into 64 bits (for indexing a table whose
// entries hold the exact key for comparison).
func (k *ExactKey) Hash() uint64 {
	h := uint64(0)
	for i := 0; i < PackedFixedBytes; i += 8 {
		w := uint64(k[i]) | uint64(k[i+1])<<8 | uint64(k[i+2])<<16 | uint64(k[i+3])<<24 |
			uint64(k[i+4])<<32 | uint64(k[i+5])<<40 | uint64(k[i+6])<<48 | uint64(k[i+7])<<56
		h = Mix64(h ^ w)
	}
	return h
}

var _ = chess.None
