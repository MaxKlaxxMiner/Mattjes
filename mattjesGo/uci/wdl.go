package uci

import (
	"fmt"
	"sort"
	"sync/atomic"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
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
	positions int
	method    string
}

// wdlBytesPerPosition turns the Hash size into the position budget of one
// graph (record, store slot, edges, counters and the second searcher's
// graph, roughly); the author ignores memory for now, so this is generous.
const wdlBytesPerPosition = 64

// runWdl drives the WDL search per root move: known table values are
// taken as they are, every other move gets the two questions. The list is
// reported after every decision; a stop ends the search with what is
// decided.
func runWdl(root *bitboard.Board, multiPV int, rep *reporter, known []rootMove, stop *atomic.Bool, maxPositions int) searchEnd {
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
	opt := wdllist.Options{Epsilon: 8, MaxPositions: maxPositions, Stop: stop, Progress: progress, ProgressEvery: 1 << 22}
	decided := n - len(open)
	rep.wdlLines(decided, entries, positions, nil)
	for _, i := range open {
		if stop.Load() {
			break
		}
		m := &entries[i]
		c := *root
		c.DoMove(m.move)
		rep.current, rep.currentNo = m.move, i+1
		// we win? (the root is a defender node of the child: the opponent
		// to move, we attack)
		r := wdllist.Solve(&c, false, opt)
		positions += uint64(r.Positions)
		m.positions += r.Positions
		switch r.Outcome {
		case wdllist.Proven:
			m.class, m.pv, m.method = wdlWin, append([]chess.Move{m.move}, r.PV...), r.Method
		case wdllist.Disproven:
			m.class, m.method = wdlNoWin, r.Method
		}
		if r.Aborted || m.class == wdlWin {
			if r.Aborted {
				end.aborted = true
				break
			}
			decided++
			rep.wdlLines(decided, entries, positions, nil)
			continue
		}
		// the opponent wins? (the root is an attacker node of the child)
		r = wdllist.Solve(&c, true, opt)
		positions += uint64(r.Positions)
		m.positions += r.Positions
		switch r.Outcome {
		case wdllist.Proven:
			m.class, m.pv, m.method = wdlLoss, append([]chess.Move{m.move}, r.PV...), r.Method
			decided++
		case wdllist.Disproven:
			if m.class == wdlNoWin {
				m.class = wdlDraw
				decided++
			} else {
				m.class = wdlNoLoss
			}
			m.method = r.Method
		}
		if r.Aborted {
			end.aborted = true
			break
		}
		rep.wdlLines(decided, entries, positions, nil)
	}
	end.wdl = orderWdl(entries)
	end.nodes = positions
	end.answered = !end.aborted
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
