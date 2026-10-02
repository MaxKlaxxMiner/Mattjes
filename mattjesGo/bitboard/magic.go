package bitboard

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// Magic bitboards: for a slider on sq, the relevant occupancy (blockers on its
// rays, edges excluded) is multiplied by a "magic" constant so that the top
// bits form a collision-free index into a precomputed attack table.
//
// The magics are searched at startup with a deterministic PRNG, which takes a
// few tens of milliseconds. See InitDuration.
type magic struct {
	mask    uint64
	mul     uint64
	shift   uint
	attacks []uint64
}

var rookMagics, bishopMagics [chess.FieldCount]magic
var attackTable []uint64 // all attack sets of all squares, sliced per magic

func rookAttacks(sq chess.Pos, occ uint64) uint64 {
	m := &rookMagics[sq]
	return m.attacks[((occ&m.mask)*m.mul)>>m.shift]
}

func bishopAttacks(sq chess.Pos, occ uint64) uint64 {
	m := &bishopMagics[sq]
	return m.attacks[((occ&m.mask)*m.mul)>>m.shift]
}

func queenAttacks(sq chess.Pos, occ uint64) uint64 {
	return rookAttacks(sq, occ) | bishopAttacks(sq, occ)
}

var rookDirs = [4][2]int{{0, -1}, {0, 1}, {-1, 0}, {1, 0}}
var bishopDirs = [4][2]int{{-1, -1}, {1, -1}, {-1, 1}, {1, 1}}

// slidingAttacksSlow walks the rays square by square. Only used to build the tables.
func slidingAttacksSlow(sq chess.Pos, occ uint64, dirs [4][2]int) uint64 {
	var att uint64
	for _, d := range dirs {
		for x, y := sq.X()+d[0], sq.Y()+d[1]; x >= 0 && x < chess.Width && y >= 0 && y < chess.Height; x, y = x+d[0], y+d[1] {
			t := bit(chess.PosFromXY(x, y))
			att |= t
			if occ&t != 0 {
				break
			}
		}
	}
	return att
}

// sliderMask is the attack set on an empty board without the last square of
// each ray: a blocker on the edge cannot hide anything behind it.
func sliderMask(sq chess.Pos, dirs [4][2]int) uint64 {
	var mask uint64
	for _, d := range dirs {
		x, y := sq.X()+d[0], sq.Y()+d[1]
		for {
			nx, ny := x+d[0], y+d[1]
			if nx < 0 || nx >= chess.Width || ny < 0 || ny >= chess.Height {
				break
			}
			mask |= bit(chess.PosFromXY(x, y))
			x, y = nx, ny
		}
	}
	return mask
}

// xorshift64* with a fixed seed: the same magics on every run.
type rng struct{ s uint64 }

func (r *rng) next() uint64 {
	r.s ^= r.s >> 12
	r.s ^= r.s << 25
	r.s ^= r.s >> 27
	return r.s * 2685821657736338717
}

// sparse numbers (few set bits) are far more likely to be valid magics
func (r *rng) sparse() uint64 { return r.next() & r.next() & r.next() }

func initMagics() {
	total := 0
	for sq := chess.Pos(0); sq < chess.FieldCount; sq++ {
		for _, t := range []struct {
			m    *magic
			dirs [4][2]int
		}{{&rookMagics[sq], rookDirs}, {&bishopMagics[sq], bishopDirs}} {
			t.m.mask = sliderMask(sq, t.dirs)
			t.m.shift = uint(64 - popcount(t.m.mask))
			total += 1 << popcount(t.m.mask)
		}
	}
	attackTable = make([]uint64, total)

	r := rng{s: 0x9E3779B97F4A7C15}
	offset := 0
	for sq := chess.Pos(0); sq < chess.FieldCount; sq++ {
		offset = findMagic(&rookMagics[sq], sq, rookDirs, offset, &r)
	}
	for sq := chess.Pos(0); sq < chess.FieldCount; sq++ {
		offset = findMagic(&bishopMagics[sq], sq, bishopDirs, offset, &r)
	}
}

// findMagic searches a multiplier for one square and fills its attack table.
// Returns the next free offset in attackTable.
func findMagic(m *magic, sq chess.Pos, dirs [4][2]int, offset int, r *rng) int {
	n := 1 << (64 - m.shift)
	m.attacks = attackTable[offset : offset+n]

	// all subsets of the mask ("carry-rippler" enumeration) and their reference attacks
	occs := make([]uint64, 0, n)
	refs := make([]uint64, 0, n)
	for occ := uint64(0); ; {
		occs = append(occs, occ)
		refs = append(refs, slidingAttacksSlow(sq, occ, dirs))
		occ = (occ - m.mask) & m.mask
		if occ == 0 {
			break
		}
	}

	used := make([]uint32, n) // epoch per slot, avoids clearing the table on every attempt
	for epoch := uint32(1); ; epoch++ {
		mul := r.sparse()
		if popcount((m.mask*mul)>>56) < 6 {
			continue
		}
		ok := true
		for i, occ := range occs {
			idx := (occ * mul) >> m.shift
			if used[idx] != epoch {
				used[idx] = epoch
				m.attacks[idx] = refs[i]
			} else if m.attacks[idx] != refs[i] {
				ok = false // destructive collision: two occupancies with different attacks
				break
			}
		}
		if ok {
			m.mul = mul
			return offset + n
		}
	}
}
