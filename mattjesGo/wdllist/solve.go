// Package wdllist is the WDL search on an explicit graph (milestone 7, step
// 2, docs/m7-wdl-search.md): positions and edges are stored, wins are
// propagated backwards with counters as they appear, and a draw is the
// greatest fixed point "the defender always has a move that keeps the
// attacker away from a win", confirmed on the graph. Cycles cost nothing
// here: a cycle of positions that the attacker cannot win from is simply
// never marked, so there is no repetition detection, no graph history
// interaction and no re-expansion, which is where df-pn (package wdlpn)
// lost factor 30 to 40 on draws.
//
// The search is guided like df-pn: proof and disproof numbers per node,
// Nagai's descent with thresholds, the children taken from the graph
// instead of a transposition table. The numbers are heuristics only; what
// decides is exact: a node is won once a terminal win is reachable along
// stored edges (counters over all legal moves, so untried moves keep a node
// open), and safe once the fixed point says so. Safe and won are monotone,
// they persist as statuses. During the descent a child that lies on the
// current path counts as a draw (pn infinite, dn zero), which is what makes
// the defender head for repetitions; when the heuristic disproves the root,
// the fixed point confirms it or the numbers of the open nodes are reset.
//
// Two questions as in wdlpn: "the side to move wins" (the root is an
// attacker node) and "the opponent wins" (the root is a defender node);
// Win if one is proven, Draw if both are refuted. BruteForce expands the
// whole reachable graph instead, the plain reference.
package wdllist

import (
	"fmt"
	"math/bits"
	"sync/atomic"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mateab"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/ttstore"
)

// Outcome of one question.
type Outcome int8

const (
	Undecided Outcome = iota
	Proven            // the attacker forces a win
	Disproven         // the defender holds at least a draw (the root is safe)
)

// Result of one question.
type Result struct {
	Outcome    Outcome
	Method     string // which search decided: "pn", "certificate", "brute force"
	PV         []chess.Move
	WinPlies   int // length of the attacker's win along the graph levels (0 if not proven)
	Positions  int // unique positions in the graph
	Expansions int // positions whose children were added
	Edges      int // child edges
	Visits     uint64
	Resets     int    // heuristic disproofs the fixed point did not confirm
	SafeChecks int    // fixed-point runs
	Rounds     int    // widening rounds of the certificate builder
	Safe       int    // positions in the safe set reachable from the root (draw certificate) when disproven
	Won        int    // positions won for the attacker
	Exhausts   int    // subgraphs expanded by brute force because the numbers had saturated
	Chases     int    // certificate builder: check chains started (chase.go)
	ChaseNodes uint64 // nodes the chase visited
	Bytes      int    // bytes of the graph's arrays and store, by capacity (the memory accounting)
	Aborted    bool
	Stuck      bool // aborted because the descent made no progress (a bug, never a result)
}

// Options of the search.
type Options struct {
	// BruteForce expands the whole reachable graph, no guidance.
	BruteForce bool
	// NoRace runs the proof-number descent alone (no certificate builder).
	NoRace bool
	// NoChase runs the certificate builder without the perpetual-check
	// chase (chase.go), the measured alternative.
	NoChase bool
	// NetWeight adds Patricia's network to the certificate builder's
	// policy: the defender prefers the moves after which the attacker's
	// evaluation is lowest, NetWeight points per centipawn (0 = off; nnue,
	// one accumulator per defender node, incremental per move); the
	// measured experiment.
	NetWeight int
	// NoDescent runs the certificate builder alone (no proof-number
	// descent racing it), for measuring the builder.
	NoDescent bool
	// Epsilon widens the child's threshold (in eighths, as in matepn).
	Epsilon int
	// NumberCap clips the stored node numbers (0 = DefaultNumberCap).
	NumberCap uint32
	// LogNumbers keeps proof and disproof numbers on a logarithmic scale
	// (value v stands for about 2^(v-1) leaves): a sum is the larger
	// number plus one when both are within a factor of two, so the numbers
	// stay below the depth plus the log of the width and never saturate,
	// where the linear sums overflow through the DAG overcounting (a child
	// reached through many parents is summed every time). The threshold
	// "second best plus one" is the doubling of the epsilon trick.
	LogNumbers bool
	// MaxPositions stops the search with Aborted beyond this many positions.
	MaxPositions int
	// Stop (optional) is polled every StopNodes visits.
	Stop *atomic.Bool
	// Progress (optional) receives a line every ProgressEvery expansions and
	// after every fixed-point run.
	Progress      func(line string)
	ProgressEvery int
	// Oracle (optional) decides positions without a search: the endgame
	// tables (mateab.Tables) make every capture into a table a terminal,
	// which a draw certificate needs wherever the attacker can give up
	// material (the certificate builder cannot prove the defender's wins
	// itself). nil = mateab.Material (dead material only).
	Oracle mateab.Oracle
}

// Inf is the infinite proof or disproof number.
const Inf = 1 << 30

// StopNodes is the polling interval of Stop (a power of two).
const StopNodes = 1 << 12

// MaxDepth caps the descent (a node beyond it returns its thresholds, i.e.
// stays unknown); the graph has no other depth bound.
const MaxDepth = 1024

const (
	stUnknown uint8 = iota
	stWin           // the side to move wins
	stLoss          // the side to move loses
	stDrawT         // terminal draw (stalemate, dead material)
	stSafe          // the attacker cannot win here (fixed point), a draw at best for it
)

const (
	flExpanded uint8 = 1 << iota // children added
	flOnPath                     // on the current descent path
	flUnsafe                     // scratch flag of the fixed point
	flRegion                     // scratch flag of exhaust
)

// initialSlots: the store starts this small and grows fourfold whenever it
// is full (the keys are kept per node for the rehash): a question that
// ends after a few thousand positions must not pay for a 512 MB store.
const initialSlots = 1 << 16

// edge is an entry of an edge arena: the node at the other end and the
// next entry of the same list (noEdge at the end).
type edge struct {
	node, next uint32
}

const noEdge = ^uint32(0)

// addEdge appends the edge i -> c: c to the children of i (insertion
// order), i to the parents of c (newest first).
func (g *Graph) addEdge(i, c uint32) {
	e := uint32(len(g.cedges))
	g.cedges = append(g.cedges, edge{c, noEdge})
	if g.lastChild[i] == noEdge {
		g.firstChild[i] = e
	} else {
		g.cedges[g.lastChild[i]].next = e
	}
	g.lastChild[i] = e
	g.nchildren[i]++
	p := uint32(len(g.pedges))
	g.pedges = append(g.pedges, edge{i, g.firstParent[c]})
	g.firstParent[c] = p
	g.edges++
}

// Bytes is the memory of the graph by the capacities of its arrays and the
// store's slots (what the process holds for it, growth slack included).
func (g *Graph) Bytes() int {
	n := cap(g.recs) * bitboard.PackedFixedBytes
	n += (cap(g.cedges) + cap(g.pedges)) * 8
	n += (cap(g.firstChild) + cap(g.lastChild) + cap(g.firstParent)) * 4
	n += cap(g.nchildren) + cap(g.status) + cap(g.white) + cap(g.nmoves) + cap(g.remaining) + cap(g.flags)
	n += cap(g.level)*2 + (cap(g.pn)+cap(g.dn))*4
	n += cap(g.unsafeLevel)*2 + cap(g.chase) + cap(g.chaseDepth)*2 + cap(g.chaseQuiet)
	n += g.store.Bytes()
	return n
}

// childrenOf copies the children of node i into buf and returns them.
func (g *Graph) childrenOf(i uint32, buf *[chess.MaxMoves]uint32) []uint32 {
	n := 0
	for e := g.firstChild[i]; e != noEdge; e = g.cedges[e].next {
		buf[n] = g.cedges[e].node
		n++
	}
	return buf[:n]
}

// Graph is the explicit position graph of one question.
type Graph struct {
	attackerWhite bool
	log           bool // numbers on the log scale (Options.LogNumbers)
	store         *ttstore.Store
	maxSlots      int
	maxPositions  int                               // the position budget; an expansion starts only below it
	recs          [][bitboard.PackedFixedBytes]byte // the positions (32 bytes each, the key is recomputed from them)
	// the edges live in two arenas of singly linked lists (edge): the
	// children of a node in insertion order (firstChild to lastChild), the
	// parents newest first (firstParent); 12 bytes per node and 16 per
	// edge, where a slice per list cost 48 bytes of headers per node plus
	// the growth slack (measured 250 bytes per position before, see
	// docs/m7-wdl-search.md); nchildren counts the children
	cedges, pedges []edge
	firstChild     []uint32
	lastChild      []uint32
	firstParent    []uint32
	nchildren      []uint8
	status         []uint8
	white          []bool
	level          []uint16 // win/loss level in plies, filled by levels() for the line
	nmoves         []uint8  // legal moves
	remaining      []uint8  // legal moves not yet known to win for the opponent
	pn, dn         []uint32
	flags          []uint8
	expansions     int
	edges          int
	visits         uint64
	lastCheck      int // expansions at the last fixed-point run
	safeChecks     int
	// defenderConfigs counts, per placement of the defender's pieces, the
	// positions in the graph with that placement (the certificate builder's
	// policy prefers placements already in use: the king shuffles instead
	// of wandering); nil in the proof-number search
	defenderConfigs map[uint64]uint32
	// rootDefender is the placement of the defender's pieces in the root
	// (bitboards per kind), the reference of the stay-put policy
	rootDefender [6]uint64
	// unsafeLevel (certificate builder only) is each unsafe node's distance
	// to the leak it is unsafe from (a won or open node), so the widening
	// can start where the leak is instead of everywhere above it
	unsafeLevel []uint16
	// chase (certificate builder only) is the chase memo per node
	// (chUnknown/chHold/chHoldCond/chFail/chCut, chase.go), chaseDepth the
	// depth on the path, the depth of the ancestor a conditional hold relies
	// on, or the depth a cut node had left; chaseQuiet the quiet-move
	// budget a failure or cut was tried with; chaseVisits the node count
	chase       []uint8
	chaseDepth  []uint16
	chaseQuiet  []uint8
	chaseVisits uint64
	// oracle decides positions on sight (tables or dead material)
	oracle mateab.Oracle
	// shape (optional) receives the certificate's shape line (diagnostics)
	shape func(line string)
	// netWeight is Options.NetWeight for the policy (certificate builder only)
	netWeight int
}

// oracleOf is the option's oracle or the material fallback.
func oracleOf(opt Options) mateab.Oracle {
	if opt.Oracle != nil {
		return opt.Oracle
	}
	return mateab.Material{}
}

// defenderConfig hashes the placement of the defender's pieces (colour
// index 0 = white, 1 = black, as in bitboard.Board.Pieces).
func (g *Graph) defenderConfig(b *bitboard.Board) uint64 {
	side := 0
	if g.attackerWhite {
		side = 1
	}
	h := uint64(0x9E3779B97F4A7C15)
	for _, bb := range b.Pieces[side] {
		h = (h ^ bb) * 0xBF58476D1CE4E5B9
		h ^= h >> 29
	}
	return h
}

// Index returns the node index of a position (ok = false if unknown).
func (g *Graph) Index(b *bitboard.Board) (uint32, bool) {
	v, ok := g.store.Get(b.Key)
	return uint32(v), ok
}

// newGraph sets up the store for maxPositions (grown on demand) and adds
// the root.
func newGraph(root *bitboard.Board, sideToMoveAttacks bool, log bool, maxPositions int, oracle mateab.Oracle) *Graph {
	g := &Graph{attackerWhite: root.WhiteMove == sideToMoveAttacks, log: log, oracle: oracle, pn: []uint32{}, dn: []uint32{}}
	g.setBudget(maxPositions)
	g.store = ttstore.NewSlots(min(initialSlots, g.maxSlots))
	g.store.Put(root.Key, 0)
	g.add(root)
	return g
}

// setBudget sets the position budget (raised between the runs of a
// Solver). The store may grow to hold the budget plus one full expansion,
// so that an expansion started below the budget never finds it full: a
// node is either expanded completely or not at all, which a resumed run
// relies on.
func (g *Graph) setBudget(maxPositions int) {
	if maxPositions <= 0 {
		maxPositions = 1 << 26
	}
	g.maxPositions = maxPositions
	g.maxSlots = max(g.maxSlots, initialSlots)
	for g.maxSlots*ttstore.MaxLoadPercent/100 < maxPositions+chess.MaxMoves {
		g.maxSlots *= 2
	}
}

// full: the position budget is used up, no further expansion starts.
func (g *Graph) full() bool { return len(g.recs) >= g.maxPositions }

// getOrPut returns the node index of a key, inserting it as the next node
// when it is new (the caller then calls add); grows the store when it is
// full. ok is false once the position budget is exhausted.
func (g *Graph) getOrPut(key [2]uint64) (idx uint32, isNew, ok bool) {
	next := uint64(len(g.recs))
	v, isNew, ok := g.store.GetOrPut(key, next)
	if ok {
		return uint32(v), isNew, true
	}
	if g.store.Slots() >= g.maxSlots {
		return 0, false, false
	}
	// doubling (a fourfold jump left the store four times the graph right
	// after it: 66 bytes per position measured), the keys rehashed from
	// the records instead of a 16-byte copy per node
	grown := ttstore.NewSlots(min(g.store.Slots()*2, g.maxSlots))
	for i := range g.recs {
		grown.Put(g.board(uint32(i)).Key, uint64(i))
	}
	g.store = grown
	v, isNew, ok = g.store.GetOrPut(key, next)
	return uint32(v), isNew, ok
}

// AttackerWon reports whether the node is a win for the attacker (attacker
// to move and winning, or defender to move and losing).
func (g *Graph) AttackerWon(i uint32) bool {
	switch g.status[i] {
	case stWin:
		return g.attackerAt(i)
	case stLoss:
		return !g.attackerAt(i)
	}
	return false
}

// defenderHolds: the node is decided against the attacker (draw or the
// attacker gets mated).
func (g *Graph) defenderHolds(i uint32) bool {
	switch g.status[i] {
	case stDrawT, stSafe:
		return true
	case stWin:
		return !g.attackerAt(i)
	case stLoss:
		return g.attackerAt(i)
	}
	return false
}

func (g *Graph) attackerAt(i uint32) bool { return g.white[i] == g.attackerWhite }

// add creates a node: terminal check (dead material, mate, stalemate) and
// the leaf numbers from its mobility.
func (g *Graph) add(b *bitboard.Board) uint32 {
	i := uint32(len(g.recs))
	g.recs = append(g.recs, b.PackedFixedRecord())
	g.firstChild = append(g.firstChild, noEdge)
	g.lastChild = append(g.lastChild, noEdge)
	g.firstParent = append(g.firstParent, noEdge)
	g.nchildren = append(g.nchildren, 0)
	g.white = append(g.white, b.WhiteMove)
	g.level = append(g.level, 0)
	g.flags = append(g.flags, 0)
	attacker := b.WhiteMove == g.attackerWhite
	status, n := stUnknown, 0
	attackerSide := 1
	if g.attackerWhite {
		attackerSide = 0
	}
	// a bare king never gives check, so the attacker can never mate: a
	// terminal hold, sound without any search (dead material is the
	// general FIDE 5.2.2 case, this one is the attacker's half of it)
	bareAttacker := b.ByColor[attackerSide] == b.Pieces[attackerSide][0]
	// the oracle (tables: win, loss or draw for the side to move; without
	// tables only dead material) decides a node on sight, it is never
	// expanded (nmoves 0, which also seeds the win levels)
	v, _, _ := g.oracle.Probe(b)
	switch {
	case v == mateab.Win:
		status = stWin
	case v == mateab.Loss:
		status = stLoss
	case v == mateab.Draw || bareAttacker:
		status = stDrawT
	default:
		var buf chess.MoveBuffer
		n = b.GenMoves(&buf)
		if n == 0 {
			if b.InCheck() {
				status = stLoss
			} else {
				status = stDrawT
			}
		}
	}
	g.status = append(g.status, status)
	g.nmoves = append(g.nmoves, uint8(n))
	g.remaining = append(g.remaining, uint8(n))
	if g.defenderConfigs != nil {
		g.defenderConfigs[g.defenderConfig(b)]++
		g.chase = append(g.chase, chUnknown)
		g.chaseDepth = append(g.chaseDepth, 0)
		g.chaseQuiet = append(g.chaseQuiet, 0)
	}
	pn, dn := uint32(1), uint32(1)
	switch {
	case status == stLoss && !attacker:
		pn, dn = 0, Inf
	case status != stUnknown:
		pn, dn = Inf, 0
	case attacker:
		dn = g.leafNumber(n)
	default:
		pn = g.leafNumber(n)
	}
	if g.pn != nil {
		// the descent's numbers; the certificate builder's graph has none
		g.pn = append(g.pn, pn)
		g.dn = append(g.dn, dn)
	}
	return i
}

// leafNumber is the mobility of a fresh leaf as a number: n moves, or on
// the log scale 1 + log2(n) rounded down (bits.Len).
func (g *Graph) leafNumber(n int) uint32 {
	if g.log {
		return uint32(bits.Len32(uint32(n)))
	}
	return uint32(n)
}

// sum2 adds two numbers: linear with saturation, or on the log scale.
func (g *Graph) sum2(a, b uint32) uint32 {
	if g.log {
		return lsum(a, b)
	}
	return satAdd(a, b)
}

func (g *Graph) board(i uint32) bitboard.Board {
	b, _ := bitboard.DecodePackedFixed(g.recs[i][:])
	return b
}

// Solve answers one question. With sideToMoveAttacks the root is an attacker
// node ("the side to move forces a win"), otherwise a defender node. Unless
// told otherwise it races the proof-number descent (good at wins) against
// the certificate builder (good at draws) on two threads; the first decision
// wins and stops the other. One Run of a Solver with Options.MaxPositions.
func Solve(root *bitboard.Board, sideToMoveAttacks bool, opt Options) Result {
	extra := opt.MaxPositions
	if extra <= 0 {
		extra = 1 << 26
	}
	return NewSolver(root, sideToMoveAttacks, opt).Run(extra)
}

// Solver answers questions on one shared pair of graphs (the descent's
// and the certificate builder's), rooted at one position: the question
// node is the root itself (Run) or the position after a root move
// (RunMove). Every run continues the race until the question is decided,
// the stop flag is set or each graph has grown by the given number of
// positions (Aborted, call again), and keeps the graphs, so that the root
// moves of a position share their transpositions and the wins and safe
// sets found for one move, and a driver can share its time over them
// without restarting anything. Options.MaxPositions caps each graph.
type Solver struct {
	root bitboard.Board
	opt  Options
	pn   *searcher // the proof-number descent
	cert *searcher // the certificate builder (nil without the race)
}

// NewSolver prepares the graphs (they hold only the root). With
// sideToMoveAttacks the side to move at the root is the attacker.
func NewSolver(root *bitboard.Board, sideToMoveAttacks bool, opt Options) *Solver {
	s := &Solver{root: *root, opt: opt, pn: newPNSearcher(root, sideToMoveAttacks, opt)}
	if !opt.BruteForce && !opt.NoRace {
		s.cert = newCertSearcher(root, sideToMoveAttacks, opt)
	}
	return s
}

// Positions is the number of positions in both graphs (the memory).
func (s *Solver) Positions() int {
	n := len(s.pn.g.recs)
	if s.cert != nil {
		n += len(s.cert.g.recs)
	}
	return n
}

// Run continues the question of the root itself ("the attacker forces a
// win from the root"); extra is the number of positions each graph may
// add. The Result's counts are those of the deciding graph, or of the
// descent's graph while undecided.
func (s *Solver) Run(extra int) Result {
	return s.run(&s.root, extra)
}

// RunMove continues the question of the position after a root move (the
// root is expanded in each graph on the first such question).
func (s *Solver) RunMove(m chess.Move, extra int) Result {
	c := s.root
	c.DoMove(m)
	return s.run(&c, extra)
}

func (s *Solver) run(q *bitboard.Board, extra int) Result {
	if s.cert == nil {
		r := s.pn.runPN(q, extra)
		r.Method = "pn"
		if s.opt.BruteForce {
			r.Method = "brute force"
		}
		return r
	}
	if s.opt.NoDescent {
		s.cert.opt.Stop = s.opt.Stop
		r := s.cert.runCert(q, extra)
		r.Method = "certificate"
		return r
	}
	var stopPN, stopCert atomic.Bool
	done := make(chan struct{})
	if s.opt.Stop != nil {
		stop := s.opt.Stop
		go func() { // the caller's stop reaches both
			for {
				select {
				case <-done:
					return
				case <-time.After(10 * time.Millisecond):
					if stop.Load() {
						stopPN.Store(true)
						stopCert.Store(true)
					}
				}
			}
		}()
	}
	s.cert.opt.Stop = &stopCert
	certResult := make(chan Result, 1)
	go func() {
		r := s.cert.runCert(q, extra)
		r.Method = "certificate"
		stopPN.Store(true)
		certResult <- r
	}()
	s.pn.opt.Stop = &stopPN
	rp := s.pn.runPN(q, extra)
	rp.Method = "pn"
	stopCert.Store(true)
	rc := <-certResult
	close(done)
	switch {
	case rp.Outcome != Undecided:
		return rp
	case rc.Outcome != Undecided:
		return rc
	}
	rp.Aborted = rp.Aborted || rc.Aborted
	rp.Stuck = rp.Stuck || rc.Stuck
	return rp
}

// begin starts a run: the budget is the graph's size plus extra, capped
// by Options.MaxPositions.
func (s *searcher) begin(extra int) {
	s.aborted = false
	capN := s.opt.MaxPositions
	if capN <= 0 {
		capN = 1 << 26
	}
	s.g.setBudget(max(1, min(len(s.g.recs)+extra, capN)))
}

// locate finds the question node: the root, or a child of the root after
// a root move (the root is expanded on the first such question; ok is
// false when that expansion was cut off by the stop flag or the budget).
func (s *searcher) locate(q *bitboard.Board) (uint32, bool) {
	g := s.g
	if q.Key == g.board(0).Key {
		return 0, true
	}
	if g.flags[0]&flExpanded == 0 {
		s.expand(0)
		if s.aborted {
			return 0, false
		}
	}
	return g.Index(q)
}

// newPNSearcher sets up the proof-number descent of a question.
func newPNSearcher(root *bitboard.Board, sideToMoveAttacks bool, opt Options) *searcher {
	g := newGraph(root, sideToMoveAttacks, opt.LogNumbers, opt.MaxPositions, oracleOf(opt))
	s := &searcher{g: g, opt: opt, start: time.Now(), capN: opt.NumberCap}
	if opt.ProgressEvery <= 0 {
		s.opt.ProgressEvery = 1 << 20
	}
	if s.capN == 0 {
		s.capN = DefaultNumberCap
	}
	return s
}

// runPN continues the proof-number descent for the question node q.
func (s *searcher) runPN(q *bitboard.Board, extra int) Result {
	g := s.g
	opt := s.opt
	s.begin(extra)
	var r Result
	n, ok := s.locate(q)
	if !ok {
		r.Aborted, r.Positions = true, len(g.recs)
		return r
	}
	if opt.BruteForce {
		for i := 0; i < len(g.recs) && !s.aborted; i++ {
			if g.status[i] == stUnknown && g.flags[i]&flExpanded == 0 {
				s.expand(uint32(i))
			}
		}
		if !s.aborted {
			g.safeSet()
		}
	} else {
		for !s.aborted {
			pn, dn := s.mid(n, Inf, Inf, 0)
			if g.AttackerWon(n) || g.defenderHolds(n) {
				break
			}
			if dn == 0 || pn == 0 {
				// the heuristic decided, the fixed point has to confirm it
				g.safeSet()
				if g.defenderHolds(n) || g.AttackerWon(n) {
					break
				}
				s.resets++
				g.resetNumbers()
			}
		}
	}
	r.Stuck, r.Exhausts, r.Resets = s.stuck, s.exhausts, s.resets
	g.fillResult(&r, n)
	r.Aborted = s.aborted && r.Outcome == Undecided
	s.report(fmt.Sprintf("done: %d positions, %d expansions, %d edges, %d visits, %d won, %d fixed-point runs, %d resets, %s", len(g.recs), g.expansions, g.edges, g.visits, r.Won, g.safeChecks, r.Resets, fmtDur(time.Since(s.start))))
	return r
}

// fillResult reads the outcome of the question node n and the graph's
// counters into r.
func (g *Graph) fillResult(r *Result, n uint32) {
	switch {
	case g.AttackerWon(n):
		r.Outcome = Proven
	case g.defenderHolds(n):
		r.Outcome = Disproven
		r.Safe = g.reachableSafe(n)
	}
	for i := range g.status {
		if g.AttackerWon(uint32(i)) {
			r.Won++
		}
	}
	r.Positions, r.Expansions, r.Edges, r.Visits, r.SafeChecks, r.ChaseNodes = len(g.recs), g.expansions, g.edges, g.visits, g.safeChecks, g.chaseVisits
	r.Bytes = g.Bytes()
	if r.Outcome == Proven {
		g.levels()
		r.WinPlies = int(g.level[n])
		r.PV = g.line(n)
	}
}

type searcher struct {
	g        *Graph
	opt      Options
	start    time.Time
	capN     uint32
	aborted  bool
	stuck    bool
	exhausts int
	resets   int      // descent: heuristic disproofs the fixed point rejected
	rounds   int      // certificate builder: widening rounds so far
	chases   int      // certificate builder: check chains started
	frontier []uint32 // certificate builder: the nodes the next closure starts from
	// condLists[d] (chase) lists the nodes whose hold relies on the path
	// node at depth d, settled when that node finishes; the counters count
	// finished chase nodes
	condLists                                    [][]uint32
	chaseCap                                     int    // the current depth cap of the chase
	chaseLimit                                   uint64 // chase node count at which a budgeted chase is cut (0 = none)
	chaseNodes                                   bool   // chase at policy nodes too (those where defenderChases holds)
	chaseFails, chaseCuts, chaseHolds, chaseCond int
	quietMoves                                   int // quiet moves the chase tried
}

func (s *searcher) report(line string) {
	if s.opt.Progress != nil {
		s.opt.Progress(line)
	}
}

// expand adds all children of a node and propagates wins. It aborts (the
// stop flag, the position budget) before touching the node, never in the
// middle of it.
func (s *searcher) expand(i uint32) {
	g := s.g
	if g.full() || g.expansions&1023 == 0 && s.opt.Stop != nil && s.opt.Stop.Load() {
		s.aborted = true
		return
	}
	b := g.board(i)
	var buf chess.MoveBuffer
	n := b.GenMoves(&buf)
	g.flags[i] |= flExpanded
	g.expansions++
	for j := 0; j < n; j++ {
		c := b
		c.DoMove(buf[j])
		ci, isNew, ok := g.getOrPut(c.Key)
		if !ok {
			s.aborted = true
			return
		}
		if isNew {
			g.add(&c)
		}
		g.addEdge(i, ci)
	}
	// wins through the new edges: a losing child wins the node, a winning
	// child is one legal move less for the node
	for e := g.firstChild[i]; e != noEdge; e = g.cedges[e].next {
		ci := g.cedges[e].node
		if g.status[ci] == stLoss && g.status[i] == stUnknown {
			g.status[i] = stWin
		} else if g.status[ci] == stWin {
			g.remaining[i]--
		}
	}
	if g.status[i] == stUnknown && g.remaining[i] == 0 {
		g.status[i] = stLoss
	}
	if g.status[i] != stUnknown {
		g.propagate(i)
	}
	if g.expansions%s.opt.ProgressEvery == 0 {
		s.report(fmt.Sprintf("%d positions, %d expansions, %d edges, %d visits, %s", len(g.recs), g.expansions, g.edges, g.visits, fmtDur(time.Since(s.start))))
	}
	// the fixed point now and then: safe nodes are decided for good
	if !s.opt.BruteForce && g.expansions-g.lastCheck >= max(4096, len(g.recs)/4) {
		g.safeSet()
		s.report(fmt.Sprintf("fixed point at %d expansions: %d positions, %s", g.expansions, len(g.recs), fmtDur(time.Since(s.start))))
	}
}

// exhaust expands every undecided node reachable from c (brute force on
// the subgraph, each position once).
func (s *searcher) exhaust(c uint32) {
	g := s.g
	stack := []uint32{c}
	g.flags[c] |= flRegion
	var marked []uint32
	marked = append(marked, c)
	for len(stack) > 0 && !s.aborted {
		i := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		if g.status[i] != stUnknown {
			continue
		}
		if g.flags[i]&flExpanded == 0 {
			s.expand(i)
			if s.aborted {
				break
			}
		}
		for e := g.firstChild[i]; e != noEdge; e = g.cedges[e].next {
			ch := g.cedges[e].node
			if g.flags[ch]&flRegion == 0 && g.status[ch] == stUnknown {
				g.flags[ch] |= flRegion
				marked = append(marked, ch)
				stack = append(stack, ch)
			}
		}
	}
	for _, i := range marked {
		g.flags[i] &^= flRegion
	}
}

// propagate pushes a decided node to its parents: a win for the side to
// move takes one legal move from each parent (all gone = the parent loses),
// a loss makes every open parent win.
func (g *Graph) propagate(i uint32) {
	queue := []uint32{i}
	for len(queue) > 0 {
		c := queue[len(queue)-1]
		queue = queue[:len(queue)-1]
		switch g.status[c] {
		case stWin:
			for e := g.firstParent[c]; e != noEdge; e = g.pedges[e].next {
				p := g.pedges[e].node
				if g.status[p] != stUnknown {
					continue
				}
				g.remaining[p]--
				if g.remaining[p] == 0 {
					g.status[p] = stLoss
					queue = append(queue, p)
				}
			}
		case stLoss:
			for e := g.firstParent[c]; e != noEdge; e = g.pedges[e].next {
				p := g.pedges[e].node
				if g.status[p] == stUnknown {
					g.status[p] = stWin
					queue = append(queue, p)
				}
			}
		}
	}
}

// numbers of a child as seen from a node during the descent.
func (g *Graph) childNumbers(c uint32) (uint32, uint32) {
	switch {
	case g.AttackerWon(c):
		return 0, Inf
	case g.defenderHolds(c), g.flags[c]&flOnPath != 0:
		return Inf, 0 // decided for the defender, or a repetition of the path
	}
	return g.pn[c], g.dn[c]
}

// mid is Nagai's multiple iterative deepening on the graph: it searches
// below node i until its proof number reaches thPn or its disproof number
// reaches thDn, and returns the numbers at that point.
func (s *searcher) mid(i uint32, thPn, thDn uint32, depth int) (uint32, uint32) {
	g := s.g
	g.visits++
	if g.visits&(StopNodes-1) == 0 && s.opt.Stop != nil && s.opt.Stop.Load() {
		s.aborted = true
	}
	if g.visits&(1<<22-1) == 0 {
		s.report(fmt.Sprintf("%d visits at depth %d: %d positions, %d expansions, node %d pn %d dn %d thresholds %d/%d, %s", g.visits, depth, len(g.recs), g.expansions, i, g.pn[i], g.dn[i], thPn, thDn, fmtDur(time.Since(s.start))))
	}
	if s.aborted || depth >= MaxDepth {
		// a threshold is never a decision: an infinite one (the root's, or
		// passed down along single children) returned as a number would
		// make the parent's sum infinite, and an infinite number at the
		// root ends every descent at once without a decision (seen on
		// KRR-KN after Nh2: 25 million visits per five seconds, no
		// expansion)
		return finite(thPn), finite(thDn)
	}
	if g.AttackerWon(i) {
		return 0, Inf
	}
	if g.defenderHolds(i) {
		return Inf, 0
	}
	if g.flags[i]&flExpanded == 0 {
		s.expand(i)
		if s.aborted {
			return finite(thPn), finite(thDn)
		}
		if g.AttackerWon(i) {
			return 0, Inf
		}
		if g.defenderHolds(i) {
			return Inf, 0
		}
	}
	attacker := g.attackerAt(i)
	g.flags[i] |= flOnPath
	var childBuf [chess.MaxMoves]uint32
	children := g.childrenOf(i, &childBuf)
	// what a child returned from its descent overrides its stored numbers
	// for the rest of this loop: a child cut off by the depth cap (or an
	// unchanged one) must still move the parent on, as in wdlpn
	var local [chess.MaxMoves]struct {
		pn, dn uint32
		set    bool
	}
	childNumbers := func(ci int, c uint32) (uint32, uint32) {
		if local[ci].set {
			return local[ci].pn, local[ci].dn
		}
		return g.childNumbers(c)
	}
	for {
		var best, second uint32 = Inf, Inf
		var bestOther uint32
		var sum uint32
		bi := -1
		for ci, c := range children {
			cpn, cdn := childNumbers(ci, c)
			k, other := cpn, cdn
			if !attacker {
				k, other = cdn, cpn
			}
			sum = g.sum2(sum, other)
			if k < best || (k == best && bi >= 0 && other < bestOther) {
				second = best
				best, bi, bestOther = k, ci, other
			} else if k < second {
				second = k
			}
		}
		var pn, dn uint32
		if attacker {
			pn, dn = best, sum
		} else {
			pn, dn = sum, best
		}
		g.pn[i], g.dn[i] = clip(pn, s.capN), clip(dn, s.capN)
		if pn >= thPn || dn >= thDn || bi < 0 {
			g.flags[i] &^= flOnPath
			return pn, dn
		}
		c := children[bi]
		if g.flags[c]&flOnPath != 0 || g.AttackerWon(c) || g.defenderHolds(c) {
			// the best child is decided or a repetition: nothing to search
			// below on this path, so the parent must move on
			g.flags[i] &^= flOnPath
			return finite(max(pn, thPn)), finite(max(dn, thDn))
		}
		cpn, cdn := childNumbers(bi, c)
		var cThPn, cThDn uint32
		if g.log {
			// the child may double its best number (+1 on the log scale,
			// epsilon >= 1 lets it quadruple), and its other number may grow
			// until the sum over all children reaches the node's threshold
			bound := satAdd(second, 1)
			if s.opt.Epsilon >= 8 && second < Inf {
				bound = satAdd(second, 2)
			}
			// lsum is not associative, so the threshold for the child's other
			// number is found with the very sum the node computes: the
			// children in list order, the child's number replaced by v
			sumWith := func(v uint32) uint32 {
				var sum uint32
				for ci, c2 := range children {
					o := v
					if ci != bi {
						cpn2, cdn2 := childNumbers(ci, c2)
						o = cdn2
						if !attacker {
							o = cpn2
						}
					}
					sum = lsum(sum, o)
				}
				return sum
			}
			th := thDn
			if !attacker {
				th = thPn
			}
			other := lthreshold(th, sumWith)
			if attacker {
				cThPn, cThDn = min(thPn, bound), other
			} else {
				cThDn, cThPn = min(thDn, bound), other
			}
		} else {
			bound := satAdd(second, 1)
			if s.opt.Epsilon > 0 && second < Inf {
				bound = satAdd(second, max(1, uint32(uint64(second)*uint64(s.opt.Epsilon)/8)))
			}
			if attacker {
				cThPn = min(thPn, bound)
				cThDn = satSub(thDn, satSub(dn, cdn))
			} else {
				cThDn = min(thDn, bound)
				cThPn = satSub(thPn, satSub(pn, cpn))
			}
		}
		rpn, rdn := s.mid(c, cThPn, cThDn, depth+1)
		if local[bi].set && local[bi].pn == rpn && local[bi].dn == rdn && !s.aborted {
			// the child came back twice with the same numbers under the same
			// thresholds: the numbers have saturated (the DAG overcounting
			// pushes sums to the ceiling, where thresholds cannot exceed
			// them) and the heuristic is blind here. Fall back to brute
			// force below the child: expand its whole undecided subgraph
			// once, then the fixed point decides it, and the parent moves on.
			s.exhausts++
			s.report(fmt.Sprintf("saturated at node %d (depth %d, pn %d dn %d, thresholds %d/%d): child %d returned %d/%d twice under %d/%d, expanding its subgraph", i, depth, pn, dn, thPn, thDn, c, rpn, rdn, cThPn, cThDn))
			s.exhaust(c)
			if !s.aborted {
				g.safeSet()
			}
			if g.AttackerWon(i) {
				g.flags[i] &^= flOnPath
				return 0, Inf
			}
			if g.defenderHolds(i) {
				g.flags[i] &^= flOnPath
				return Inf, 0
			}
			if s.aborted {
				// the budget or the stop flag ended the exhaust: not a decision, not a bug
				g.flags[i] &^= flOnPath
				return pn, dn
			}
			if !g.AttackerWon(c) && !g.defenderHolds(c) {
				// cannot happen: a fully expanded subgraph is decided by the fixed point
				s.aborted, s.stuck = true, true
				g.flags[i] &^= flOnPath
				return pn, dn
			}
			local[bi].set = false
			continue
		}
		local[bi].pn, local[bi].dn, local[bi].set = rpn, rdn, true
		if s.aborted {
			g.flags[i] &^= flOnPath
			return pn, dn
		}
		if g.AttackerWon(i) {
			g.flags[i] &^= flOnPath
			return 0, Inf
		}
		if g.defenderHolds(i) {
			g.flags[i] &^= flOnPath
			return Inf, 0
		}
	}
}

// resetNumbers gives every open node its leaf numbers again (after a
// heuristic disproof the fixed point did not confirm).
func (g *Graph) resetNumbers() {
	for i := range g.status {
		if g.status[i] != stUnknown {
			continue
		}
		n := g.leafNumber(int(g.nmoves[i]))
		if g.attackerAt(uint32(i)) {
			g.pn[i], g.dn[i] = 1, n
		} else {
			g.pn[i], g.dn[i] = n, 1
		}
	}
}

// safeSet runs the fixed point: unsafe is the least fixed point from the
// won nodes and the unexpanded open nodes (an attacker node is unsafe if
// any child is, a defender node if all children are); every open node that
// is not unsafe becomes safe for good.
func (g *Graph) safeSet() {
	g.safeChecks++
	g.lastCheck = g.expansions
	n := len(g.recs)
	safeChildren := make([]uint16, n)
	if g.unsafeLevel != nil {
		g.unsafeLevel = g.unsafeLevel[:0]
		g.unsafeLevel = append(g.unsafeLevel, make([]uint16, n)...)
	}
	var queue []uint32
	for i := 0; i < n; i++ {
		g.flags[i] &^= flUnsafe
		idx := uint32(i)
		unsafe := false
		switch {
		case g.status[i] != stUnknown:
			unsafe = g.AttackerWon(idx)
		case g.flags[i]&flExpanded == 0:
			unsafe = true
		default:
			safeChildren[i] = uint16(g.nchildren[i])
		}
		if unsafe {
			g.flags[i] |= flUnsafe
			queue = append(queue, idx)
		}
	}
	// breadth-first, so that unsafeLevel is the distance to the leak
	for head := 0; head < len(queue); head++ {
		c := queue[head]
		for e := g.firstParent[c]; e != noEdge; e = g.pedges[e].next {
			p := g.pedges[e].node
			if g.flags[p]&flUnsafe != 0 || g.status[p] != stUnknown {
				continue
			}
			if !g.attackerAt(p) {
				safeChildren[p]--
				if safeChildren[p] != 0 {
					continue
				}
			}
			g.flags[p] |= flUnsafe
			if g.unsafeLevel != nil {
				g.unsafeLevel[p] = g.unsafeLevel[c] + 1
			}
			queue = append(queue, p)
		}
	}
	for i := 0; i < n; i++ {
		if g.status[i] == stUnknown && g.flags[i]&flUnsafe == 0 {
			g.status[i] = stSafe
		}
	}
}

// reachableSafe counts the positions reachable from the root through
// defender-holding children only, the attacker with all moves, the defender
// with one (the draw certificate), and reports the certificate's shape.
func (g *Graph) reachableSafe(from uint32) int {
	seen := make([]bool, len(g.recs))
	stack := []uint32{from}
	seen[from] = true
	count := 0
	var kingHist [64]int
	attackerSide, defenderSide := 1, 0
	if g.attackerWhite {
		attackerSide, defenderSide = 0, 1
	}
	placements := map[[6]uint64]bool{}
	defenderPlacements := map[[6]uint64]bool{}
	for len(stack) > 0 {
		i := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		count++
		if count <= 1<<20 {
			b := g.board(i)
			kingHist[bits.TrailingZeros64(b.Pieces[attackerSide][0])]++
			placements[b.Pieces[attackerSide]] = true
			defenderPlacements[b.Pieces[defenderSide]] = true
		}
		attacker := g.attackerAt(i)
		for e := g.firstChild[i]; e != noEdge; e = g.cedges[e].next {
			c := g.cedges[e].node
			if !g.defenderHolds(c) {
				continue
			}
			if !seen[c] {
				seen[c] = true
				stack = append(stack, c)
			}
			if !attacker {
				break
			}
		}
	}
	if g.shape != nil {
		squares := 0
		for _, n := range kingHist {
			if n > 0 {
				squares++
			}
		}
		g.shape(fmt.Sprintf("certificate: %d positions, attacker king on %d squares, %d attacker placements, %d defender placements", count, squares, len(placements), len(defenderPlacements)))
	}
	return count
}

// levels recomputes win and loss levels from the mates (for the line).
func (g *Graph) levels() {
	n := len(g.recs)
	remaining := make([]uint8, n)
	done := make([]bool, n)
	var queue, winSeeds []uint32
	for i := 0; i < n; i++ {
		remaining[i] = g.nmoves[i]
		g.level[i] = 0
		if g.nmoves[i] != 0 {
			continue
		}
		switch g.status[i] {
		case stLoss: // mate, or a table loss: level 0
			done[i] = true
			queue = append(queue, uint32(i))
		case stWin: // a table win: level 1, joins the first wave of wins
			done[i] = true
			g.level[i] = 1
			winSeeds = append(winSeeds, uint32(i))
		}
	}
	for lvl := 0; len(queue) > 0 || (lvl == 0 && len(winSeeds) > 0); lvl++ {
		var next []uint32
		if lvl == 0 {
			next = append(next, winSeeds...)
		}
		for _, c := range queue {
			for e := g.firstParent[c]; e != noEdge; e = g.pedges[e].next {
				p := g.pedges[e].node
				if done[p] {
					continue
				}
				if lvl%2 == 0 {
					done[p] = true
					g.level[p] = uint16(lvl + 1)
					next = append(next, p)
				} else {
					remaining[p]--
					if remaining[p] == 0 {
						done[p] = true
						g.level[p] = uint16(lvl + 1)
						next = append(next, p)
					}
				}
			}
		}
		queue = next
	}
}

// line follows the levels from the root: the attacker takes the child that
// loses fastest, the defender the child that wins slowest.
func (g *Graph) line(root uint32) []chess.Move {
	var pv []chess.Move
	i := root
	for len(pv) < 500 {
		b := g.board(i)
		var buf chess.MoveBuffer
		n := b.GenMoves(&buf)
		if n == 0 {
			break
		}
		attackerWins := g.AttackerWon(i)
		best, bestIdx, found := chess.Move{}, uint32(0), false
		var bestLevel uint16
		for j := 0; j < n; j++ {
			c := b
			c.DoMove(buf[j])
			ci, ok := g.Index(&c)
			if !ok || !g.AttackerWon(ci) {
				continue
			}
			if g.attackerAt(i) == attackerWins && g.attackerAt(i) {
				// the attacker moves: shortest
				if !found || g.level[ci] < bestLevel {
					best, bestIdx, bestLevel, found = buf[j], ci, g.level[ci], true
				}
			} else if !found || g.level[ci] > bestLevel {
				best, bestIdx, bestLevel, found = buf[j], ci, g.level[ci], true
			}
		}
		if !found {
			break
		}
		pv = append(pv, best)
		i = bestIdx
	}
	return pv
}

// DefaultNumberCap clips the numbers a node stores, exactly like the
// saturating codec of wdlpn (10 bits each): the sums computed inside a
// node and the thresholds are not clipped, so the children of one node stay
// distinguishable and a child at the clip can still exceed its threshold.
// The DAG overcounting (a child reached through many parents is summed
// every time) pushes stored numbers to the clip quickly; a wide clip (Inf/2,
// testbank letter w) is the measured alternative.
const DefaultNumberCap = 1022

// WideNumberCap stores the numbers almost unclipped.
const WideNumberCap = Inf / 2

func clip(x, capN uint32) uint32 {
	if x >= Inf {
		return Inf
	}
	return min(x, capN)
}

// lsum is the sum on the log scale: the larger number, plus one when the
// two are within a factor of two (log2 of 2^a + 2^b, rounded); 0 (a proof)
// adds nothing, Inf absorbs.
func lsum(a, b uint32) uint32 {
	if a >= Inf || b >= Inf {
		return Inf
	}
	if a == 0 {
		return b
	}
	if b == 0 {
		return a
	}
	hi, lo := max(a, b), min(a, b)
	if hi-lo <= 1 {
		return hi + 1
	}
	return hi
}

// lthreshold is the smallest number v with sumWith(v) >= th: how far a
// child's number may grow before the sum over all children reaches the
// node's threshold (the log-scale form of th - (sum - child)). sumWith must
// be monotone in v.
func lthreshold(th uint32, sumWith func(v uint32) uint32) uint32 {
	if th >= Inf {
		return Inf
	}
	v := uint32(0)
	if th > 2 {
		v = th - 2
	}
	for v < Inf && sumWith(v) < th {
		v++
	}
	return v
}

// finite caps a threshold handed back as a number below Inf.
func finite(x uint32) uint32 { return min(x, Inf-1) }

// satAdd adds two node numbers, Inf absorbing, finite sums below Inf.
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

func fmtDur(d time.Duration) string {
	if d < time.Second {
		return fmt.Sprintf("%d ms", d.Milliseconds())
	}
	return fmt.Sprintf("%.2f s", d.Seconds())
}
