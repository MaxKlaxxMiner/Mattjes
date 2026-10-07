// Package matepn is the proof-number mate search, variant df-pn (Nagai 2002):
// depth-first, with proof and disproof numbers kept in the transposition
// table, so the table is the tree. Working title from the design document
// (docs/m4-mate-search-design.md, section 3.2).
//
// Proof number pn = how many leaves must still be proven for the node to be
// proven (a forced mate for the attacker), disproof number dn = how many must
// be disproven. OR node (attacker to move): pn = min over children, dn = sum.
// AND node (defender to move): pn = sum, dn = min. The search always descends
// into the child where the proof looks cheapest and stops as soon as the
// node's numbers exceed the thresholds handed down by the parent, which are
// derived from the second-best sibling: that is what makes it depth-first and
// still best-first.
//
// The proof goal is "mate in at most N plies". The remaining depth is part of
// the table key (like perftTT), so every entry is path-independent: a
// refutation at the horizon is a refutation for that depth, nothing else. The
// unbounded variant (any mate, cycles cut by path repetition) was measured
// first: it proves KRR-K in 0.26 s but is lost in perpetual checks on KQ-KN,
// because check sequences have tiny proof numbers and only end at the depth
// bound, whose refutations then poison the table (see m4-mate-search.md).
package matepn

import (
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mateab"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/tt"
)

// Table is what the search needs from a transposition table (tt.Table and
// tt.Buckets qualify). df-pn needs one, there is no "without".
type Table interface {
	Probe(k tt.Key) (uint64, bool)
	Store(k tt.Key, value uint64)
}

// MaxPlies is the deepest proof goal (the depth salt table size).
const MaxPlies = 127

// Result of a search.
type Result struct {
	Proven    bool // a forced mate within the depth exists
	Disproven bool // no forced mate within the depth
	// MatePlies is the length of the mate along the proof tree in the table;
	// with a single depth it is an upper bound of the shortest mate, with
	// iterative deepening it is the shortest. 0 when the tree is incomplete
	// (entries overwritten).
	MatePlies int
	PV        []chess.Move
	Nodes     uint64 // node visits (each regenerates its moves)
	Leaves    uint64 // children seen for the first time (no table entry)
	TTHits    uint64 // children found in the table
	FinalHits uint64 // children decided by a depth-free final entry
}

// Searcher holds the search state. Progress, if set, is called every
// ProgressNodes node visits.
type Searcher struct {
	oracle   mateab.Oracle
	table    Table
	codec    Codec
	mobility bool
	// Epsilon widens the child's threshold from second+1 to second*(1+epsilon)
	// (Pawlewicz & Lew 2007): the search returns to the parent less often and
	// re-expands fewer nodes, at the price of a less strictly best-first order.
	// Given in eighths: 0 = off, 1 = 1/8, 4 = 1/2, 8 = 1, 16 = 2.
	Epsilon int
	// Final reuses proven and disproven results across depths: a position
	// proven within d plies is proven within any d' >= d, one disproven within
	// d is disproven within any d' <= d. Stored under the unsalted key as the
	// smallest proven and the largest disproven depth.
	Final    bool
	Progress func(nodes uint64)

	nodes, leaves, ttHits, finalHits uint64
}

// ProgressNodes is the interval of Progress calls (a power of two).
const ProgressNodes = 1 << 22

// New returns a searcher. mobility initialises unknown children with their
// number of legal moves instead of 1 (one generator call per new leaf).
func New(oracle mateab.Oracle, table Table, codec Codec, mobility bool) *Searcher {
	return &Searcher{oracle: oracle, table: table, codec: codec, mobility: mobility}
}

// depthSalt is XORed into both key words, so (position, remaining depth) is
// simply another 128-bit key. Same generator as perftTT.
var depthSalt = func() (s [MaxPlies + 1]tt.Key) {
	x := uint64(0x9E3779B97F4A7C15)
	for i := range s {
		for w := range s[i] {
			x ^= x << 13
			x ^= x >> 7
			x ^= x << 17
			s[i][w] = x
		}
	}
	return
}()

func key(b *bitboard.Board, depth int) tt.Key {
	return tt.Key{b.Key[0] ^ depthSalt[depth][0], b.Key[1] ^ depthSalt[depth][1]}
}

type child struct {
	move   chess.Move
	key    tt.Key
	pn, dn uint32
}

// Solve proves or disproves "the side to move mates within plies" (odd).
func (s *Searcher) Solve(root *bitboard.Board, plies int) Result {
	if plies < 1 || plies > MaxPlies || plies%2 == 0 {
		panic("matepn: plies must be odd and at most MaxPlies")
	}
	s.nodes, s.leaves, s.ttHits, s.finalHits = 0, 0, 0, 0
	pn, dn := s.mid(root, Inf, Inf, plies)
	r := Result{Proven: pn == 0, Disproven: dn == 0, Nodes: s.nodes, Leaves: s.leaves, TTHits: s.ttHits, FinalHits: s.finalHits}
	if r.Proven {
		memo := map[tt.Key]int{}
		r.MatePlies = s.proofLength(root, plies, memo)
		if r.MatePlies > 0 {
			r.PV = s.proofLine(root, plies, memo)
		}
	}
	return r
}

// SolveShortest runs Solve with iterative deepening over odd depths up to
// maxPlies; report is called after every depth. The first proven depth is the
// shortest mate. Nodes accumulate.
func (s *Searcher) SolveShortest(root *bitboard.Board, maxPlies int, report func(plies int, r Result)) Result {
	var total Result
	for d := 1; d <= maxPlies && d <= MaxPlies; d += 2 {
		r := s.Solve(root, d)
		total.Nodes += r.Nodes
		total.Leaves += r.Leaves
		total.TTHits += r.TTHits
		total.FinalHits += r.FinalHits
		r.Nodes, r.Leaves, r.TTHits, r.FinalHits = total.Nodes, total.Leaves, total.TTHits, total.FinalHits
		if report != nil {
			report(d, r)
		}
		if r.Proven {
			return r
		}
		total.Disproven = r.Disproven
	}
	return total
}

// terminal evaluates a node without expanding it: oracle, no legal moves,
// horizon. Returns ok = false when the node must be expanded; n is then the
// number of legal moves in buf (only checks for the attacker at depth 1, since
// a mating move is always a check). Attacker nodes have odd depth.
func (s *Searcher) terminal(b *bitboard.Board, buf *chess.MoveBuffer, depth int) (pn, dn uint32, n int, ok bool) {
	attacker := depth%2 == 1
	v, plies := s.oracle.Probe(b)
	switch {
	case v == mateab.Draw, attacker && v == mateab.Loss, !attacker && v == mateab.Win:
		return Inf, 0, 0, true
	case attacker && v == mateab.Win, !attacker && v == mateab.Loss:
		if plies > 0 && plies > depth {
			return Inf, 0, 0, true // the mate exists but is too long for this depth
		}
		if plies > 0 {
			return 0, Inf, 0, true
		}
		// distance unknown: the search has to find it (not the case with endgame tables)
	}
	if attacker && depth == 1 {
		n = b.GenChecks(buf)
	} else {
		n = b.GenMoves(buf)
	}
	if n == 0 {
		if !attacker && b.InCheck() {
			return 0, Inf, 0, true // mate
		}
		return Inf, 0, 0, true // stalemate, no move, or no check at depth 1
	}
	if depth == 0 {
		return Inf, 0, n, true // horizon: the defender still has moves
	}
	return 0, 0, n, false
}

// mid is Nagai's "multiple iterative deepening": it searches below b until the
// node's proof number reaches thPn or its disproof number reaches thDn, and
// returns the numbers at that point.
func (s *Searcher) mid(b *bitboard.Board, thPn, thDn uint32, depth int) (uint32, uint32) {
	s.nodes++
	if s.nodes&(ProgressNodes-1) == 0 && s.Progress != nil {
		s.Progress(s.nodes)
	}
	var buf chess.MoveBuffer
	pn, dn, n, done := s.terminal(b, &buf, depth)
	if done {
		s.store(b, depth, pn, dn)
		return pn, dn
	}
	attacker := depth%2 == 1

	// expand: numbers of all children from the table or as fresh leaves
	var children [chess.MaxMoves]child
	for i := 0; i < n; i++ {
		c := *b
		c.DoMove(buf[i])
		children[i] = child{move: buf[i], key: key(&c, depth-1)}
		if pn, dn, ok := s.probeFinal(&c, depth-1); ok {
			s.finalHits++
			children[i].pn, children[i].dn = pn, dn
			continue
		}
		if v, ok := s.table.Probe(children[i].key); ok {
			s.ttHits++
			children[i].pn, children[i].dn = s.codec.Unpack(v)
			continue
		}
		s.leaves++
		children[i].pn, children[i].dn = s.leafNumbers(&c, depth-1)
	}

	for {
		// the node's numbers, the best child and the second-best number
		var best, second uint32 = Inf, Inf // best = min pn (attacker) or min dn (defender), second = runner-up
		var sum uint32
		bi := -1
		for i := 0; i < n; i++ {
			c := &children[i]
			k, other := c.pn, c.dn
			if !attacker {
				k, other = c.dn, c.pn
			}
			sum = satAdd(sum, other)
			if k < best || (k == best && bi >= 0 && other < otherOf(&children[bi], attacker)) {
				second = best
				best, bi = k, i
			} else if k < second {
				second = k
			}
		}
		if attacker {
			pn, dn = best, sum
		} else {
			pn, dn = sum, best
		}
		if pn >= thPn || dn >= thDn {
			s.store(b, depth, pn, dn)
			return pn, dn
		}
		// thresholds for the best child: it may grow until it stops being the
		// best (second + 1), and the other number until the node's threshold
		// would be reached through the sum
		c := &children[bi]
		bound := satAdd(second, 1)
		if s.Epsilon > 0 && second < Inf {
			bound = satAdd(second, max(1, uint32(uint64(second)*uint64(s.Epsilon)/8)))
		}
		var cThPn, cThDn uint32
		if attacker {
			cThPn = min(thPn, bound)
			cThDn = satSub(thDn, satSub(dn, c.dn))
		} else {
			cThDn = min(thDn, bound)
			cThPn = satSub(thPn, satSub(pn, c.pn))
		}
		cb := *b
		cb.DoMove(c.move)
		c.pn, c.dn = s.mid(&cb, cThPn, cThDn, depth-1)
	}
}

func otherOf(c *child, attacker bool) uint32 {
	if attacker {
		return c.dn
	}
	return c.pn
}

// leafNumbers initialises a child that has no table entry: 1/1, or with
// mobility the number of legal moves on the side that must prove all of them
// (defender: pn, attacker: dn). A terminal child is recognised right away.
func (s *Searcher) leafNumbers(c *bitboard.Board, depth int) (uint32, uint32) {
	if !s.mobility {
		return 1, 1
	}
	var buf chess.MoveBuffer
	pn, dn, n, done := s.terminal(c, &buf, depth)
	if done {
		return pn, dn
	}
	if depth%2 == 1 {
		return 1, uint32(n) // attacker: every move must be refuted to disprove
	}
	return uint32(n), 1 // defender: every move must be answered to prove
}

func (s *Searcher) store(b *bitboard.Board, depth int, pn, dn uint32) {
	s.table.Store(key(b, depth), s.codec.Pack(pn, dn))
	if s.Final && (pn == 0 || dn == 0) {
		s.storeFinal(b, depth, pn == 0)
	}
}

// Final entries live under the unsalted key: 7 bits smallest proven depth + 1
// and 7 bits largest disproven depth + 1 (0 = none each).
func unpackFinal(v uint64) (proven, disproven int) {
	return int(v>>7&127) - 1, int(v&127) - 1
}

func (s *Searcher) storeFinal(b *bitboard.Board, depth int, proven bool) {
	pd, dd := -1, -1
	if v, ok := s.table.Probe(b.Key); ok {
		pd, dd = unpackFinal(v)
	}
	if proven {
		if pd < 0 || depth < pd {
			pd = depth
		}
	} else if depth > dd {
		dd = depth
	}
	s.table.Store(b.Key, uint64(pd+1)<<7|uint64(dd+1))
}

// probeFinal answers from the depth-free entry when it decides the node at
// this depth.
func (s *Searcher) probeFinal(b *bitboard.Board, depth int) (pn, dn uint32, ok bool) {
	if !s.Final {
		return 0, 0, false
	}
	v, found := s.table.Probe(b.Key)
	if !found {
		return 0, 0, false
	}
	pd, dd := unpackFinal(v)
	switch {
	case pd >= 0 && pd <= depth:
		return 0, Inf, true
	case dd >= depth:
		return Inf, 0, true
	}
	return 0, 0, false
}

func satAdd(a, b uint32) uint32 {
	if a >= Inf || b >= Inf || a+b >= Inf {
		return Inf
	}
	return a + b
}

func satSub(a, b uint32) uint32 {
	if a >= Inf {
		return Inf
	}
	if b >= a {
		return 0
	}
	return a - b
}

// proofLength walks the proof tree in the table: at an attacker node the
// shortest proven child, at a defender node the longest. Returns -1 when a
// needed entry is missing (overwritten). memo is keyed by the salted key.
func (s *Searcher) proofLength(b *bitboard.Board, depth int, memo map[tt.Key]int) int {
	k := key(b, depth)
	if l, ok := memo[k]; ok {
		return l
	}
	attacker := depth%2 == 1
	if v, plies := s.oracle.Probe(b); plies > 0 && ((attacker && v == mateab.Win) || (!attacker && v == mateab.Loss)) {
		memo[k] = plies // the oracle (endgame table) ended the line
		return plies
	}
	var buf chess.MoveBuffer
	n := b.GenMoves(&buf)
	if n == 0 {
		if !attacker && b.InCheck() {
			memo[k] = 0
			return 0
		}
		return -1
	}
	if depth == 0 {
		return -1
	}
	result := -1
	for i := 0; i < n; i++ {
		c := *b
		c.DoMove(buf[i])
		// proven when the table says so (salted or final entry) or when the
		// child is terminal itself (mate, oracle): terminal children found as
		// fresh leaves were never visited and have no entry
		proven := false
		if pn, _, ok := s.probeFinal(&c, depth-1); ok {
			proven = pn == 0
		} else if v, ok := s.table.Probe(key(&c, depth-1)); ok {
			pn, _ := s.codec.Unpack(v)
			proven = pn == 0
		} else {
			var cbuf chess.MoveBuffer
			pn, _, _, done := s.terminal(&c, &cbuf, depth-1)
			proven = done && pn == 0
		}
		l := -1
		if proven {
			l = s.proofLength(&c, depth-1, memo)
		}
		if l < 0 {
			if attacker {
				continue
			}
			return -1
		}
		if attacker {
			if result < 0 || l+1 < result {
				result = l + 1
			}
		} else {
			result = max(result, l+1)
		}
	}
	if result >= 0 {
		memo[k] = result
	}
	return result
}

// proofLine follows the lengths computed by proofLength.
func (s *Searcher) proofLine(root *bitboard.Board, depth int, memo map[tt.Key]int) []chess.Move {
	var pv []chess.Move
	b := *root
	for {
		want, ok := memo[key(&b, depth)]
		if !ok || want <= 0 {
			return pv
		}
		var buf chess.MoveBuffer
		n := b.GenMoves(&buf)
		found := false
		for i := 0; i < n; i++ {
			c := b
			c.DoMove(buf[i])
			if l, ok := memo[key(&c, depth-1)]; ok && l == want-1 {
				pv = append(pv, buf[i])
				b = c
				depth--
				found = true
				break
			}
		}
		if !found {
			return pv
		}
	}
}
