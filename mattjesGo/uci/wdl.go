package uci

import (
	"fmt"
	"math/bits"
	"sort"
	"sync/atomic"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mateab"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/wdllist"
)

// The WDL search (Search fastwdl, milestone 7): every root move is asked
// two questions on the explicit graph (wdllist, the proof-number descent
// racing the certificate builder): "does the opponent win after it" (a
// proof is our loss) and "do we win after it" (a proof is our win); both
// refuted is a guaranteed draw. No mate distance, no fifty-move rule.
// Scores (the author's choice): 9999 win, -9999 loss, 0 draw, 1 unknown,
// 10 "cannot lose, might win", -10 "cannot win, might lose".

// wdlClass is a root move's state, best first.
type wdlClass int

const (
	wdlWin     wdlClass = iota // proven win
	wdlNoLoss                  // the opponent cannot win, our win is open
	wdlDraw                    // proven draw
	wdlUnknown                 // nothing decided
	wdlNoWin                   // we cannot win, the opponent's win is open
	wdlLoss                    // proven loss
)

func (c wdlClass) score() string {
	switch c {
	case wdlWin:
		return "cp 9999"
	case wdlNoLoss:
		return "cp 10"
	case wdlDraw:
		return "cp 0"
	case wdlNoWin:
		return "cp -10"
	case wdlLoss:
		return "cp -9999"
	}
	return "cp 1"
}

// wdlEntry is one root move's result.
type wdlEntry struct {
	move      chess.Move
	class     wdlClass
	pv        []chess.Move // the winning line (ours or the opponent's), the move first
	positions int          // positions the shared graphs grew by while this move was asked
	method    string
}

// wdlMaterial is the piece value sum of a side without the king (queen 9,
// rook 5, bishop and knight 3, pawn 1), to ask the weaker side's question
// first: refuting the win of a lone knight is a small certificate, proving
// the win of two rooks against it is deep, and the cheap answer ("we
// cannot lose", cp 10) should not wait behind the expensive one.
func wdlMaterial(b *bitboard.Board, white bool) int {
	side := 1
	if white {
		side = 0
	}
	values := [6]int{0, 9, 5, 3, 3, 1} // kind order K, Q, R, B, N, P
	sum := 0
	for kind, v := range values {
		sum += v * bits.OnesCount64(b.Pieces[side][kind])
	}
	return sum
}

// wdlBytesPerPosition is the measured memory of a graph position (record,
// key, store slot, edges with their slice headers and growth, counters:
// 240 to 255 bytes on KQ-KP with 4 million positions, 2026-10-22), and
// wdlGraphs the number of graphs the search holds (two questions, each
// racing the descent against the certificate builder). Hash is the hard
// cap over all of them: Hash·2^20 / (wdlGraphs·wdlBytesPerPosition)
// positions per graph, so Hash 16384 allows 16 Mi positions per graph.
const (
	wdlBytesPerPosition = 256
	wdlGraphs           = 4
)

// wdlFirstBudget is the position budget of the first round (per question).
const wdlFirstBudget = 1 << 16

// wdlBudgetGrowth multiplies the budget from round to round; the rounds
// before the last one cost a third of it in total.
const wdlBudgetGrowth = 4

// runWdl drives the WDL search over the root moves in rounds: known table
// values are taken as they are, every other move gets the two questions
// with a position budget per graph that grows by wdlBudgetGrowth per round
// until it reaches the Hash budget, so that no single move eats the whole
// time before the others are even looked at. The questions are wdllist
// Solvers that keep their graphs between the rounds (no restart, the
// memory is the price: every open question holds two graphs). The list is
// reported after every decision; the search ends when every move is
// decided, when the top multiPV moves are proven wins (nothing can rank
// above them), when the full budget left moves open, or on stop.
func runWdl(root *bitboard.Board, multiPV int, rep *reporter, known []rootMove, stop *atomic.Bool, maxPositions int, oracle mateab.Oracle) searchEnd {
	var buf chess.MoveBuffer
	n := root.GenMoves(&buf)
	entries := make([]wdlEntry, n)
	var open []int
	for i := range entries {
		entries[i] = wdlEntry{move: buf[i], class: wdlUnknown}
		settled := false
		for _, k := range known {
			if k.move != buf[i] {
				continue
			}
			settled = true
			entries[i].method = "table"
			switch {
			case k.value.IsWin():
				entries[i].class, entries[i].pv = wdlWin, k.pv
			case k.value.IsLoss():
				entries[i].class, entries[i].pv = wdlLoss, k.pv
			default:
				entries[i].class = wdlDraw
			}
		}
		if !settled {
			open = append(open, i)
		}
	}
	end := searchEnd{}
	var positions uint64
	last := time.Now()
	progress := func(line string) {
		if time.Since(last) < progressInterval {
			return
		}
		last = time.Now()
		rep.text("wdl: " + line)
	}
	opt := wdllist.Options{Epsilon: 8, MaxPositions: maxPositions, Stop: stop, Progress: progress, ProgressEvery: 1 << 22, Oracle: oracle}
	// two shared graph pairs for all root moves: 0 "we win after the
	// move" (we attack), 1 "the opponent wins after it"
	solvers := [2]*wdllist.Solver{wdllist.NewSolver(root, true, opt), wdllist.NewSolver(root, false, opt)}
	counted := 0 // positions of all graphs booked so far
	decided := n - len(open)
	wins := 0
	for _, en := range entries {
		if en.class == wdlWin {
			wins++
		}
	}
	rep.wdlLines(decided, entries, positions, nil)
	// the weaker side's question first (wdlMaterial): 0 = we win, 1 = the
	// opponent wins
	order := [2]int{0, 1}
	if wdlMaterial(root, !root.WhiteMove) < wdlMaterial(root, root.WhiteMove) {
		order = [2]int{1, 0}
	}
	budget := min(wdlFirstBudget, maxPositions)
	for round := 1; len(open) > 0 && wins < multiPV && !end.aborted; round++ {
		rep.note(fmt.Sprintf("wdl: round %d, %d new positions per graph and question, %d moves open, %d positions so far", round, budget, len(open), positions))
		stillOpen := open[:0]
		roundStart := counted
		for _, i := range open {
			if stop.Load() {
				end.aborted = true
				break
			}
			m := &entries[i]
			rep.current, rep.currentNo = m.move, i+1
			before := m.class
			// asks question q for the move with this round's increment and
			// books what the shared graphs grew by
			ask := func(q int) wdllist.Result {
				r := solvers[q].RunMove(m.move, budget)
				total := solvers[0].Positions() + solvers[1].Positions()
				positions += uint64(total - counted)
				m.positions += total - counted
				counted = total
				return r
			}
			for _, q := range order {
				if q == 0 && (m.class == wdlUnknown || m.class == wdlNoLoss) {
					// we win? (the root is a defender node of the child: the
					// opponent to move, we attack)
					r := ask(0)
					switch r.Outcome {
					case wdllist.Proven:
						m.class, m.pv, m.method = wdlWin, append([]chess.Move{m.move}, r.PV...), r.Method
						wins++
					case wdllist.Disproven:
						if m.class == wdlNoLoss {
							m.class = wdlDraw
						} else {
							m.class = wdlNoWin
						}
						m.method = r.Method
					}
					if r.Aborted && stop.Load() {
						end.aborted = true
						break
					}
				} else if q == 1 && (m.class == wdlUnknown || m.class == wdlNoWin) {
					// the opponent wins? (the root is an attacker node of the child)
					r := ask(1)
					switch r.Outcome {
					case wdllist.Proven:
						m.class, m.pv, m.method = wdlLoss, append([]chess.Move{m.move}, r.PV...), r.Method
					case wdllist.Disproven:
						if m.class == wdlNoWin {
							m.class = wdlDraw
						} else {
							m.class = wdlNoLoss
						}
						m.method = r.Method
					}
					if r.Aborted && stop.Load() {
						end.aborted = true
						break
					}
				}
			}
			if end.aborted {
				break
			}
			switch m.class {
			case wdlWin, wdlDraw, wdlLoss:
				decided++
			default:
				stillOpen = append(stillOpen, i)
			}
			if m.class != before {
				rep.wdlLines(decided, entries, positions, nil)
			}
			if wins >= multiPV {
				break
			}
		}
		if end.aborted {
			break
		}
		progressed := counted > roundStart || len(stillOpen) < len(open)
		open = stillOpen
		if !progressed {
			break // the graphs are full and nothing was decided: the rest stays open
		}
		budget = min(budget*wdlBudgetGrowth, maxPositions)
	}
	end.wdl = orderWdl(entries)
	end.nodes = positions
	// answered only when every move is decided: full graphs end the search
	// with open moves, and a background generation must go on for them
	end.answered = decided == n
	return end
}

// orderWdl sorts best first: wins, no-loss, draws, unknown, no-win, losses;
// within a class by the positions it cost (the hardest first), so that the
// open moves the search sweated over lead.
func orderWdl(entries []wdlEntry) []wdlEntry {
	out := append([]wdlEntry(nil), entries...)
	sort.SliceStable(out, func(i, j int) bool {
		if out[i].class != out[j].class {
			return out[i].class < out[j].class
		}
		return out[i].positions > out[j].positions
	})
	return out
}

// wdlLines prints the MultiPV block of the WDL search: one depth per block
// (the number of decided moves, so that GUIs keep the lines), the score of
// the class, the winning line as the graph has it (the final block in
// finishSearch continues it through the tables; the search thread must not
// touch the tables).
func (r *reporter) wdlLines(decided int, entries []wdlEntry, positions uint64, extend func([]chess.Move) []chess.Move) {
	list := orderWdl(entries)
	ms := time.Since(r.start).Milliseconds()
	for i, en := range list {
		if i >= r.multiPV {
			break
		}
		pv := en.pv
		if len(pv) == 0 {
			pv = []chess.Move{en.move}
		}
		if extend != nil && (en.class == wdlWin || en.class == wdlLoss) {
			pv = extend(pv)
		}
		r.e.send("info depth %d multipv %d score %s nodes %d nps %d time %d pv %s", max(decided, 1), i+1, en.class.score(), positions, nps(positions, ms), ms, pvString(pv))
	}
	r.e.flush()
}

// wdlSummary is the closing info string of a WDL search.
func wdlSummary(entries []wdlEntry) string {
	var counts [6]int
	for _, en := range entries {
		counts[en.class]++
	}
	return fmt.Sprintf("wdl: %d win, %d cannot lose, %d draw, %d unknown, %d cannot win, %d loss", counts[wdlWin], counts[wdlNoLoss], counts[wdlDraw], counts[wdlUnknown], counts[wdlNoWin], counts[wdlLoss])
}
