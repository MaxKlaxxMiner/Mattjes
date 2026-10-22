package wdllist

import (
	"fmt"
	"math/bits"
	"sort"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mateab"
)

// The chase is the perpetual-check searcher of the certificate builder.
// The policy builder fails on a perpetual check for a structural reason: it
// picks one check per defender node and widens where the fixed point finds
// the leak, but the leak of a check chain is where the attacker's king
// finally escapes, 9 to 25 plies below the check that was wrong, and no
// second move there helps (measured on KQ-KRR, docs/m7-wdl-search.md). The
// question "which check keeps the perpetual" is a depth-first question, so
// the chase asks it as one: it follows the chain as a path, the defender
// with chasing moves only (checks, captures into the tables, moves into
// positions known to hold), the attacker with every reply, and backtracks
// to swap the last check when a chain ends. The branching is tiny (a
// check leaves the attacker a handful of replies), so the chain is cheap
// where the policy builder's closure was hundreds of thousands of
// positions.
//
// A repetition of a node on the path closes the chain, which is sound only
// if that ancestor holds in the end: the graph-history problem. The chase
// keeps it exact the way Tarjan finds strongly connected components: every
// hold carries the depth of the highest ancestor its repetitions reach
// (its low link). A hold whose low link is its own depth is a closed set
// in itself and is memoized for good; a hold that reaches higher is
// conditional, memoized only while that ancestor is on the path and listed
// with it: when the ancestor closes in itself, the listed nodes become
// holds for good, when it closes conditionally they move to its own
// ancestor, and when it fails or is cut they are forgotten (chUnknown).
// Failures are memoized unconditionally (a failure never relies on the
// path: every check has a reply from which the attacker escapes, however
// the node was reached), cuts with the depth they had left, so a node cut
// once is not cut again at the same depth within a chase but is chased
// deeper from the top when the widening reaches it. Everything the chase
// touches is in the graph (attacker nodes fully expanded, defender nodes
// with their tried checks), so the fixed point sees exactly the structure
// the chase built and stays the judge. Where the chain fails the node
// falls back to the policy move, and the closure and widening go on as
// before.
//
// Measured on KQ-KRR (3k4/8/8/8/8/8/5rr1/K6Q w, the four-piece tables as
// oracle): the question "Black wins" was open at 16 Mi positions before,
// the chase answers it with a certificate of 28,712 positions in 22 s,
// the attacker's king on 60 squares (the queen always has a check, there
// is no cage). The seven-piece KQP-KRRP position holds in under a second
// with Syzygy. Tried and dropped: the attacker's replies ordered by the
// king's distance to its pieces (no change), a cut ending the node's
// alternatives (the chains never closed), and the proof-number descent
// restricted to chasing moves on the same graph (fast on the seven-piece
// position, thrashing at four million visits on KQ-KRR).

const (
	chUnknown  uint8 = iota // not chased yet, cut by a budget, or a conditional hold whose ancestor failed: chase again
	chHold                  // a chain from here closes in itself
	chHoldCond              // a chain from here closes on an ancestor still on the path (chaseDepth = its depth)
	chFail                  // every chasing move fails, however the node was reached
	chCut                   // cut off with chaseDepth plies left: chase again only with more
)

// ChaseMaxDepth is the deepest cap of a chain; a node beyond the cap is
// cut, not failed. A chase from the top deepens iteratively (chaseTop): a
// perpetual check closes within a few plies once the right checks are
// picked, and a chain that runs for hundreds of plies without a repetition
// is a king walking across the board, whose cuts multiply with every
// alternative check above them.
const ChaseMaxDepth = 256

// ChaseFirstDepth is the first cap of the iterative deepening.
const ChaseFirstDepth = 16

// ChaseNodeBudget is the number of chase nodes a chase at a policy node
// may visit (the chase from the question node is unbounded): a short
// perpetual is found within it, a long chain is left to the policy and
// the widening, which chase the node again when they reach it.
const ChaseNodeBudget = 1 << 12

type chaseResult uint8

const (
	chaseFail chaseResult = iota
	chaseHold
	chaseCut // the depth cap, the budget or the stop flag: no verdict
)

// noLow is the low link of a hold that relies on no ancestor.
const noLow = ChaseMaxDepth + 1

func (r chaseResult) String() string { return [...]string{"fail", "hold", "cut"}[r] }

// chaseHeld: the node holds by the chase memo (for good or on the path).
func (g *Graph) chaseHeld(i uint32) bool {
	return g.chase[i] == chHold || g.chase[i] == chHoldCond
}

// chaseable: the node is chased when reached (not chased yet, forgotten,
// or cut off with less depth than a chase from the top has).
func (g *Graph) chaseable(i uint32) bool {
	return g.chase[i] == chUnknown || g.chase[i] == chCut
}

// chaseTop chases from node i with caps doubling from ChaseFirstDepth to
// ChaseMaxDepth until the result is a verdict (the memo keeps what the
// shallower chases settled, only the cut parts are chased deeper); with a
// budget (nodes, 0 = none) the chase is cut when it is used up.
func (s *searcher) chaseTop(i uint32, budget uint64) chaseResult {
	s.chases++
	s.chaseLimit = 0
	if budget > 0 {
		s.chaseLimit = s.g.chaseVisits + budget
	}
	defer func() { s.chaseLimit = 0 }()
	for capN := ChaseFirstDepth; ; capN *= 2 {
		s.chaseCap = min(capN, ChaseMaxDepth)
		before, nodesBefore := len(s.g.recs), s.g.chaseVisits
		res, _ := s.chase(i, 0)
		if s.g.chaseVisits-nodesBefore >= 1<<14 {
			s.report(fmt.Sprintf("chase from node %d, cap %d: %s, %d nodes, %d new positions, %d fails, %d cuts, %d holds (%d conditional) so far, %s", i, s.chaseCap, res, s.g.chaseVisits-nodesBefore, len(s.g.recs)-before, s.chaseFails, s.chaseCuts, s.chaseHolds, s.chaseCond, fmtDur(time.Since(s.start))))
		}
		if res != chaseCut || s.aborted || s.chaseCap == ChaseMaxDepth || s.chaseLimit != 0 && s.g.chaseVisits >= s.chaseLimit {
			return res
		}
	}
}

// chase follows the check chain below node i: a defender node needs one
// move whose chain closes, an attacker node needs every reply to close.
// Returns the result and, for a hold, its low link (the depth of the
// highest ancestor its repetitions reach; noLow if none).
func (s *searcher) chase(i uint32, depth int) (chaseResult, int) {
	g := s.g
	if g.defenderHolds(i) {
		return chaseHold, noLow
	}
	if g.AttackerWon(i) {
		return chaseFail, noLow
	}
	if g.flags[i]&flOnPath != 0 {
		// a repetition closes the chain on this ancestor (checked before the
		// memo: a node on the path still carries the memo of its last chase,
		// a cut there would hide the repetition)
		return chaseHold, int(g.chaseDepth[i])
	}
	switch g.chase[i] {
	case chHold:
		return chaseHold, noLow
	case chHoldCond:
		return chaseHold, int(g.chaseDepth[i])
	case chFail:
		return chaseFail, noLow
	case chCut:
		if s.chaseCap-depth <= int(g.chaseDepth[i]) {
			return chaseCut, noLow
		}
	}
	if s.aborted || depth >= s.chaseCap || s.chaseLimit != 0 && g.chaseVisits >= s.chaseLimit {
		return chaseCut, noLow
	}
	g.chaseVisits++
	if g.chaseVisits&(1<<20-1) == 0 {
		s.report(fmt.Sprintf("chase: %d nodes, %d positions, %d fails, %d cuts, %d holds (%d conditional), depth %d, %s", g.chaseVisits, len(g.recs), s.chaseFails, s.chaseCuts, s.chaseHolds, s.chaseCond, depth, fmtDur(time.Since(s.start))))
	}
	g.flags[i] |= flOnPath
	g.chaseDepth[i] = uint16(depth)
	s.condLists[depth] = s.condLists[depth][:0]
	var res chaseResult
	var low int
	if g.attackerAt(i) {
		res, low = s.chaseAttacker(i, depth)
	} else {
		res, low = s.chaseDefender(i, depth)
	}
	g.flags[i] &^= flOnPath
	dependents := s.condLists[depth]
	switch {
	case res == chaseHold && low >= depth:
		s.chaseHolds++
		g.chase[i] = chHold
		for _, d := range dependents {
			g.chase[d] = chHold
		}
		low = noLow
	case res == chaseHold:
		s.chaseCond++
		g.chase[i], g.chaseDepth[i] = chHoldCond, uint16(low)
		for _, d := range dependents {
			g.chaseDepth[d] = uint16(low)
		}
		s.condLists[low] = append(s.condLists[low], i)
		s.condLists[low] = append(s.condLists[low], dependents...)
	case res == chaseFail:
		s.chaseFails++
		g.chase[i] = chFail
		for _, d := range dependents {
			g.chase[d] = chUnknown
		}
	default:
		s.chaseCuts++
		g.chase[i], g.chaseDepth[i] = chCut, uint16(s.chaseCap-depth)
		if s.chaseLimit != 0 && g.chaseVisits >= s.chaseLimit {
			g.chase[i] = chUnknown // cut by the budget, not by the depth: chase again at any depth
		}
		for _, d := range dependents {
			g.chase[d] = chUnknown
		}
	}
	return res, low
}

// chaseAttacker: every reply of the attacker must close (all replies are
// looked at even after a cut, a failing one is a verdict). The replies are
// tried failure first: known failures, then the replies that leave the
// defender the fewest checks (the escape), new positions before known
// ones, the known holds last; a chain that fails must fail cheaply, since
// every check of every node above it is tried in turn.
func (s *searcher) chaseAttacker(i uint32, depth int) (chaseResult, int) {
	g := s.g
	if g.flags[i]&flExpanded == 0 {
		s.expand(i)
		if s.aborted {
			return chaseCut, noLow
		}
		if g.AttackerWon(i) {
			return chaseFail, noLow
		}
		if g.defenderHolds(i) {
			return chaseHold, noLow
		}
	}
	type reply struct {
		c     uint32
		score int
	}
	var replies [chess.MaxMoves]reply
	nr := 0
	low := noLow
	for _, c := range g.children[i] {
		score := 0
		switch {
		case g.AttackerWon(c) || g.chase[c] == chFail:
			return chaseFail, noLow
		case g.defenderHolds(c) || g.chase[c] == chHold:
			continue // closes for free
		case g.chase[c] == chHoldCond || g.flags[c]&flOnPath != 0:
			low = min(low, int(g.chaseDepth[c]))
			continue // closes on an ancestor
		default:
			b := g.board(c)
			score = 2 * checkCount(&b)
			if g.flags[c]&flExpanded != 0 {
				score++ // a known position: the defender has been here
			}
		}
		replies[nr] = reply{c, score}
		nr++
	}
	sort.SliceStable(replies[:nr], func(a, b int) bool { return replies[a].score < replies[b].score })
	res := chaseHold
	for _, r := range replies[:nr] {
		rres, rlow := s.chase(r.c, depth+1)
		switch rres {
		case chaseFail:
			return chaseFail, noLow
		case chaseCut:
			res = chaseCut
		default:
			low = min(low, rlow)
		}
	}
	return res, low
}

// chaseDefender tries the defender's chasing moves in order: moves into
// positions that hold (decided, chased or on the path) first, then the
// checks by a two-ply look-ahead (lookAhead) that seeks the closure (a
// check with a reply that wins for the attacker is refuted and skipped).
// Every tried move becomes an edge; a node without such a move fails at
// once and is left untouched (an expanded defender node without children
// would count as safe in the fixed point).
func (s *searcher) chaseDefender(i uint32, depth int) (chaseResult, int) {
	g := s.g
	if g.full() {
		s.aborted = true
		return chaseCut, noLow
	}
	b := g.board(i)
	var buf chess.MoveBuffer
	n := b.GenMoves(&buf)
	type candidate struct {
		m     chess.Move
		score int
	}
	var cands [chess.MaxMoves]candidate
	nc := 0
	for _, m := range buf[:n] {
		c := b
		c.DoMove(m)
		score := 0
		known := false
		if idx, ok := g.Index(&c); ok {
			known = true
			switch {
			case g.AttackerWon(idx) || g.chase[idx] == chFail:
				continue
			case g.defenderHolds(idx) || g.chase[idx] == chHold:
				score = 3000
			case g.chase[idx] == chHoldCond:
				score = 2500
			case g.flags[idx]&flOnPath != 0:
				score = 2000
			}
		}
		capture := m.Capture != chess.None || m.Promo != chess.None
		if score == 0 && capture && !known {
			// a capture into the tables: a terminal, decided on sight
			switch v, _, _ := g.oracle.Probe(&c); v {
			case mateab.Draw, mateab.Loss:
				score = 3000
			case mateab.Win:
				continue
			}
		}
		if score == 0 {
			if !c.InCheck() {
				continue
			}
			var refuted bool
			score, refuted = s.lookAhead(&c)
			if refuted {
				continue
			}
			if capture {
				score++
			}
		}
		cands[nc] = candidate{m, score}
		nc++
	}
	if nc == 0 {
		return chaseFail, noLow
	}
	sort.SliceStable(cands[:nc], func(a, b int) bool { return cands[a].score > cands[b].score })
	res := chaseFail
	for _, cand := range cands[:nc] {
		if s.aborted {
			return chaseCut, noLow
		}
		c := b
		c.DoMove(cand.m)
		ci, ok := s.link(i, &c)
		if !ok {
			return chaseCut, noLow
		}
		if g.AttackerWon(i) {
			return chaseFail, noLow
		}
		if g.defenderHolds(i) {
			return chaseHold, noLow
		}
		cres, clow := s.chase(ci, depth+1)
		switch cres {
		case chaseHold:
			return chaseHold, clow
		case chaseCut:
			res = chaseCut
		}
	}
	return res, noLow
}

// lookAhead scores a check (c is the position after it, the attacker to
// move): 1000 when every reply holds already, else 8 times the smallest
// number of checks the defender has left after a reply, minus 40 per reply
// into a new position and 20 per reply into a known open one (the chain
// should close, not roam); refuted when a reply refutes the check (it
// wins for the attacker by the tables, the graph or the memo).
func (s *searcher) lookAhead(c *bitboard.Board) (score int, refuted bool) {
	g := s.g
	var buf chess.MoveBuffer
	n := c.GenMoves(&buf)
	worst := 1000
	penalty := 0
	for _, r := range buf[:n] {
		d := *c
		d.DoMove(r)
		if idx, ok := g.Index(&d); ok {
			switch {
			case g.AttackerWon(idx) || g.chase[idx] == chFail:
				return 0, true
			case g.defenderHolds(idx) || g.chaseHeld(idx) || g.flags[idx]&flOnPath != 0:
				continue
			}
			penalty += 20
		} else {
			if r.Capture != chess.None || r.Promo != chess.None {
				switch v, _, _ := g.oracle.Probe(&d); v {
				case mateab.Loss:
					return 0, true // the defender to move loses
				case mateab.Win, mateab.Draw:
					continue
				}
			}
			penalty += 40
		}
		worst = min(worst, checkCount(&d))
	}
	if worst == 1000 {
		return 1000, false
	}
	return 8*worst - penalty, false
}

// checkCount is the number of checking moves of the side to move.
func checkCount(b *bitboard.Board) int {
	var buf chess.MoveBuffer
	n := b.GenMoves(&buf)
	count := 0
	for _, m := range buf[:n] {
		if b.GivesCheck(m) {
			count++
		}
	}
	return count
}

// material is the usual piece count of a side (queen 9, rook 5, bishop
// and knight 3, pawn 1).
func material(b *bitboard.Board, side int) int {
	total := 0
	for kind, value := range [6]int{0, 9, 5, 3, 3, 1} {
		total += value * bits.OnesCount64(b.Pieces[side][kind])
	}
	return total
}

// link adds the edge from node i to the position c (a new node if needed)
// and updates the win counters as expand does; ok is false when the budget
// is used up. An existing edge is left alone.
func (s *searcher) link(i uint32, c *bitboard.Board) (uint32, bool) {
	g := s.g
	ci, isNew, ok := g.getOrPut(c.Key)
	if !ok {
		s.aborted = true
		return 0, false
	}
	if isNew {
		g.add(c)
	}
	if g.flags[i]&flExpanded == 0 {
		g.flags[i] |= flExpanded
		g.expansions++
	}
	if g.hasChild(i, ci) {
		return ci, true
	}
	g.children[i] = append(g.children[i], ci)
	g.parents[ci] = append(g.parents[ci], i)
	g.edges++
	if g.status[i] != stUnknown {
		return ci, true // decided already (and propagated then): the counters are over
	}
	if g.status[ci] == stLoss {
		g.status[i] = stWin
	} else if g.status[ci] == stWin {
		g.remaining[i]--
		if g.remaining[i] == 0 {
			g.status[i] = stLoss
		}
	}
	if g.status[i] != stUnknown {
		// propagated exactly once, on the change: a second propagation would
		// take another move from every parent's counter (seen as a false
		// loss after Qh4+ in the perpetual-check position)
		g.propagate(i)
	}
	return ci, true
}

// resetChase forgets the holds the fixed point did not confirm (none, if
// the low links are right: the fixed point is the judge, this is the
// insurance) and returns their number.
func (g *Graph) resetChase() int {
	n := 0
	for i := range g.chase {
		if g.chase[i] == chHold && g.status[i] == stUnknown {
			g.chase[i] = chUnknown
			n++
		}
	}
	return n
}
