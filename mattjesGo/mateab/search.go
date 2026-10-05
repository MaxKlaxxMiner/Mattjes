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
package mateab

import (
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// MaxPly bounds the search depth in plies.
const MaxPly = 128

// NoMate is returned by the node functions when no forced mate exists within the
// remaining depth.
const NoMate = -1

// Result of one depth of Solve. MatePlies is the mate distance in plies (odd,
// attacker to move at the root), 0 if no mate was found within the depth.
type Result struct {
	MatePlies int
	PV        []chess.Move
	Nodes     uint64
}

// Searcher holds the per-search state: the oracle, node counter, principal
// variation (triangular table) and one killer move per ply, the move that most
// recently mated or refuted at that ply and is tried first.
type Searcher struct {
	oracle Oracle
	nodes  uint64
	pv     [MaxPly][MaxPly]chess.Move
	pvLen  [MaxPly]int
	killer [MaxPly]chess.Move
}

// New returns a searcher that asks oracle at every node.
func New(oracle Oracle) *Searcher {
	return &Searcher{oracle: oracle}
}

// Solve looks for a forced mate for the side to move within maxPlies plies, with
// iterative deepening over odd depths. report is called after every depth; the
// returned Result is the first depth that found a mate (or the last depth, with
// MatePlies 0). Nodes accumulate over all depths.
func (s *Searcher) Solve(root *bitboard.Board, maxPlies int, report func(plies int, r Result)) Result {
	s.killer = [MaxPly]chess.Move{}
	var total uint64
	var last Result
	for d := 1; d <= maxPlies && d < MaxPly; d += 2 {
		s.nodes = 0
		dist := s.attack(root, d, 0)
		total += s.nodes
		last = Result{Nodes: total}
		if dist != NoMate {
			last.MatePlies = dist
			last.PV = append([]chess.Move(nil), s.pv[0][:s.pvLen[0]]...)
		}
		if report != nil {
			report(d, last)
		}
		if dist != NoMate {
			break
		}
	}
	return last
}

// attack is an OR node: the attacker is to move with depth plies left (odd).
// Returns the mate distance in plies or NoMate. At depth 1 only checking moves
// are tried, because a mating move is always a check.
func (s *Searcher) attack(b *bitboard.Board, depth, ply int) int {
	s.nodes++
	if v, _ := s.oracle.Probe(b); v == Draw || v == Loss {
		return NoMate
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
				return 1
			}
		}
		return NoMate
	}
	n := b.GenMoves(&buf)
	s.orderAttack(b, buf[:n], ply)
	for i := 0; i < n; i++ {
		child := *b
		child.DoMove(buf[i])
		if r := s.defend(&child, depth-1, ply+1); r != NoMate {
			s.setPV(ply, buf[i])
			s.killer[ply] = buf[i]
			return r + 1
		}
	}
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
	if v, _ := s.oracle.Probe(b); v == Draw || v == Win {
		return NoMate
	}
	s.orderDefend(buf[:n], ply)
	worst := 0
	for i := 0; i < n; i++ {
		child := *b
		child.DoMove(buf[i])
		r := s.attack(&child, depth-1, ply+1)
		if r == NoMate {
			s.killer[ply] = buf[i]
			return NoMate
		}
		if r > worst || i == 0 {
			worst = r
			s.setPV(ply, buf[i])
		}
	}
	return worst + 1
}

// setPV records m as the move at ply and appends the child's line.
func (s *Searcher) setPV(ply int, m chess.Move) {
	s.pv[ply][0] = m
	n := copy(s.pv[ply][1:], s.pv[ply+1][:s.pvLen[ply+1]])
	s.pvLen[ply] = n + 1
}

// orderAttack sorts the attacker's moves: killer, checks, captures, the rest.
func (s *Searcher) orderAttack(b *bitboard.Board, moves []chess.Move, ply int) {
	var tmp chess.MoveBuffer
	k := 0
	killer := s.killer[ply]
	for _, m := range moves {
		if m == killer {
			tmp[k] = m
			k++
		}
	}
	for _, m := range moves {
		if m != killer && b.GivesCheck(m) {
			tmp[k] = m
			k++
		}
	}
	for _, m := range moves {
		if m != killer && m.Capture != chess.None && !b.GivesCheck(m) {
			tmp[k] = m
			k++
		}
	}
	for _, m := range moves {
		if m != killer && m.Capture == chess.None && !b.GivesCheck(m) {
			tmp[k] = m
			k++
		}
	}
	copy(moves, tmp[:k])
}

// orderDefend moves the killer (the move that refuted most recently at this ply)
// to the front, then captures.
func (s *Searcher) orderDefend(moves []chess.Move, ply int) {
	var tmp chess.MoveBuffer
	k := 0
	killer := s.killer[ply]
	for _, m := range moves {
		if m == killer {
			tmp[k] = m
			k++
		}
	}
	for _, m := range moves {
		if m != killer && m.Capture != chess.None {
			tmp[k] = m
			k++
		}
	}
	for _, m := range moves {
		if m != killer && m.Capture == chess.None {
			tmp[k] = m
			k++
		}
	}
	copy(moves, tmp[:k])
}
