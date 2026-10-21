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
	switch v, _, _ := g.oracle.Probe(&c); v {
	case mateab.Draw:
		return 100000 // dead material, or a table draw
	case mateab.Loss:
		return 100000 // the attacker to move loses by the table
	case mateab.Win:
		return -100000 // the attacker to move wins by the table
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
		if c.Squares[am.To] != chess.None && !recapturable(&c, am) {
			s -= 2000 // the attacker wins material for free
		}
	}
	s -= 10 * n // the attacker's mobility: quiet positions make small certificates
	if c.InCheck() {
		// a check leaves the attacker only king moves, captures and blocks:
		// the certificate of a perpetual check is the loop of checks, and
		// it must beat the glue and stay-put terms below
		s += 1500
	}
	if b.Squares[m.To] != chess.None {
		s += 300 // the defender captures
	}
	s -= 300 * g.displacement(&c)
	s += 400 * g.gluedPieces(&c)
	s += int(min(g.defenderConfigs[g.defenderConfig(&c)], 200))
	return s
}

// gluedPieces counts the defender's pieces other than the king that stand
// next to the king: a piece the king defends cannot be won for free, and
// a defender that keeps its pieces together has few placements to cover.
func (g *Graph) gluedPieces(b *bitboard.Board) int {
	side := 0
	if g.attackerWhite {
		side = 1
	}
	king := bits.TrailingZeros64(b.Pieces[side][0])
	n := 0
	for kind := 1; kind < 6; kind++ {
		for pieces := b.Pieces[side][kind]; pieces != 0; pieces &= pieces - 1 {
			if chebyshev(bits.TrailingZeros64(pieces), king) == 1 {
				n++
			}
		}
	}
	return n
}

// recapturable: after the capture m on board b, the side then to move can
// take back on the same square (a one-ply exchange check).
func recapturable(b *bitboard.Board, m chess.Move) bool {
	c := *b
	c.DoMove(m)
	var buf chess.MoveBuffer
	n := c.GenMoves(&buf)
	for _, r := range buf[:n] {
		if r.To == m.To {
			return true
		}
	}
	return false
}

// displacement is the Chebyshev distance of the defender's king from its
// root square (the other pieces follow the king, see gluedPieces).
func (g *Graph) displacement(b *bitboard.Board) int {
	side := 0
	if g.attackerWhite {
		side = 1
	}
	return chebyshev(bits.TrailingZeros64(b.Pieces[side][0]), bits.TrailingZeros64(g.rootDefender[0]))
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
// (generation on the first call); returns false when the position budget
// is used up (checked before the node is touched, as in expand).
func (s *searcher) expandPartial(i uint32, k int) bool {
	g := s.g
	if g.full() {
		s.aborted = true
		return false
	}
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
		ci, isNew, ok := g.getOrPut(c.Key)
		if !ok {
			s.aborted = true
			return false
		}
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

// newCertSearcher sets up the certificate builder of a question.
func newCertSearcher(root *bitboard.Board, sideToMoveAttacks bool, opt Options) *searcher {
	g := &Graph{attackerWhite: root.WhiteMove == sideToMoveAttacks, defenderConfigs: map[uint64]uint32{}, unsafeLevel: []uint16{}, oracle: oracleOf(opt)}
	side := 0
	if g.attackerWhite {
		side = 1
	}
	g.rootDefender = root.Pieces[side]
	g.setBudget(opt.MaxPositions)
	g.store = ttstore.NewSlots(min(initialSlots, g.maxSlots))
	g.store.Put(root.Key, 0)
	g.add(root)
	s := &searcher{g: g, opt: opt, start: time.Now(), capN: DefaultNumberCap, frontier: []uint32{0}}
	if opt.ProgressEvery <= 0 {
		s.opt.ProgressEvery = 1 << 20
	}
	return s
}

// openUnexpanded collects the open nodes without children reachable from
// the root through open nodes: the frontier of a resumed run, whose
// interrupted closure lost its queue (the nodes it had added are in the
// graph, partially expanded defenders keep their move).
func (g *Graph) openUnexpanded(from uint32) []uint32 {
	var out []uint32
	stack := []uint32{from}
	marked := []uint32{from}
	g.flags[from] |= flRegion
	for len(stack) > 0 {
		i := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		if g.flags[i]&flExpanded == 0 {
			out = append(out, i)
			continue
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

// runCert continues the certificate builder up to maxPositions.
func (s *searcher) runCert(q *bitboard.Board, extra int) Result {
	g := s.g
	opt := s.opt
	s.begin(extra)
	var r Result
	n, ok := s.locate(q)
	if !ok {
		r.Aborted, r.Positions = true, len(g.recs)
		return r
	}
	// the stay-put policy measures displacement from the question node
	side := 0
	if g.attackerWhite {
		side = 1
	}
	g.rootDefender = q.Pieces[side]
	// the queue of an interrupted closure is gone (and another question
	// may have expanded nodes below this one): start from the open nodes
	// without children below the question node
	s.frontier = g.openUnexpanded(n)
	for !s.aborted {
		s.rounds++
		s.closure(s.frontier)
		if s.aborted {
			break
		}
		g.safeSet()
		if g.AttackerWon(n) || g.defenderHolds(n) {
			break
		}
		if opt.Stop != nil && opt.Stop.Load() {
			s.aborted = true
			break
		}
		s.frontier = g.unsafeDefenders(n)
		won, safe := 0, 0
		for i := range g.status {
			if g.AttackerWon(uint32(i)) {
				won++
			} else if g.status[i] == stSafe {
				safe++
			}
		}
		s.report(fmt.Sprintf("cert round %d: %d positions, %d expansions, %d edges, %d won, %d safe, %d defender nodes widened, %s", s.rounds, len(g.recs), g.expansions, g.edges, won, safe, len(s.frontier), fmtDur(time.Since(s.start))))
		if len(s.frontier) == 0 {
			s.aborted, s.stuck = true, true // cannot happen: an unsafe root has an unsafe defender with untried moves below it
			break
		}
	}
	r.Rounds = s.rounds
	r.Aborted, r.Stuck = s.aborted, s.stuck
	g.fillResult(&r, n)
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
	var next []uint32
	for len(queue) > 0 && !s.aborted {
		next = nil
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
				if s.aborted {
					break
				}
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
		if s.aborted {
			break // the rest of the queue stays marked until below
		}
		queue = next
	}
	// an interrupted wave leaves its queue and the next one marked
	for _, i := range queue {
		g.flags[i] &^= flRegion
	}
	for _, i := range next {
		g.flags[i] &^= flRegion
	}
}

// unsafeDefenders collects the defender nodes that still have untried
// moves, reachable from the root through unsafe open nodes, and keeps
// those closest to the leak (unsafeLevel within two of the smallest): a
// leak is plugged where it springs, the nodes above it become safe with
// it, and widening them all would multiply the graph by the number of
// rounds.
func (g *Graph) unsafeDefenders(from uint32) []uint32 {
	var all []uint32
	stack := []uint32{from}
	var marked []uint32
	g.flags[from] |= flRegion
	marked = append(marked, from)
	minLevel := uint16(1<<16 - 1)
	for len(stack) > 0 {
		i := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		if !g.attackerAt(i) && !g.fullyTried(i) {
			all = append(all, i)
			minLevel = min(minLevel, g.unsafeLevel[i])
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
	var out []uint32
	for _, i := range all {
		if g.unsafeLevel[i] <= minLevel+2 {
			out = append(out, i)
		}
	}
	return out
}
