// Package wdlpn is the WDL search (milestone 7, docs/m7-wdl-search-design.md):
// df-pn without a depth bound that proves or disproves "the attacker forces
// a win", whatever the distance. Two questions per position, "the side to
// move wins" (the root is an attacker node) and "the opponent wins" (the root
// is a defender node); a win is proven when one of them is, a draw when both
// are disproven. Mate distance, fifty-move rule and tables are deliberately
// left out: the search looks for a guaranteed result with as few nodes and
// table entries as possible.
//
// Without a depth bound the game graph has cycles. They are cut by the path:
// a position that already occurred on the path from the root (same side to
// move) is a draw, the one repetition stands for the threefold one, because
// a defender who can force the return can force it again. A disproof that
// relies on such a repetition depends on the path (graph history
// interaction): it is valid only where the repeated ancestors are on the
// path. So every disproof carries the set of path plies it relies on (a
// depSet); a disproof that relies on nothing above its own node is
// path-independent and stored as usual, a conditional one is stored with
// the keys of the ancestors it needs (Searcher.deps, next to the table) and
// is used on another path only when all of them are on that path again
// (Kishimoto and Müller 2004, the dependency list of the general GHI
// solution). Proofs are finite trees and never depend on the path. A ply
// cap (MaxPly) is the safety net: a node beyond it is unknown, never a draw,
// and nothing about it is stored.
//
// Keys are the plain position keys, salted only by the attacker's colour, so
// that the two questions and the root driver's child searches share one
// table. The mechanics (mid, thresholds, prefetch, codec, priority classes)
// are those of matepn.
package wdlpn

import (
	"math/bits"
	"sync/atomic"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mateab"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/matepn"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/tt"
)

// Table is what the search needs from a transposition table (tt.Table and
// tt.Buckets qualify).
type Table interface {
	Probe(k tt.Key) (uint64, bool)
	Store(k tt.Key, value uint64)
	Prefetch(k tt.Key)
	ValueBits() int
}

// Codec packs pn/dn into the value bits; matepn's codecs are reused.
type Codec = matepn.Codec

// Inf is the infinite proof or disproof number (matepn.Inf).
const Inf = matepn.Inf

// PriorityBits are the top bits of every value: the class of the entry, so
// that a bucket table with tt.Buckets.SetPriorityBits never evicts a proof
// for an open estimate: 0 open, 1 conditionally disproven (valid with its
// ancestors on the path), 2 disproven, 3 proven.
const PriorityBits = 2

const (
	classOpen uint64 = iota
	classConditional
	classDisproven
	classProven
)

// MaxCodecBits caps the codec width like in matepn, so that the visits do
// not depend on the table size once nothing is evicted.
const MaxCodecBits = 20

// CodecBits is the width a codec for this table should use.
func CodecBits(t Table) int { return min(t.ValueBits()-PriorityBits, MaxCodecBits) }

// MaxPly is the ply cap of the search path (the root is ply 0).
const MaxPly = 255

// depSet is a set of path plies a disproof relies on (repeated ancestors).
type depSet [(MaxPly + 64) / 64]uint64

func (d *depSet) set(ply int)     { d[ply>>6] |= 1 << (uint(ply) & 63) }
func (d *depSet) union(o *depSet) { d[0] |= o[0]; d[1] |= o[1]; d[2] |= o[2]; d[3] |= o[3] }
func (d *depSet) empty() bool     { return d[0]|d[1]|d[2]|d[3] == 0 }
func (d *depSet) clearFrom(ply int) { // drops every ply >= ply
	for i := range d {
		switch lo := i << 6; {
		case lo >= ply:
			d[i] = 0
		case lo+64 > ply:
			d[i] &= 1<<(uint(ply)&63) - 1
		}
	}
}

// lowest is the shallowest ply in the set (MaxPly+1 when empty).
func (d *depSet) lowest() int {
	for i, w := range d {
		if w != 0 {
			return i<<6 + bits.TrailingZeros64(w)
		}
	}
	return MaxPly + 1
}

// Result of one question.
type Result struct {
	Proven    bool // the attacker forces a win
	Disproven bool // the defender holds at least a draw
	Aborted   bool // Stop was set, nothing decided
	PV        []chess.Move
	Nodes     uint64 // node visits
	Leaves    uint64 // children seen for the first time
	TTHits    uint64 // children found in the table
	CondHits  uint64 // conditional disproofs reused (their ancestors were on the path)
	CondMiss  uint64 // conditional disproofs found but unusable on this path
	Capped    uint64 // visits that hit MaxPly (unknown, not stored)
	CondSize  int    // conditional disproofs kept next to the table
	Pn, Dn    uint32 // the root's final numbers (0/Inf when decided)
}

// Searcher holds the search state.
type Searcher struct {
	oracle   mateab.Oracle
	table    Table
	codec    Codec
	mobility bool
	// Epsilon widens the child's threshold (in eighths, see matepn).
	Epsilon int
	// Progress, if set, is called every ProgressEvery visits (a power of two).
	Progress      func(nodes uint64)
	ProgressEvery uint64
	// Stop, if set, is polled every StopNodes visits.
	Stop    *atomic.Bool
	aborted bool

	attackerWhite bool
	scratch       bitboard.Board
	stack         [MaxPly + 2]bitboard.Board
	path          [MaxPly + 2]tt.Key // position keys of the current path, path[ply]
	// deps holds, per conditionally disproven position (table key), the keys
	// of the ancestors its disproof relies on
	deps      map[tt.Key][]tt.Key
	prioShift uint
	valueMask uint64

	nodes, leaves, ttHits, condHits, condMiss, capped uint64
}

// ProgressNodes is the default interval of Progress calls.
const ProgressNodes = 1 << 22

// StopNodes is the polling interval of Stop (a power of two).
const StopNodes = 1 << 12

// New returns a searcher. mobility initialises unknown children with their
// number of legal moves instead of 1.
func New(oracle mateab.Oracle, table Table, codec Codec, mobility bool) *Searcher {
	shift := uint(table.ValueBits() - PriorityBits)
	if codec.Pack(Inf, Inf)>>shift != 0 {
		panic("wdlpn: the codec leaves no room for the priority bits (use CodecBits)")
	}
	return &Searcher{oracle: oracle, table: table, codec: codec, mobility: mobility, ProgressEvery: ProgressNodes, prioShift: shift, valueMask: 1<<shift - 1, deps: map[tt.Key][]tt.Key{}}
}

// Clear forgets the conditional disproofs (call it together with the
// table's Clear).
func (s *Searcher) Clear() { s.deps = map[tt.Key][]tt.Key{} }

// Counters returns the running counters of the current question (for
// progress lines): leaves, table hits, conditional hits and misses, kept
// conditional disproofs, capped visits.
func (s *Searcher) Counters() (leaves, ttHits, condHits, condMiss, condSize, capped uint64) {
	return s.leaves, s.ttHits, s.condHits, s.condMiss, uint64(len(s.deps)), s.capped
}

// blackAttackerSalt distinguishes the two questions in one table.
var blackAttackerSalt = tt.Key{0x2545F4914F6CDD1D, 0x9E3779B97F4A7C15}

func (s *Searcher) key(b *bitboard.Board) tt.Key {
	if s.attackerWhite {
		return b.Key
	}
	return tt.Key{b.Key[0] ^ blackAttackerSalt[0], b.Key[1] ^ blackAttackerSalt[1]}
}

func (s *Searcher) attacker(b *bitboard.Board) bool { return b.WhiteMove == s.attackerWhite }

func (s *Searcher) probeOracle(b *bitboard.Board) mateab.Verdict {
	s.scratch = *b
	v, _, _ := s.oracle.Probe(&s.scratch)
	return v
}

type child struct {
	move   chess.Move
	key    tt.Key
	pn, dn uint32
	dep    depSet
}

// Solve answers one question: with sideToMoveAttacks the root is an attacker
// node ("the side to move forces a win"), otherwise a defender node ("the
// opponent forces a win").
func (s *Searcher) Solve(root *bitboard.Board, sideToMoveAttacks bool) Result {
	s.attackerWhite = root.WhiteMove == sideToMoveAttacks
	s.nodes, s.leaves, s.ttHits, s.condHits, s.condMiss, s.capped = 0, 0, 0, 0, 0, 0
	s.aborted = false
	pn, dn, _ := s.mid(root, Inf, Inf, 0)
	r := Result{Proven: pn == 0, Disproven: dn == 0, Nodes: s.nodes, Leaves: s.leaves, TTHits: s.ttHits, CondHits: s.condHits, CondMiss: s.condMiss, Capped: s.capped, CondSize: len(s.deps), Pn: pn, Dn: dn}
	if s.aborted {
		r.Proven, r.Disproven, r.Aborted = false, false, true
		return r
	}
	if r.Proven {
		r.PV = s.proofLine(root)
	}
	return r
}

func (s *Searcher) checkStop() {
	if s.nodes&(StopNodes-1) == 0 && s.Stop != nil && s.Stop.Load() {
		s.aborted = true
	}
}

// repetition looks for b on the path above ply (same side to move, back to
// the last irreversible move) and returns the ply of the repeated ancestor
// or -1.
func (s *Searcher) repetition(b *bitboard.Board, ply int) int {
	for i := ply - 2; i >= 0 && i >= ply-int(b.HalfmoveClock); i -= 2 {
		if s.path[i] == b.Key {
			return i
		}
	}
	return -1
}

// terminal evaluates a node without expanding it: repetition on the path,
// oracle, no legal moves. ok = false means the node must be expanded, n is
// then the number of legal moves in buf. dep holds the repeated ancestor.
func (s *Searcher) terminal(b *bitboard.Board, buf *chess.MoveBuffer, ply int) (pn, dn uint32, dep depSet, n int, ok bool) {
	if r := s.repetition(b, ply); r >= 0 {
		dep.set(r)
		return Inf, 0, dep, 0, true
	}
	attacker := s.attacker(b)
	switch s.probeOracle(b) {
	case mateab.Draw:
		return Inf, 0, dep, 0, true
	case mateab.Win:
		if attacker {
			return 0, Inf, dep, 0, true
		}
		return Inf, 0, dep, 0, true
	case mateab.Loss:
		if attacker {
			return Inf, 0, dep, 0, true
		}
		return 0, Inf, dep, 0, true
	}
	n = b.GenMoves(buf)
	if n == 0 {
		if !attacker && b.InCheck() {
			return 0, Inf, dep, 0, true // mate
		}
		return Inf, 0, dep, 0, true // stalemate, or the attacker is mated
	}
	return 0, 0, dep, n, false
}

// probeChild reads a child's table entry. A conditional disproof counts
// only when every ancestor it relies on is on the current path; the child
// then carries those plies as its dependency.
func (s *Searcher) probeChild(c *child, ply int) bool {
	v, ok := s.table.Probe(c.key)
	if !ok {
		return false
	}
	if v>>s.prioShift != classConditional {
		c.pn, c.dn = s.codec.Unpack(v & s.valueMask)
		return true
	}
	need, ok := s.deps[c.key]
	if !ok {
		return false // the list was dropped with a Clear, the entry is stale
	}
	var dep depSet
	for _, k := range need {
		found := false
		for i := ply; i >= 0; i-- { // ply is the parent's ply, the child's ancestors
			if s.path[i] == k {
				dep.set(i)
				found = true
				break
			}
		}
		if !found {
			s.condMiss++
			return false
		}
	}
	s.condHits++
	c.pn, c.dn, c.dep = Inf, 0, dep
	return true
}

// mid is Nagai's multiple iterative deepening (see matepn): it searches
// below b until pn reaches thPn or dn reaches thDn and returns the numbers
// plus the path plies a disproof relies on.
func (s *Searcher) mid(b *bitboard.Board, thPn, thDn uint32, ply int) (uint32, uint32, depSet) {
	var none depSet
	s.nodes++
	if s.nodes&(s.ProgressEvery-1) == 0 && s.Progress != nil {
		s.Progress(s.nodes)
	}
	s.checkStop()
	if s.aborted {
		return thPn, thDn, none
	}
	if ply >= MaxPly {
		s.capped++
		return thPn, thDn, none // unknown: the parent moves on, nothing is stored
	}
	s.path[ply] = b.Key
	var buf chess.MoveBuffer
	pn, dn, dep, n, done := s.terminal(b, &buf, ply)
	if done {
		s.store(b, pn, dn, &dep, ply)
		return pn, dn, dep
	}
	attacker := s.attacker(b)

	var children [chess.MaxMoves]child
	for i := 0; i < n; i++ {
		c := *b
		c.DoMove(buf[i])
		children[i] = child{move: buf[i], key: s.key(&c)}
		s.table.Prefetch(children[i].key)
	}
	for i := 0; i < n; i++ {
		if s.probeChild(&children[i], ply) {
			s.ttHits++
			continue
		}
		s.leaves++
		c := *b
		c.DoMove(children[i].move)
		children[i].pn, children[i].dn, children[i].dep = s.leafNumbers(&c, ply+1)
	}

	for {
		var best, second uint32 = Inf, Inf
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
			dep = none
			if dn == 0 {
				dep = disproofDep(children[:n], attacker)
				dep.clearFrom(ply) // a repetition of this node itself is resolved here
			}
			s.store(b, pn, dn, &dep, ply)
			return pn, dn, dep
		}
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
		cb := &s.stack[ply+1]
		*cb = *b
		cb.DoMove(c.move)
		c.pn, c.dn, c.dep = s.mid(cb, cThPn, cThDn, ply+1)
		if s.aborted {
			return pn, dn, none
		}
	}
}

// disproofDep collects what the node's disproof relies on: an attacker node
// needs all children disproven (the union of their dependencies), a
// defender node needs one and takes the least conditional one (no
// dependency, else the one whose shallowest ancestor is deepest).
func disproofDep(children []child, attacker bool) depSet {
	var dep depSet
	if attacker {
		for i := range children {
			dep.union(&children[i].dep)
		}
		return dep
	}
	bi, bestLow := -1, -1
	for i := range children {
		if children[i].dn != 0 {
			continue
		}
		if low := children[i].dep.lowest(); low > bestLow {
			bi, bestLow = i, low
		}
	}
	if bi >= 0 {
		dep = children[bi].dep
	}
	return dep
}

func otherOf(c *child, attacker bool) uint32 {
	if attacker {
		return c.dn
	}
	return c.pn
}

// leafNumbers initialises a child without a table entry: a terminal child is
// recognised right away, otherwise 1/1 or, with mobility, the number of
// legal moves on the side that must prove all of them.
func (s *Searcher) leafNumbers(c *bitboard.Board, ply int) (uint32, uint32, depSet) {
	var dep depSet
	if !s.mobility {
		// still catch repetitions, they are the whole point of the draw side
		if r := s.repetition(c, ply); r >= 0 {
			dep.set(r)
			return Inf, 0, dep
		}
		return 1, 1, dep
	}
	var buf chess.MoveBuffer
	pn, dn, dep, n, done := s.terminal(c, &buf, ply)
	if done {
		return pn, dn, dep
	}
	if s.attacker(c) {
		return 1, uint32(n), dep
	}
	return uint32(n), 1, dep
}

// store writes the node; a conditional disproof (dep not empty) goes in as
// classConditional with the keys of the ancestors it relies on.
func (s *Searcher) store(b *bitboard.Board, pn, dn uint32, dep *depSet, ply int) {
	k := s.key(b)
	class := classOpen
	switch {
	case pn == 0:
		class = classProven
	case dn == 0 && dep.empty():
		class = classDisproven
	case dn == 0:
		class = classConditional
		need := make([]tt.Key, 0, 2)
		for i := 0; i < ply; i++ {
			if dep[i>>6]>>(uint(i)&63)&1 != 0 {
				need = append(need, s.path[i])
			}
		}
		s.deps[k] = need
	}
	s.table.Store(k, s.codec.Pack(pn, dn)|class<<s.prioShift)
}

// satAdd adds two numbers; Inf is absorbing, finite sums stay finite (a sum
// that reached Inf would look like a proof or disproof: without a depth
// bound the DAG overcounting makes sums of tens of millions real).
func satAdd(a, b uint32) uint32 {
	if a >= Inf || b >= Inf {
		return Inf
	}
	return min(a+b, Inf-1)
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

// childProven reports whether a child position is proven: terminal by
// itself (mate, oracle) or proven in the table. terminalOnly restricts it
// to the terminal case.
func (s *Searcher) childProven(c *bitboard.Board, terminalOnly bool) bool {
	attacker := s.attacker(c)
	switch s.probeOracle(c) {
	case mateab.Win:
		return attacker
	case mateab.Loss:
		return !attacker
	case mateab.Draw:
		return false
	}
	if !attacker && !c.HasMoves() {
		return c.InCheck()
	}
	if terminalOnly {
		return false
	}
	if v, ok := s.table.Probe(s.key(c)); ok {
		return v>>s.prioShift == classProven
	}
	return false
}

// proofLine walks proven children from the root: the attacker prefers a
// terminal child (mate, won table position), the defender takes the first
// proven child; positions already on the line are skipped, so the walk is
// acyclic. It is a line of the proof, not the shortest.
func (s *Searcher) proofLine(root *bitboard.Board) []chess.Move {
	visited := map[tt.Key]bool{}
	var pv []chess.Move
	b := *root
	for len(pv) < MaxPly {
		visited[b.Key] = true
		if s.probeOracle(&b) != mateab.Unknown {
			return pv
		}
		var buf chess.MoveBuffer
		n := b.GenMoves(&buf)
		if n == 0 {
			return pv
		}
		chosen := -1
		for pass := 0; pass < 2 && chosen < 0; pass++ {
			for i := 0; i < n; i++ {
				c := b
				c.DoMove(buf[i])
				if visited[c.Key] {
					continue
				}
				if s.childProven(&c, pass == 0) {
					chosen = i
					break
				}
			}
		}
		if chosen < 0 {
			return pv
		}
		pv = append(pv, buf[chosen])
		b.DoMove(buf[chosen])
	}
	return pv
}
