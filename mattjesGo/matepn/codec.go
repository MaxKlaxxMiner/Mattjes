package matepn

import "math/bits"

// Inf is "infinite" proof or disproof number inside the search: a proven node
// has pn = 0, dn = Inf, a disproven one pn = Inf, dn = 0. Sums saturate at Inf.
const Inf = 1 << 30

// Codec packs a (pn, dn) pair into the value bits of a transposition table
// entry and back. Only the order of the numbers matters for the search, the
// exact magnitude does not, so lossy encodings are fine; 0 and Inf must be
// exact, because they carry the proof.
type Codec interface {
	Pack(pn, dn uint32) uint64
	Unpack(v uint64) (pn, dn uint32)
	Name() string
}

// Saturating stores each number in half the value bits, capped at the largest
// representable number below the "infinite" code (all ones).
type Saturating struct{ Bits int }

func (c Saturating) half() int    { return c.Bits / 2 }
func (c Saturating) Name() string { return "saturating" }

func (c Saturating) packOne(x uint32) uint64 {
	all := uint64(1)<<uint(c.half()) - 1
	if x >= Inf {
		return all
	}
	return min(uint64(x), all-1)
}

func (c Saturating) unpackOne(v uint64) uint32 {
	all := uint64(1)<<uint(c.half()) - 1
	if v == all {
		return Inf
	}
	return uint32(v)
}

func (c Saturating) Pack(pn, dn uint32) uint64 {
	return c.packOne(pn)<<uint(c.half()) | c.packOne(dn)
}

func (c Saturating) Unpack(v uint64) (uint32, uint32) {
	mask := uint64(1)<<uint(c.half()) - 1
	return c.unpackOne(v >> uint(c.half())), c.unpackOne(v & mask)
}

// Float stores each number as a small floating point value: 4 exponent bits
// and the rest mantissa (8 at 24 value bits, 7 at 22). Exact up to 2^mantissa,
// then rounded down; all ones is infinite. Range with 8 mantissa bits: up to
// 511 << 14 = 8.4 million.
type Float struct{ Bits int }

const floatExpBits = 4

func (c Float) half() int     { return c.Bits / 2 }
func (c Float) mantBits() int { return c.half() - floatExpBits }
func (c Float) Name() string  { return "float" }

func (c Float) packOne(x uint32) uint64 {
	all := uint64(1)<<uint(c.half()) - 1
	if x >= Inf {
		return all
	}
	m := c.mantBits()
	if x < 1<<uint(m) {
		return uint64(x) // exponent 0: the number itself
	}
	// normalise: shift until the leading one sits at bit m, exponent = shift + 1
	shift := bits.Len32(x) - (m + 1)
	e := uint64(shift + 1)
	if e >= 1<<floatExpBits-1 {
		return all - 1<<uint(m) // largest finite code (exponent 14, mantissa all ones)
	}
	mant := uint64(x>>uint(shift)) & (1<<uint(m) - 1) // the leading one is implicit
	return e<<uint(m) | mant
}

func (c Float) unpackOne(v uint64) uint32 {
	all := uint64(1)<<uint(c.half()) - 1
	if v == all {
		return Inf
	}
	m := c.mantBits()
	e := v >> uint(m)
	mant := v & (1<<uint(m) - 1)
	if e == 0 {
		return uint32(mant)
	}
	return uint32((mant | 1<<uint(m)) << (e - 1))
}

func (c Float) Pack(pn, dn uint32) uint64 {
	return c.packOne(pn)<<uint(c.half()) | c.packOne(dn)
}

func (c Float) Unpack(v uint64) (uint32, uint32) {
	mask := uint64(1)<<uint(c.half()) - 1
	return c.unpackOne(v >> uint(c.half())), c.unpackOne(v & mask)
}
