package syzygy

// The inverse of encode for one encoding: index -> squares. Needed to
// enumerate slices of a stream directly (the candidate orders of a large
// table are judged on sampled slices instead of being built completely).
// The index is a mixed-radix number: the leading group's value times its
// factor plus every other group's combination rank (colex order over the
// squares the earlier listed pieces leave free) times its factor. The
// leading group is inverted by a table enumerated once per shape, the
// other groups by colex unranking.

import (
	"fmt"
	"math/rand"
	"sync"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
)

// decoder inverts the index of one encoding (an order, a pawn file class).
type decoder struct {
	be       *baseEntry
	ei       *encInfo
	enc      int
	num      int
	leadK    int     // pieces of the leading group
	leadSize uint64  // its values
	lead     [][]int // per value: the canonical squares of the leading pieces
	groups   []decGroup
}

// decGroup is a group of like pieces after the leading group.
type decGroup struct {
	start, count int    // list positions
	factor, size uint64 // in the mixed radix
	lo, hi       int    // the squares it may use (0..64, or 8..56 for the other colour's pawns)
}

// leadTables caches the leading-group inversions per shape: the key is the
// group kind (kk, triple, or pawns with their count and file class).
var leadTables sync.Map

// newDecoder builds the decoder of an encoding; t is the file class of a
// pawn table.
func newDecoder(be *baseEntry, ei *encInfo, enc int, t int) *decoder {
	d := &decoder{be: be, ei: ei, enc: enc, num: int(be.num), leadK: int(ei.norm[0])}
	switch {
	case enc != encPiece:
		d.leadSize = pawnFactorFile[d.leadK-1][t]
		d.lead = leadTable(fmt.Sprintf("pawns%d/%d", d.leadK, t), func() [][]int { return enumeratePawnLead(be, d.leadK, t) })
	case be.kkEnc:
		d.leadSize = 462
		d.lead = leadTable("kk", func() [][]int { return enumeratePieceLead(be, 2) })
	default:
		d.leadSize = 31332
		d.lead = leadTable("triple", func() [][]int { return enumeratePieceLead(be, 3) })
	}
	k := d.leadK
	n := 64 - k
	if enc != encPiece && be.pawns[1] != 0 {
		count := int(ei.norm[k])
		d.groups = append(d.groups, decGroup{start: k, count: count, factor: ei.factor[k], size: subfactor(uint64(count), uint64(48-k)), lo: 8, hi: 56})
		k += count
		n = 64 - k
	}
	for k < d.num {
		count := int(ei.norm[k])
		d.groups = append(d.groups, decGroup{start: k, count: count, factor: ei.factor[k], size: subfactor(uint64(count), uint64(n)), lo: 0, hi: 64})
		n -= count
		k += count
	}
	return d
}

func leadTable(key string, build func() [][]int) [][]int {
	if v, ok := leadTables.Load(key); ok {
		return v.([][]int)
	}
	table := build()
	leadTables.Store(key, table)
	return table
}

// enumeratePieceLead inverts the leading group of a pawnless table (the two
// kings, or three single pieces): every placement is canonicalised as encode
// does it (mirrored by the first piece, transposed by the first off-diagonal
// piece) and the first placement of every value is kept.
func enumeratePieceLead(be *baseEntry, k int) [][]int {
	size := 31332
	if k == 2 {
		size = 462
	}
	table := make([][]int, size)
	var q [3]int
	var walk func(i int)
	walk = func(i int) {
		if i == k {
			c := q
			canonicalLead(c[:k], k)
			v := pieceLeadValue(c[:k], k)
			if v >= 0 && table[v] == nil {
				table[v] = append([]int(nil), c[:k]...)
			}
			return
		}
		for s := 0; s < 64; s++ {
			dup := false
			for j := 0; j < i; j++ {
				if q[j] == s {
					dup = true
				}
			}
			if !dup {
				q[i] = s
				walk(i + 1)
			}
		}
	}
	walk(0)
	for v, sq := range table {
		if sq == nil {
			panic(fmt.Sprintf("syzygy: leading value %d of %d has no placement", v, size))
		}
	}
	_ = be
	return table
}

// canonicalLead applies encode's mirrors and the diagonal flip to the
// leading squares alone (they decide the flips).
func canonicalLead(q []int, k int) {
	if q[0]&4 != 0 {
		for i := range q {
			q[i] ^= 7
		}
	}
	if q[0]&0x20 != 0 {
		for i := range q {
			q[i] ^= 0x38
		}
	}
	for i := 0; i < k; i++ {
		if offDiag[q[i]] != 0 {
			if offDiag[q[i]] > 0 {
				for j := range q {
					q[j] = int(flipDiag[q[j]])
				}
			}
			break
		}
	}
}

// pieceLeadValue is the leading group's value of canonical squares (encode's
// formulas), -1 for an impossible king pair.
func pieceLeadValue(p []int, k int) int {
	if k == 2 {
		return int(kkIdx[triangle[p[0]]][p[1]])
	}
	s1 := 0
	if p[1] > p[0] {
		s1 = 1
	}
	s2 := 0
	if p[2] > p[0] {
		s2++
	}
	if p[2] > p[1] {
		s2++
	}
	switch {
	case offDiag[p[0]] != 0:
		return int(triangle[p[0]])*63*62 + (p[1]-s1)*62 + (p[2] - s2)
	case offDiag[p[1]] != 0:
		return 6*63*62 + int(diag[p[0]])*28*62 + int(lower[p[1]])*62 + (p[2] - s2)
	case offDiag[p[2]] != 0:
		return 6*63*62 + 4*28*62 + int(diag[p[0]])*7*28 + (int(diag[p[1]])-s1)*28 + int(lower[p[2]])
	}
	return 6*63*62 + 4*28*62 + 4*7*28 + int(diag[p[0]])*7*6 + (int(diag[p[1]])-s1)*6 + (int(diag[p[2]]) - s2)
}

// enumeratePawnLead inverts the leading pawns of a pawn table for one file
// class: every placement is ordered as the probe orders it (the pawn with
// the smallest flap first, mirrored to the files a-d, the rest by twist)
// and the first placement of every value is kept.
func enumeratePawnLead(be *baseEntry, k int, t int) [][]int {
	size := int(pawnFactorFile[k-1][t])
	table := make([][]int, size)
	var q [6]int
	var walk func(i int)
	walk = func(i int) {
		if i == k {
			c := q
			p := c[:k]
			if leadingPawn(p, be, encFile) != t {
				return
			}
			if p[0]&4 != 0 {
				for j := range p {
					p[j] ^= 7
				}
			}
			for a := 1; a < k; a++ {
				for b := a + 1; b < k; b++ {
					if pawnTwist[0][p[a]] < pawnTwist[0][p[b]] {
						p[a], p[b] = p[b], p[a]
					}
				}
			}
			v := int(pawnIdx[0][k-1][flap[0][p[0]]])
			for a := 1; a < k; a++ {
				v += int(binomial[k-a][pawnTwist[0][p[a]]])
			}
			if table[v] == nil {
				table[v] = append([]int(nil), p...)
			}
			return
		}
		for s := 8; s < 56; s++ {
			dup := false
			for j := 0; j < i; j++ {
				if q[j] == s {
					dup = true
				}
			}
			if !dup {
				q[i] = s
				walk(i + 1)
			}
		}
	}
	walk(0)
	for v, sq := range table {
		if sq == nil {
			panic(fmt.Sprintf("syzygy: pawn leading value %d of %d (class %d) has no placement", v, size, t))
		}
	}
	return table
}

// CheckDecoder round-trips random indices of up to maxOrders candidate
// orders of a material (every pawn file class) through the decoder and
// encode; returns the indices checked and the mismatches (the first few are
// printed through report).
func CheckDecoder(m egtb.Material, maxOrders int, samples int, seed int64, report func(string)) (checked, bad int) {
	first, second := countsOf(m)
	be := newEntry(Name(first, second), first, second)
	rng := rand.New(rand.NewSource(seed))
	enc, files := encPiece, 1
	if be.hasPawns {
		enc, files = encFile, 4
	}
	cands := Candidates(be, first, second)
	if len(cands) > maxOrders {
		cands = cands[:maxOrders]
	}
	for _, o := range cands {
		for f := 0; f < files; f++ {
			var ei encInfo
			copy(ei.pieces[:], o.Pieces)
			size := ei.setup(be, o.Order, o.Order2, f, enc)
			d := newDecoder(be, &ei, enc, f)
			var p, q [maxPieces]int
			for s := 0; s < samples; s++ {
				idx := uint64(rng.Int63n(int64(size)))
				d.squares(idx, p[:be.num])
				q = p
				back := encode(q[:be.num], &ei, be, enc)
				checked++
				if back != idx {
					bad++
					if bad <= 5 && report != nil {
						report(fmt.Sprintf("  %s order %s file %d: index %d decodes to %v, encodes to %d", m.Name(), o, f, idx, p[:be.num], back))
					}
				}
			}
		}
	}
	return checked, bad
}

// squares fills p (Syzygy squares, in the list order) for an index.
func (d *decoder) squares(idx uint64, p []int) {
	lead := d.lead[(idx/d.ei.factor[0])%d.leadSize]
	copy(p, lead)
	for _, g := range d.groups {
		v := (idx / g.factor) % g.size
		// colex unranking: the i-th rank square (1-based, ascending) is the
		// largest r with C(r, i) <= v
		var ranks [6]int
		for i := g.count; i >= 1; i-- {
			r := i - 1
			for r+1 < 64 && binomial[i][r+1] <= v {
				r++
			}
			ranks[i-1] = r
			v -= binomial[i][r]
		}
		// the r-th square the earlier listed pieces leave free
		for i := 0; i < g.count; i++ {
			r := ranks[i]
			sq := g.lo
			for ; sq < g.hi; sq++ {
				taken := false
				for j := 0; j < g.start; j++ {
					if p[j] == sq {
						taken = true
						break
					}
				}
				if taken {
					continue
				}
				if r == 0 {
					break
				}
				r--
			}
			p[g.start+i] = sq
		}
	}
}
