//! pn/dn encodings for the table value bits, see `mattjesGo/matepn/codec.go`.

/// "Infinite" proof or disproof number inside the search: a proven node has
/// pn = 0, dn = INF, a disproven one pn = INF, dn = 0. Sums saturate at INF.
pub const INF: u32 = 1 << 30;

/// Packs a (pn, dn) pair into the value bits of a transposition table entry and
/// back. Only the order of the numbers matters for the search, so lossy
/// encodings are fine; 0 and INF must be exact, because they carry the proof.
pub trait Codec: Copy {
    fn pack(&self, pn: u32, dn: u32) -> u64;
    fn unpack(&self, v: u64) -> (u32, u32);
    fn name(&self) -> &'static str;
}

/// Each number in half the value bits, capped at the largest representable
/// number below the "infinite" code (all ones).
#[derive(Clone, Copy)]
pub struct Saturating {
    pub bits: u32,
}

impl Saturating {
    fn half(&self) -> u32 {
        self.bits / 2
    }

    fn pack_one(&self, x: u32) -> u64 {
        let all = (1u64 << self.half()) - 1;
        if x >= INF {
            all
        } else {
            (x as u64).min(all - 1)
        }
    }

    fn unpack_one(&self, v: u64) -> u32 {
        let all = (1u64 << self.half()) - 1;
        if v == all {
            INF
        } else {
            v as u32
        }
    }
}

impl Codec for Saturating {
    fn pack(&self, pn: u32, dn: u32) -> u64 {
        self.pack_one(pn) << self.half() | self.pack_one(dn)
    }

    fn unpack(&self, v: u64) -> (u32, u32) {
        let mask = (1u64 << self.half()) - 1;
        (self.unpack_one(v >> self.half()), self.unpack_one(v & mask))
    }

    fn name(&self) -> &'static str {
        "saturating"
    }
}

/// Each number as a small floating point value: 4 exponent bits and the rest
/// mantissa. Exact up to 2^mantissa, then rounded down; all ones is infinite.
#[derive(Clone, Copy)]
pub struct Float {
    pub bits: u32,
}

const FLOAT_EXP_BITS: u32 = 4;

impl Float {
    fn half(&self) -> u32 {
        self.bits / 2
    }

    fn mant_bits(&self) -> u32 {
        self.half() - FLOAT_EXP_BITS
    }

    fn pack_one(&self, x: u32) -> u64 {
        let all = (1u64 << self.half()) - 1;
        if x >= INF {
            return all;
        }
        let m = self.mant_bits();
        if x < 1 << m {
            return x as u64; // exponent 0: the number itself
        }
        // normalise: shift until the leading one sits at bit m, exponent = shift + 1
        let shift = (32 - x.leading_zeros()) - (m + 1);
        let e = (shift + 1) as u64;
        if e >= (1 << FLOAT_EXP_BITS) - 1 {
            return all - (1 << m); // largest finite code
        }
        let mant = ((x >> shift) as u64) & ((1 << m) - 1); // the leading one is implicit
        e << m | mant
    }

    fn unpack_one(&self, v: u64) -> u32 {
        let all = (1u64 << self.half()) - 1;
        if v == all {
            return INF;
        }
        let m = self.mant_bits();
        let e = v >> m;
        let mant = v & ((1 << m) - 1);
        if e == 0 {
            mant as u32
        } else {
            ((mant | 1 << m) << (e - 1)) as u32
        }
    }
}

impl Codec for Float {
    fn pack(&self, pn: u32, dn: u32) -> u64 {
        self.pack_one(pn) << self.half() | self.pack_one(dn)
    }

    fn unpack(&self, v: u64) -> (u32, u32) {
        let mask = (1u64 << self.half()) - 1;
        (self.unpack_one(v >> self.half()), self.unpack_one(v & mask))
    }

    fn name(&self) -> &'static str {
        "float"
    }
}
