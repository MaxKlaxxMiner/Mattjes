// Package matelist is the list-based mate search: breadth-first enumeration of
// every position reachable from the root (deduplicated, each expanded exactly
// once), then retrograde analysis over that finite graph. Working title from the
// design document.
//
// Where the depth-first search with iterative deepening re-searches the same
// positions once per depth (KBN-K: 192 million expansions for 13 million
// positions), this touches every position once forwards and every edge once
// backwards. The result is the exact mate distance for every reachable
// position, not only for the root, so the principal variation is complete. For
// few pieces this is already the endgame table of milestone 5, restricted to the
// positions reachable from the root and keyed by hash instead of by index.
//
// Memory: a 32-byte record per position, 16 bytes per store slot, 4 bytes per
// edge (parent lists) plus a few bytes of counters per position.
package matelist

import (
	"fmt"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/ttstore"
)

// Result of Solve. MatePlies is the mate distance of the root in plies for the
// side to move (0 = no forced mate within the enumerated graph). PV is the
// complete principal variation.
type Result struct {
	MatePlies int
	PV        []chess.Move
	Positions int // enumerated positions
	Expanded  int // positions whose moves were generated (all but the horizon layer)
	Edges     int
	Plies     int // breadth-first depth reached
	Resolved  int // positions with a known mate distance for either side
	MaxLevel  int // longest mate distance found anywhere in the graph
	// ProofPositions and ProofEdges measure one proof DAG of the root's mate: the
	// attacker plays one shortest move per position, the defender every move. No
	// search can prove the mate with fewer positions than the smallest such DAG,
	// so this bounds what a smarter forward search could save against brute force.
	ProofPositions, ProofEdges int
}

// Progress receives one line per phase and per breadth-first ply.
type Progress func(line string)

// status per position: 0 unknown, 1..127 side to move wins (mates) in n plies,
// 128|n side to move loses (is mated) in n plies. Mate itself is lose in 0.
const (
	unknown  = 0
	loseFlag = 128
)

func winIn(n int) uint8  { return uint8(n) }
func loseIn(n int) uint8 { return loseFlag | uint8(n) }

// Solve enumerates the positions reachable from root within maxPlies (every
// position once, whichever ply reaches it first), gives up with an error beyond
// maxPositions, and resolves mate distances backwards. The horizon layer is not
// expanded; positions whose proof would need it stay unknown, which is safe.
func Solve(root *bitboard.Board, maxPlies, maxPositions int, progress Progress) (Result, error) {
	report := func(line string) {
		if progress != nil {
			progress(line)
		}
	}
	slots := 1024
	for slots*ttstore.MaxLoadPercent/100 < maxPositions {
		slots *= 2
	}
	store := ttstore.NewSlots(slots)

	recs := make([][bitboard.PackedFixedBytes]byte, 0, 1<<16)
	status := make([]uint8, 0, 1<<16)
	childCount := make([]uint8, 0, 1<<16)   // legal moves of the position (0 at the horizon or when there are none)
	parentCount := make([]uint32, 0, 1<<16) // edges arriving at the position
	childStart := make([]uint32, 0, 1<<16)  // CSR offsets into children; positions are expanded in index order
	children := make([]uint32, 0, 1<<20)    // child index per edge, in expansion order
	add := func(b *bitboard.Board) uint32 {
		recs = append(recs, b.PackedFixedRecord())
		status = append(status, unknown)
		childCount = append(childCount, 0)
		parentCount = append(parentCount, 0)
		return uint32(len(recs) - 1)
	}
	store.Put(root.Key, 0)
	add(root)

	// --- forward: breadth-first enumeration, remembering the child edges ---
	var buf chess.MoveBuffer
	var mates []uint32 // positions where the side to move is mated (level 0)
	level := []uint32{0}
	plies := 0
	for ; plies < maxPlies && len(level) > 0; plies++ {
		var next []uint32
		for _, i := range level {
			childStart = append(childStart, uint32(len(children)))
			b, _ := bitboard.DecodePackedFixed(recs[i][:])
			n := b.GenMoves(&buf)
			childCount[i] = uint8(n)
			if n == 0 {
				if b.InCheck() {
					status[i] = loseIn(0)
					mates = append(mates, i)
				}
				continue
			}
			for j := 0; j < n; j++ {
				child := b
				child.DoMove(buf[j])
				idx, isNew, ok := store.GetOrPut(child.Key, uint64(len(recs)))
				if !ok {
					return Result{}, fmt.Errorf("more than %d positions at ply %d", maxPositions, plies+1)
				}
				if isNew {
					add(&child)
					next = append(next, uint32(idx))
				}
				parentCount[idx]++
				children = append(children, uint32(idx))
			}
		}
		report(fmt.Sprintf("ply %3d: %12d new positions, %12d total, %12d edges, %d mates so far", plies+1, len(next), len(recs), len(children), len(mates)))
		level = next
	}
	edges := len(children)
	expanded := len(childStart) // the last level (horizon or empty) was not expanded
	childStart = append(childStart, uint32(edges))
	if len(level) > 0 {
		report(fmt.Sprintf("horizon at ply %d: %d positions not expanded", plies, len(level)))
	}

	// --- parent lists (CSR) by counting sort of the child edges, no generator, no hash ---
	parentStart := make([]uint32, len(recs)+1)
	for i, c := range parentCount {
		parentStart[i+1] = parentStart[i] + c
	}
	parents := make([]uint32, edges)
	fill := parentCount // reuse as cursor
	for i := range fill {
		fill[i] = 0
	}
	for p := 0; p < expanded; p++ {
		for _, c := range children[childStart[p]:childStart[p+1]] {
			parents[parentStart[c]+fill[c]] = uint32(p)
			fill[c]++
		}
	}
	children, childStart = nil, nil // no longer needed; the retrograde walks parents only
	report(fmt.Sprintf("parent lists: %d edges for %d positions", edges, len(recs)))

	// --- backward: retrograde levels ---
	remaining := childCount // open children per position; a position is lost when all children are won by the opponent
	queue := mates
	resolved := len(mates)
	maxLevel := 0
	for n := 0; len(queue) > 0; n++ {
		var next []uint32
		if n%2 == 0 {
			// queue holds "lose in n": every parent that is still unknown wins in n+1
			for _, c := range queue {
				for _, p := range parents[parentStart[c]:parentStart[c+1]] {
					if status[p] == unknown {
						status[p] = winIn(n + 1)
						next = append(next, p)
					}
				}
			}
		} else {
			// queue holds "win in n": a parent loses in n+1 once all its children are won
			for _, c := range queue {
				for _, p := range parents[parentStart[c]:parentStart[c+1]] {
					remaining[p]--
					if remaining[p] == 0 && status[p] == unknown {
						status[p] = loseIn(n + 1)
						next = append(next, p)
					}
				}
			}
		}
		if len(next) > 0 {
			maxLevel = n + 1
			resolved += len(next)
		}
		queue = next
	}
	report(fmt.Sprintf("retrograde: %d of %d positions resolved, longest mate %d plies", resolved, len(recs), maxLevel))

	r := Result{Positions: len(recs), Expanded: expanded, Edges: edges, Plies: plies, Resolved: resolved, MaxLevel: maxLevel}
	if s := status[0]; s != unknown && s&loseFlag == 0 {
		r.MatePlies = int(s)
		r.PV = principalVariation(root, store, status)
		r.ProofPositions, r.ProofEdges = proofSize(recs, store, status)
		report(fmt.Sprintf("proof DAG: %d positions, %d edges (attacker one shortest move, defender all moves)", r.ProofPositions, r.ProofEdges))
	}
	return r, nil
}

// proofSize walks one proof DAG of the root's mate breadth-first: at a winning
// position the first child that loses in n-1 is taken, at a losing position all
// children. Returns the number of distinct positions and edges in it.
func proofSize(recs [][bitboard.PackedFixedBytes]byte, store *ttstore.Store, status []uint8) (int, int) {
	inProof := make([]bool, len(recs))
	inProof[0] = true
	queue := []uint32{0}
	positions, edges := 1, 0
	var buf chess.MoveBuffer
	for len(queue) > 0 {
		var next []uint32
		for _, i := range queue {
			s := status[i]
			if s == loseIn(0) {
				continue
			}
			b, _ := bitboard.DecodePackedFixed(recs[i][:])
			n := b.GenMoves(&buf)
			attacker := s&loseFlag == 0
			want := loseIn(int(s) - 1)
			for j := 0; j < n; j++ {
				child := b
				child.DoMove(buf[j])
				idx, _ := store.Get(child.Key)
				if attacker && status[idx] != want {
					continue
				}
				edges++
				if !inProof[idx] {
					inProof[idx] = true
					positions++
					next = append(next, uint32(idx))
				}
				if attacker {
					break
				}
			}
		}
		queue = next
	}
	return positions, edges
}

// principalVariation follows the mate distances from the root: the winner picks
// a child that loses in n-1, the loser a child that wins in n-1.
func principalVariation(root *bitboard.Board, store *ttstore.Store, status []uint8) []chess.Move {
	var pv []chess.Move
	b := *root
	cur := uint32(0)
	var buf chess.MoveBuffer
	for {
		s := status[cur]
		if s == unknown || s == loseIn(0) {
			return pv
		}
		var want uint8
		if s&loseFlag == 0 {
			want = loseIn(int(s) - 1)
		} else {
			want = winIn(int(s&^loseFlag) - 1)
		}
		n := b.GenMoves(&buf)
		found := false
		for j := 0; j < n; j++ {
			child := b
			child.DoMove(buf[j])
			idx, ok := store.Get(child.Key)
			if ok && status[idx] == want {
				pv = append(pv, buf[j])
				b = child
				cur = uint32(idx)
				found = true
				break
			}
		}
		if !found {
			return pv
		}
	}
}
