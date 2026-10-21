package wdllist

import (
	"fmt"
	"math/bits"
	"sort"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mateab"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/ttstore"
)

// The certificate builder is the draw side of the search: it builds the
// defender's safe set directly instead of hunting it with proof numbers.
// Every defender node gets one policy move, every attacker node all of its
// moves; the breadth-first closure of that is a candidate certificate, the
// fixed point says whether it holds. Where it does not (an attacker win
// leaks in, or a leaf is open), only the unsafe defender nodes on the way
// from the root get their next policy move, and the closure grows from
// there. A draw whose certificate is small (KBP-K with the wrong bishop:
// the king shuffles between a8 and b8 while the attacker roams) is found
// in the size of that certificate, where the proof numbers saturated on
// the attacker's freedom and brute force faced 25 million positions.
//
// It is complete for draws (the widening ends at the reachable graph) and
// slow for wins (a win needs every defender move); the proof-number
// descent is the other way round, so Solve races the two.

// policyScore orders a defender's moves. Decisive first: a position known
// to hold (the cycle closes for free), a terminal draw, a capture of
// attacker material; known attacker wins and mates last. Then the shape of
// the certificate: stay put (every step the defender's pieces stray from
// their root squares multiplies the attacker configurations to cover, and
// the king that leaves its corner lets the pawn promote), never leave a
// piece en prise, prefer positions already in the graph and placements of
// the own pieces the graph already uses (the king shuffles between the
// same squares).
func (g *Graph) policyScore(b *bitboard.Board, m chess.Move) int {
	c := *b
	c.DoMove(m)
	s := 0
	if idx, ok := g.Index(&c); ok {
		switch {
		case g.AttackerWon(idx):
			return -100000
		case g.defenderHolds(idx):
			return 100000
		default:
			s += 500
		}
	}
	if v, _, _ := (mateab.Material{}).Probe(&c); v == mateab.Draw {
		return 100000 // dead material
	}
	var buf chess.MoveBuffer
	n := c.GenMoves(&buf)
	if n == 0 {
		if c.InCheck() {
			return -100000 // mate
		}
		return 100000 // stalemate
	}
	for _, am := range buf[:n] {
		if c.Squares[am.To] != chess.None {
			s -= 2000 // the attacker can capture (no exchange evaluation yet)
		}
	}
	if b.Squares[m.To] != chess.None {
		s += 300 // the defender captures
	}
	s -= 300 * g.displacement(&c)
	s += int(min(g.defenderConfigs[g.defenderConfig(&c)], 200))
	return s
}

// displacement sums, over the defender's pieces, the Chebyshev distance to
// the nearest square its kind occupied in the root position.
func (g *Graph) displacement(b *bitboard.Board) int {
	side := 0
	if g.attackerWhite {
		side = 1
	}
	d := 0
	for kind, bb := range b.Pieces[side] {
		root := g.rootDefender[kind]
		if bb == 0 || root == 0 {
			continue
		}
		for pieces := bb; pieces != 0; pieces &= pieces - 1 {
			sq := bits.TrailingZeros64(pieces)
			best := 8
			for r := root; r != 0; r &= r - 1 {
				best = min(best, chebyshev(sq, bits.TrailingZeros64(r)))
			}
			d += best
		}
	}
	return d
}

// kingSquares is a diagnostic: the defender king's squares over a sample of
// the graph, most frequent first.
func (g *Graph) kingSquares() string {
	side := 0
	if g.attackerWhite {
		side = 1
	}
	var hist [64]int
	for i := 0; i < len(g.recs); i += 97 {
		b := g.board(uint32(i))
		hist[bits.TrailingZeros64(b.Pieces[side][0])]++
	}
	type e struct{ sq, n int }
	var top []e
	for sq, n := range hist {
		if n > 0 {
			top = append(top, e{sq, n})
		}
	}
	sort.Slice(top, func(a, b int) bool { return top[a].n > top[b].n })
	out := ""
	for i, t := range top {
		if i == 6 {
			out += " ..."
			break
		}
		out += fmt.Sprintf(" %s:%d", chess.Pos(t.sq), t.n)
	}
	return out
}

func chebyshev(a, b int) int {
	dx, dy := a&7-b&7, a>>3-b>>3
	if dx < 0 {
		dx = -dx
	}
	if dy < 0 {
		dy = -dy
	}
	return max(dx, dy)
}

// expandPartial adds the next k untried policy moves of a defender node
// (generation on the first call); returns false when the store is full.
func (s *searcher) expandPartial(i uint32, k int) bool {
	g := s.g
	b := g.board(i)
	var buf chess.MoveBuffer
	n := b.GenMoves(&buf)
	if g.flags[i]&flExpanded == 0 {
		g.flags[i] |= flExpanded
		g.expansions++
		g.children[i] = make([]uint32, 0, min(n, k))
	}
	moves := buf[:n]
	scores := make([]int, n)
	for j, m := range moves {
		scores[j] = g.policyScore(&b, m)
	}
	order := make([]int, n)
	for j := range order {
		order[j] = j
	}
	sort.SliceStable(order, func(a, c int) bool { return scores[order[a]] > scores[order[c]] })
	added := 0
	for _, j := range order {
		if added >= k {
			break
		}
		c := b
		c.DoMove(moves[j])
		idx, isNew, ok := g.store.GetOrPut(c.Key, uint64(len(g.recs)))
		if !ok {
			s.aborted = true
			return false
		}
		ci := uint32(idx)
		if !isNew && g.hasChild(i, ci) {
			continue
		}
		if isNew {
			g.add(&c)
		}
		g.children[i] = append(g.children[i], ci)
		g.parents[ci] = append(g.parents[ci], i)
		g.edges++
		added++
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
	return true
}

func (g *Graph) hasChild(i, c uint32) bool {
	for _, x := range g.children[i] {
		if x == c {
			return true
		}
	}
	return false
}

// fullyTried: every legal move of the node is a child edge.
func (g *Graph) fullyTried(i uint32) bool {
	return g.flags[i]&flExpanded != 0 && len(g.children[i]) >= int(g.nmoves[i])
}

// solveCertificate answers one question with the certificate builder.
func solveCertificate(root *bitboard.Board, sideToMoveAttacks bool, opt Options) Result {
	maxPositions := opt.MaxPositions
	if maxPositions <= 0 {
		maxPositions = 1 << 26
	}
	slots := 1024
	for slots*ttstore.MaxLoadPercent/100 < maxPositions {
		slots *= 2
	}
	g := &Graph{attackerWhite: root.WhiteMove == sideToMoveAttacks, store: ttstore.NewSlots(slots), defenderConfigs: map[uint64]uint32{}}
	side := 0
	if g.attackerWhite {
		side = 1
	}
	g.rootDefender = root.Pieces[side]
	g.store.Put(root.Key, 0)
	g.add(root)
	s := &searcher{g: g, opt: opt, start: time.Now(), capN: DefaultNumberCap}
	if opt.ProgressEvery <= 0 {
		s.opt.ProgressEvery = 1 << 20
	}
	var r Result
	frontier := []uint32{0}
	for round := 1; !s.aborted; round++ {
		r.Rounds = round
		s.closure(frontier)
		if s.aborted {
			break
		}
		g.safeSet()
		if g.AttackerWon(0) || g.status[0] == stSafe {
			break
		}
		if opt.Stop != nil && opt.Stop.Load() {
			s.aborted = true
			break
		}
		frontier = g.unsafeDefenders()
		won, safe := 0, 0
		for i := range g.status {
			if g.AttackerWon(uint32(i)) {
				won++
			} else if g.status[i] == stSafe {
				safe++
			}
		}
		s.report(fmt.Sprintf("cert round %d: %d positions, %d expansions, %d edges, %d won, %d safe, %d defender nodes widened, %s", round, len(g.recs), g.expansions, g.edges, won, safe, len(frontier), fmtDur(time.Since(s.start))))
		if len(frontier) == 0 {
			s.aborted, s.stuck = true, true // cannot happen: an unsafe root has an unsafe defender with untried moves below it
			break
		}
	}
	r.Aborted, r.Stuck = s.aborted, s.stuck
	switch {
	case g.AttackerWon(0):
		r.Outcome = Proven
	case g.status[0] == stSafe:
		r.Outcome = Disproven
		r.Safe = g.reachableSafe()
	}
	for i := range g.status {
		if g.AttackerWon(uint32(i)) {
			r.Won++
		}
	}
	r.Positions, r.Expansions, r.Edges, r.Visits, r.SafeChecks = len(g.recs), g.expansions, g.edges, g.visits, g.safeChecks
	if r.Outcome == Proven {
		g.levels()
		r.WinPlies = int(g.level[0])
		r.PV = g.line(0)
	}
	return r
}

// closure expands breadth-first from the frontier until no open node is
// left: attacker nodes completely, defender nodes with one policy move
// (one more when they are in the frontier again). A node is queued once
// per wave (flRegion), otherwise a defender reached through several
// attackers in one wave would get one move per arrival.
func (s *searcher) closure(frontier []uint32) {
	g := s.g
	queue := frontier
	push := func(next []uint32, c uint32) []uint32 {
		if g.flags[c]&flRegion != 0 || g.status[c] != stUnknown {
			return next
		}
		g.flags[c] |= flRegion
		return append(next, c)
	}
	for len(queue) > 0 && !s.aborted {
		var next []uint32
		for _, i := range queue {
			g.flags[i] &^= flRegion
			if g.status[i] != stUnknown {
				continue
			}
			before := len(g.recs)
			if g.attackerAt(i) {
				if g.flags[i]&flExpanded != 0 {
					continue
				}
				s.expand(i)
				if g.expansions&(1<<16-1) == 0 {
					s.report(fmt.Sprintf("cert closure: %d positions, %d expansions, %d edges, %d defender placements (%s), %s", len(g.recs), g.expansions, g.edges, len(g.defenderConfigs), g.kingSquares(), fmtDur(time.Since(s.start))))
				}
			} else {
				if !s.expandPartial(i, 1) {
					break
				}
			}
			for idx := before; idx < len(g.recs); idx++ {
				next = push(next, uint32(idx))
			}
			// an old child reached again that is still open and unexpanded
			for _, c := range g.children[i] {
				if g.flags[c]&flExpanded == 0 {
					next = push(next, c)
				}
			}
		}
		queue = next
	}
	for _, i := range queue {
		g.flags[i] &^= flRegion
	}
}

// unsafeDefenders collects the defender nodes that still have untried
// moves, reachable from the root through unsafe open nodes.
func (g *Graph) unsafeDefenders() []uint32 {
	var out []uint32
	stack := []uint32{0}
	var marked []uint32
	g.flags[0] |= flRegion
	marked = append(marked, 0)
	for len(stack) > 0 {
		i := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		if !g.attackerAt(i) && !g.fullyTried(i) {
			out = append(out, i)
		}
		for _, c := range g.children[i] {
			if g.flags[c]&flRegion != 0 || g.status[c] != stUnknown {
				continue
			}
			g.flags[c] |= flRegion
			marked = append(marked, c)
			stack = append(stack, c)
		}
	}
	for _, i := range marked {
		g.flags[i] &^= flRegion
	}
	return out
}
