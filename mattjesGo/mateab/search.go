// Package mateab is the first mate search: depth-first with a "mate window" and
// iterative deepening, built as the simple, exact reference that the later
// searches are checked against. Working title from the design document.
//
// The tree is an AND/OR tree. At an attacker node (OR) one move that leads to
// mate is enough, so the node returns at the first success. At a defender node
// (AND) one move that escapes is enough to refute, so the node returns at the
// first escape. There is no evaluation, only "mate in n plies" or "no mate within
// the remaining depth". Iterative deepening over 1, 3, 5, ... plies makes the
// first mate found the shortest one.
//
// With a transposition table (ttvalue.go) finished nodes are remembered: a
// proven mate is exact and depth independent, a refutation holds for every
// remaining depth up to the one searched, and the stored move is tried first.
package mateab

import (
	"sort"
	"sync/atomic"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// MaxPly bounds the search depth in plies (the TT depth field holds 7 bits).
const MaxPly = 128

// NoMate is returned by the node functions when no forced mate exists within the
// remaining depth.
const NoMate = -1

// Result of one depth of Solve. MatePlies is the mate distance in plies (odd,
// attacker to move at the root), 0 if no mate was found within the depth. PV is
// the principal variation; with a table it may be shorter than MatePlies when
// the entries needed to extend it were replaced.
type Result struct {
	MatePlies  int
	PV         []chess.Move
	Nodes      uint64
	TTHits     uint64 // probes that ended the node (mate or refutation taken from the table)
	OracleHits uint64 // nodes decided by the oracle with an exact distance (endgame tables)
	Plies      int    // the deepest completed depth
	Aborted    bool   // Stop was set: the result is that of the last completed depth
	// Root lists the root moves best first: the mating move, then the
	// refuted ones by the nodes their refutation cost (the most promising
	// first), then the moves the depth never reached.
	Root []chess.RootMove
}

// Searcher holds the per-search state: the oracle, the optional table, node
// counters, principal variation (triangular table) and one killer move per ply,
// the move that most recently mated or refuted at that ply and is tried first.
type Searcher struct {
	oracle     Oracle
	table      Table
	nodes      uint64
	ttHits     uint64
	oracleHits uint64
	pv         [MaxPly][MaxPly]chess.Move
	pvLen      [MaxPly]int
	killer     [MaxPly]chess.Move

	// Progress, if set, is called every ProgressEvery nodes of a depth (a
	// power of two, ProgressNodes unless changed), for long searches.
	Progress      func(nodes uint64)
	ProgressEvery uint64
	// Stop, if set, is polled every StopNodes nodes: once true, the search
	// unwinds without storing anything and Solve returns the last completed
	// depth with Aborted set (UCI "stop" and time limits).
	Stop    *atomic.Bool
	aborted bool
	// the root move under examination (for progress lines) and the nodes
	// each refuted root move cost in the current depth
	current   chess.Move
	rootCosts []rootCost
}

type rootCost struct {
	move chess.Move
	cost uint64
}

// CurrentMove is the root move the search is examining right now.
func (s *Searcher) CurrentMove() chess.Move { return s.current }

// ProgressNodes is the interval of Progress calls (a power of two).
const ProgressNodes = 1 << 26

// StopNodes is the polling interval of Stop (a power of two).
const StopNodes = 1 << 12

// New returns a searcher that asks oracle at every node and uses table (nil for none).
func New(oracle Oracle, table Table) *Searcher {
	return &Searcher{oracle: oracle, table: table, ProgressEvery: ProgressNodes}
}

// Solve looks for a forced mate for the side to move within maxPlies plies, with
// iterative deepening over odd depths. report is called after every depth; the
// returned Result is the first depth that found a mate (or the last depth, with
// MatePlies 0). Nodes accumulate over all depths.
func (s *Searcher) Solve(root *bitboard.Board, maxPlies int, report func(plies int, r Result)) Result {
	s.killer = [MaxPly]chess.Move{}
	s.aborted = false
	var totalNodes, totalHits, totalOracle uint64
	var last Result
	for d := 1; d <= maxPlies && d < MaxPly; d += 2 {
		s.nodes, s.ttHits, s.oracleHits = 0, 0, 0
		s.rootCosts = s.rootCosts[:0]
		dist := s.attack(root, d, 0)
		totalNodes += s.nodes
		totalHits += s.ttHits
		totalOracle += s.oracleHits
		if s.aborted {
			last.Nodes, last.TTHits, last.OracleHits, last.Aborted = totalNodes, totalHits, totalOracle, true
			break
		}
		last = Result{Nodes: totalNodes, TTHits: totalHits, OracleHits: totalOracle, Plies: d}
		if dist != NoMate {
			last.MatePlies = dist
			last.PV = s.extendPV(root, dist)
		}
		last.Root = s.rootMoves(root, last.PV, dist)
		if report != nil {
			report(d, last)
		}
		if dist != NoMate {
			break
		}
	}
	return last
}

// rootMoves lists the root moves best first: the mating move with its line,
// then the refuted moves by the nodes their refutation cost (a hard
// refutation is the best hint at a promising move), then the moves the
// depth never reached (the first success ended it) in generation order.
func (s *Searcher) rootMoves(root *bitboard.Board, pv []chess.Move, dist int) []chess.RootMove {
	var out []chess.RootMove
	if dist != NoMate && len(pv) > 0 {
		out = append(out, chess.RootMove{Move: pv[0], Proven: true, MatePlies: dist, PV: pv})
	}
	costs := append([]rootCost(nil), s.rootCosts...)
	sort.SliceStable(costs, func(i, j int) bool { return costs[i].cost > costs[j].cost })
	seen := map[chess.Move]bool{}
	for _, e := range out {
		seen[e.Move] = true
	}
	for _, c := range costs {
		if !seen[c.move] {
			seen[c.move] = true
			out = append(out, chess.RootMove{Move: c.move})
		}
	}
	var buf chess.MoveBuffer
	n := root.GenMoves(&buf)
	for _, m := range buf[:n] {
		if !seen[m] {
			out = append(out, chess.RootMove{Move: m})
		}
	}
	return out
}

// checkStop polls Stop every StopNodes nodes.
func (s *Searcher) checkStop() {
	if s.nodes&(StopNodes-1) == 0 && s.Stop != nil && s.Stop.Load() {
		s.aborted = true
	}
}

// attack is an OR node: the attacker is to move with depth plies left (odd).
// Returns the mate distance in plies or NoMate. At depth 1 only checking moves
// are tried, because a mating move is always a check.
func (s *Searcher) attack(b *bitboard.Board, depth, ply int) int {
	s.nodes++
	if s.nodes&(s.ProgressEvery-1) == 0 && s.Progress != nil {
		s.Progress(s.nodes)
	}
	s.checkStop()
	if s.aborted {
		return NoMate
	}
	switch v, plies := s.oracle.Probe(b); {
	case v == Draw || v == Loss:
		return NoMate
	case v == Win && plies > 0: // an endgame table knows the exact distance
		s.oracleHits++
		if plies > depth {
			return NoMate
		}
		s.pvLen[ply] = 0
		return plies
	}
	var ttFrom, ttTo chess.Pos = -1, -1
	if s.table != nil {
		if v, ok := s.table.Probe(b.Key); ok {
			typ, d, from, to := unpackValue(v)
			if typ == ttMate && d <= depth {
				s.ttHits++
				s.pvLen[ply] = 0 // the line continues in the table, see extendPV
				return d
			}
			if typ == ttNoMate && d >= depth {
				s.ttHits++
				return NoMate
			}
			ttFrom, ttTo = from, to
		}
	}
	var buf chess.MoveBuffer
	if depth == 1 {
		n := b.GenChecks(&buf)
		for i := 0; i < n; i++ {
			child := *b
			child.DoMove(buf[i])
			s.nodes++
			if !child.HasMoves() {
				s.pv[ply][0] = buf[i]
				s.pvLen[ply] = 1
				s.killer[ply] = buf[i]
				s.store(b, ttMate, 1, buf[i])
				return 1
			}
		}
		s.store(b, ttNoMate, 1, chess.Move{})
		return NoMate
	}
	n := b.GenMoves(&buf)
	s.orderAttack(b, buf[:n], ply, ttFrom, ttTo)
	for i := 0; i < n; i++ {
		child := *b
		child.DoMove(buf[i])
		before := s.nodes
		if ply == 0 {
			s.current = buf[i]
		}
		r := s.defend(&child, depth-1, ply+1)
		if s.aborted {
			return NoMate // nothing is stored on the way out
		}
		if ply == 0 {
			s.rootCosts = append(s.rootCosts, rootCost{buf[i], s.nodes - before})
		}
		if r != NoMate {
			s.setPV(ply, buf[i])
			s.killer[ply] = buf[i]
			s.store(b, ttMate, r+1, buf[i])
			return r + 1
		}
	}
	s.store(b, ttNoMate, depth, chess.Move{})
	return NoMate
}

// defend is an AND node: the defender is to move with depth plies left (even,
// at least 2). Every defender move must lead to mate; the longest of these
// mates is the distance. The first escape refutes the node.
func (s *Searcher) defend(b *bitboard.Board, depth, ply int) int {
	s.nodes++
	var buf chess.MoveBuffer
	n := b.GenMoves(&buf)
	if n == 0 {
		if b.InCheck() {
			s.pvLen[ply] = 0
			return 0 // mated already (the attacker's previous move mated before the last ply)
		}
		return NoMate // stalemate
	}
	switch v, plies := s.oracle.Probe(b); {
	case v == Draw || v == Win:
		return NoMate
	case v == Loss && plies > 0: // the defender is mated in plies, known exactly
		s.oracleHits++
		if plies > depth {
			return NoMate
		}
		s.pvLen[ply] = 0
		return plies
	}
	var ttFrom, ttTo chess.Pos = -1, -1
	if s.table != nil {
		if v, ok := s.table.Probe(b.Key); ok {
			typ, d, from, to := unpackValue(v)
			if typ == ttMate && d <= depth {
				s.ttHits++
				s.pvLen[ply] = 0
				return d
			}
			if typ == ttNoMate && d >= depth {
				s.ttHits++
				return NoMate
			}
			ttFrom, ttTo = from, to
		}
	}
	s.orderDefend(buf[:n], ply, ttFrom, ttTo)
	worst := 0
	var worstMove chess.Move
	for i := 0; i < n; i++ {
		child := *b
		child.DoMove(buf[i])
		r := s.attack(&child, depth-1, ply+1)
		if s.aborted {
			return NoMate
		}
		if r == NoMate {
			s.killer[ply] = buf[i]
			s.store(b, ttNoMate, depth, buf[i])
			return NoMate
		}
		if r > worst || i == 0 {
			worst = r
			worstMove = buf[i]
			s.setPV(ply, buf[i])
		}
	}
	s.store(b, ttMate, worst+1, worstMove)
	return worst + 1
}

func (s *Searcher) store(b *bitboard.Board, typ, depth int, m chess.Move) {
	if s.table != nil {
		s.table.Store(b.Key, packValue(typ, depth, m))
	}
}

// setPV records m as the move at ply and appends the child's line.
func (s *Searcher) setPV(ply int, m chess.Move) {
	s.pv[ply][0] = m
	n := copy(s.pv[ply][1:], s.pv[ply+1][:s.pvLen[ply+1]])
	s.pvLen[ply] = n + 1
}

// extendPV returns the principal variation of a found mate: the moves from the
// triangular table, then, where a table hit cut the line short, the moves the
// transposition table remembers for each position (mating move at attacker
// nodes, longest defence at defender nodes), until the mate or a missing entry.
func (s *Searcher) extendPV(root *bitboard.Board, matePlies int) []chess.Move {
	pv := append([]chess.Move(nil), s.pv[0][:s.pvLen[0]]...)
	if s.table == nil || len(pv) >= matePlies {
		return pv
	}
	b := *root
	for _, m := range pv {
		b.DoMove(m)
	}
	for len(pv) < matePlies {
		v, ok := s.table.Probe(b.Key)
		if !ok {
			break
		}
		typ, _, from, to := unpackValue(v)
		if typ != ttMate {
			break
		}
		var buf chess.MoveBuffer
		n := b.GenMoves(&buf)
		found := false
		for i := 0; i < n; i++ {
			if sameMove(buf[i], from, to) {
				pv = append(pv, buf[i])
				b.DoMove(buf[i])
				found = true
				break
			}
		}
		if !found {
			break
		}
	}
	return pv
}

// orderAttack sorts the attacker's moves: table move, killer, checks, captures, the rest.
func (s *Searcher) orderAttack(b *bitboard.Board, moves []chess.Move, ply int, ttFrom, ttTo chess.Pos) {
	var tmp chess.MoveBuffer
	k := 0
	killer := s.killer[ply]
	first := func(m chess.Move) bool { return sameMove(m, ttFrom, ttTo) || m == killer }
	for _, m := range moves {
		if sameMove(m, ttFrom, ttTo) {
			tmp[k] = m
			k++
		}
	}
	for _, m := range moves {
		if m == killer && !sameMove(m, ttFrom, ttTo) {
			tmp[k] = m
			k++
		}
	}
	for _, m := range moves {
		if !first(m) && b.GivesCheck(m) {
			tmp[k] = m
			k++
		}
	}
	for _, m := range moves {
		if !first(m) && m.Capture != chess.None && !b.GivesCheck(m) {
			tmp[k] = m
			k++
		}
	}
	for _, m := range moves {
		if !first(m) && m.Capture == chess.None && !b.GivesCheck(m) {
			tmp[k] = m
			k++
		}
	}
	copy(moves, tmp[:k])
}

// orderDefend sorts the defender's moves: table move (the known escape), killer,
// captures, the rest.
func (s *Searcher) orderDefend(moves []chess.Move, ply int, ttFrom, ttTo chess.Pos) {
	var tmp chess.MoveBuffer
	k := 0
	killer := s.killer[ply]
	first := func(m chess.Move) bool { return sameMove(m, ttFrom, ttTo) || m == killer }
	for _, m := range moves {
		if sameMove(m, ttFrom, ttTo) {
			tmp[k] = m
			k++
		}
	}
	for _, m := range moves {
		if m == killer && !sameMove(m, ttFrom, ttTo) {
			tmp[k] = m
			k++
		}
	}
	for _, m := range moves {
		if !first(m) && m.Capture != chess.None {
			tmp[k] = m
			k++
		}
	}
	for _, m := range moves {
		if !first(m) && m.Capture == chess.None {
			tmp[k] = m
			k++
		}
	}
	copy(moves, tmp[:k])
}
