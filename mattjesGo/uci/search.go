package uci

// Stage c of the UCI milestone: positions the tables do not settle go to one
// of the mate searches of milestone 4, running in its own goroutine with the
// tables as oracle. The main loop keeps reading commands; "stop" (or the time
// budget) sets a flag the search polls, the goroutine unwinds and reports
// the last completed depth. The search goroutine never touches the engine's
// set (the main loop registers and loads tables), it only reads it through
// the oracle and prints its progress through the shared writer.
//
// The root is driven here, not inside the searchers: every root move is
// solved on its own (the position after it as the search root, with the
// opponent's mate and the opponent being mated as the two questions), depth
// by depth, so each move's first proof is its shortest mate and a MultiPV
// list holds exact values, the way an alpha-beta engine searches every root
// move for MultiPV. A search that stopped at the root's first success or
// first escape would leave the other moves with bounds only.

import (
	"fmt"
	"sort"
	"strconv"
	"strings"
	"sync/atomic"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mateab"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/matelist"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/matepn"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/tt"
)

const (
	// defaultAlgo is the Search option's default: df-pn proves mates with the
	// fewest nodes (docs/m4-mate-search.md).
	defaultAlgo = "matepn"
	// defaultHashMB is the Hash option's default (the transposition table of
	// mateab and matepn, the position budget of matelist); minHashMB is what
	// mateab's 21 value bits need from a direct-mapped table (2^21 slots).
	defaultHashMB = 256
	minHashMB     = 32
	// bucketsFromMB: below this mateab uses the direct-mapped table, because
	// buckets have two value bits less than a direct table of the same size.
	bucketsFromMB = 128
	// maxSearchPlies bounds the iterative deepening: mate in 64 at most
	// (matepn.MaxPlies; mateab.MaxPly is one more).
	maxSearchPlies = matepn.MaxPlies
	// matelistBytesPerPosition turns the Hash size into matelist's position
	// budget (record, store slot, edges and counters, measured 110 to 130).
	matelistBytesPerPosition = 128
	// progressEvery is how often (in nodes) a search offers a progress line;
	// the reporter then throttles by time. The searchers' own default (2^22
	// and 2^26) is minutes apart at 200,000 nodes per second.
	progressEvery = 1 << 16
)

func isAlgo(s string) bool {
	switch strings.ToLower(s) {
	case "none", "mateab", "matepn", "matelist":
		return true
	}
	return false
}

// goParams is what "go" asks for: infinite, a depth bound in plies (mate N =
// 2N-1 plies, depth N rounded up to odd) and a time budget (movetime, or a
// twentieth of the remaining time plus half the increment).
type goParams struct {
	infinite bool
	maxPlies int
	budget   time.Duration
}

func (e *Engine) parseGo(words []string) goParams {
	p := goParams{maxPlies: maxSearchPlies}
	num := func(i int) int {
		if i+1 < len(words) {
			if n, err := strconv.Atoi(words[i+1]); err == nil {
				return n
			}
		}
		return 0
	}
	var timeLeft, inc, movetime int // milliseconds
	for i, w := range words {
		switch w {
		case "infinite":
			p.infinite = true
		case "movetime":
			movetime = num(i)
		case "wtime":
			if e.board.WhiteMove {
				timeLeft = num(i)
			}
		case "btime":
			if !e.board.WhiteMove {
				timeLeft = num(i)
			}
		case "winc":
			if e.board.WhiteMove {
				inc = num(i)
			}
		case "binc":
			if !e.board.WhiteMove {
				inc = num(i)
			}
		case "mate":
			if n := num(i); n > 0 {
				p.maxPlies = min(maxSearchPlies, 2*n-1)
			}
		case "depth":
			if n := num(i); n > 0 {
				p.maxPlies = min(maxSearchPlies, n|1)
			}
		}
	}
	switch {
	case p.infinite:
	case movetime > 0:
		p.budget = time.Duration(movetime) * time.Millisecond
	case timeLeft > 0:
		p.budget = time.Duration(max(50, timeLeft/20+inc/2)) * time.Millisecond
	}
	return p
}

// searchEnd is what the search goroutine hands back: the root moves best
// first (root[0] is the headline), the deepest completed depth and the
// counters. The main loop prints the final block, because extending the
// lines through the tables may load tables, which only the main loop does.
type searchEnd struct {
	matePlies int              // positive: the engine mates, negative: it is mated, 0: nothing found
	root      []chess.RootMove // best first, root[0] is the headline
	plies     int
	nodes     uint64
	elapsed   time.Duration
	aborted   bool
	note      string
}

// reporter prints the per-depth and progress lines of a running search.
type reporter struct {
	e       *Engine
	start   time.Time
	last    time.Time
	multiPV int
	// the root move under examination (set by the root driver) and the nodes
	// of the finished root moves, for the progress lines
	current chess.Move
	base    uint64
	depth   int
}

// rootScore renders a root move's score: a proven mate by its length
// (positive plies: the engine mates, negative: it is mated), or by the
// signed depth as upper bound when the proof tree lost the length; cp 0
// ("no mate within the depth") otherwise.
func rootScore(rm chess.RootMove, depth int) string {
	if rm.Proven {
		n := rm.MatePlies
		if n == 0 {
			n = depth
		}
		if n < 0 {
			return fmt.Sprintf("mate -%d", -n/2)
		}
		return fmt.Sprintf("mate %d", (n+1)/2)
	}
	return "cp 0"
}

func rootPV(rm chess.RootMove) string {
	if len(rm.PV) > 0 {
		return pvString(rm.PV)
	}
	return rm.Move.UCI()
}

func (r *reporter) counters(nodes uint64) string {
	ms := time.Since(r.start).Milliseconds()
	return fmt.Sprintf("nodes %d nps %d time %d", nodes, nps(nodes, ms), ms)
}

// depthLines reports a completed depth: the first MultiPV root moves best
// first, proven ones with their mate, the rest as cp 0 ("no mate within
// the depth"), so the GUI's list stays complete.
func (r *reporter) depthLines(depth int, nodes uint64, root []chess.RootMove) {
	r.last = time.Now()
	if len(root) == 0 {
		r.e.send("info depth %d score cp 0 %s", depth, r.counters(nodes))
	}
	for i := 0; i < len(root) && i < r.multiPV; i++ {
		r.e.send("info depth %d multipv %d score %s %s pv %s", depth, i+1, rootScore(root[i], depth), r.counters(nodes), rootPV(root[i]))
	}
	r.e.flush()
}

// progress reports the node count inside a depth and the root move under
// examination, at most every few seconds (called by the searchers with
// their node count of the current solve).
func (r *reporter) progress(nodes uint64) {
	if time.Since(r.last) < progressInterval {
		return
	}
	r.last = time.Now()
	cur := ""
	if r.current != (chess.Move{}) {
		cur = " currmove " + r.current.UCI()
	}
	r.e.send("info depth %d%s %s", r.depth, cur, r.counters(r.base+nodes))
	r.e.flush()
}

// text reports a status line of a phase-based search, at most every few seconds.
func (r *reporter) text(line string) {
	if time.Since(r.last) < progressInterval {
		return
	}
	r.last = time.Now()
	r.e.send("info string %s", compact(line))
	r.e.flush()
}

func nps(nodes uint64, ms int64) uint64 {
	return nodes * 1000 / uint64(max(1, ms))
}

// startSearch runs the configured algorithm on the current position in a
// goroutine; the result arrives on searchDone.
func (e *Engine) startSearch(p goParams) {
	stop := new(atomic.Bool)
	e.stop, e.searching = stop, true
	if p.budget > 0 {
		e.timer = time.AfterFunc(p.budget, func() { stop.Store(true) })
	}
	root, algo, maxPlies, multiPV := e.board, e.algo, p.maxPlies, e.multiPV
	oracle := mateab.Tables{Set: e.set}
	var direct *tt.Table
	var abTable mateab.Table
	switch algo {
	case "matepn":
		direct = e.directTable()
	case "mateab":
		if e.hashMB >= bucketsFromMB {
			abTable = e.bucketTable()
		} else {
			abTable = e.directTable()
		}
	}
	maxPositions := e.hashMB << 20 / matelistBytesPerPosition
	go func() {
		rep := &reporter{e: e, start: time.Now(), multiPV: multiPV}
		rep.last = rep.start
		var r searchEnd
		switch algo {
		case "mateab":
			s := mateab.New(oracle, abTable)
			s.Stop, s.ProgressEvery, s.Progress = stop, progressEvery, rep.progress
			r = runRoot(&root, maxPlies, multiPV, rep, func(c *bitboard.Board, plies int) childResult {
				cr := s.SolveDepth(c, plies)
				return childResult{proven: cr.MatePlies != 0, length: max(cr.MatePlies, -cr.MatePlies), pv: cr.PV, nodes: cr.Nodes, aborted: cr.Aborted}
			})
		case "matepn":
			s := matepn.New(oracle, direct, matepn.Saturating{Bits: direct.ValueBits()}, true)
			s.Epsilon, s.Final, s.Stop, s.ProgressEvery, s.Progress = 4, true, stop, progressEvery, rep.progress
			r = runRoot(&root, maxPlies, multiPV, rep, func(c *bitboard.Board, plies int) childResult {
				cr := s.Solve(c, plies)
				// proven with MatePlies 0: the proof tree lost entries, the
				// depth is an upper bound of the length
				return childResult{proven: cr.Proven, length: max(cr.MatePlies, -cr.MatePlies), bound: cr.Proven && cr.MatePlies == 0, pv: cr.PV, nodes: cr.Nodes, aborted: cr.Aborted}
			})
		case "matelist":
			r = runMatelist(&root, maxPlies, maxPositions, stop, rep)
		}
		r.elapsed = time.Since(rep.start)
		r.aborted = stop.Load()
		e.searchDone <- r
	}()
}

// childResult is one root move's answer at one depth from the searchers:
// proven = the question of the depth holds (odd plies: the side to move at
// the child mates within plies, even: it is mated), length = the plies of
// that mate along the proof (bound = only the depth is known), pv = the line
// from the child.
type childResult struct {
	proven  bool
	length  int
	bound   bool
	pv      []chess.Move
	nodes   uint64
	aborted bool
}

// rootEntry is the root driver's state of one move.
type rootEntry struct {
	rm   chess.RootMove
	cost uint64 // nodes of the last unresolved depth, the ordering of the open moves
}

// runRoot drives the search per root move: depth by depth, every unresolved
// move's child position is solved as its own root with depth-1 plies (even
// depths: the opponent is mated, our mate; odd: the opponent mates, our
// loss). A move's first proof is its shortest mate. Reports every depth;
// ends when all moves are resolved, when the shortest win is known and only
// one line is wanted, at the depth bound or when stopped.
func runRoot(root *bitboard.Board, maxPlies, multiPV int, rep *reporter, solve func(c *bitboard.Board, plies int) childResult) searchEnd {
	var buf chess.MoveBuffer
	n := root.GenMoves(&buf)
	moves := make([]rootEntry, n)
	for i := range moves {
		moves[i].rm.Move = buf[i]
	}
	var nodes uint64
	end := searchEnd{}
	for d := 1; d <= maxPlies; d++ {
		rep.depth = d
		for i := range moves {
			m := &moves[i]
			if m.rm.Proven {
				continue
			}
			c := *root
			c.DoMove(m.rm.Move)
			var res childResult
			if d == 1 {
				// our mate in one: the child has no moves and is in check
				var cbuf chess.MoveBuffer
				if c.GenMoves(&cbuf) == 0 && c.InCheck() {
					res.proven = true
				}
			} else {
				rep.current, rep.base = m.rm.Move, nodes
				res = solve(&c, d-1)
			}
			nodes += res.nodes
			if res.aborted {
				end.aborted = true
				return end.finish(moves, nodes)
			}
			m.cost = res.nodes
			if !res.proven {
				continue
			}
			// odd child depth: the opponent mates, our loss; even: our win
			length := res.length + 1
			if res.bound {
				length = d
			}
			if (d-1)%2 == 1 {
				length = -length
			}
			m.rm.Proven, m.rm.MatePlies = true, length
			m.rm.PV = append([]chess.Move{m.rm.Move}, res.pv...)
			if res.bound {
				end.note = "proof tree incomplete (table entries replaced), a mate length is an upper bound"
			}
		}
		end.plies = d
		list := orderRoot(moves)
		rep.depthLines(d, nodes, list)
		allDone := true
		for _, m := range moves {
			allDone = allDone && m.rm.Proven
		}
		if allDone || (multiPV == 1 && len(list) > 0 && list[0].MatePlies > 0) {
			break
		}
	}
	return end.finish(moves, nodes)
}

// finish fills the result from the driver's state: the headline is a win
// when one is proven (the shortest), a loss when every move loses (the
// longest), nothing otherwise.
func (end searchEnd) finish(moves []rootEntry, nodes uint64) searchEnd {
	end.root, end.nodes = orderRoot(moves), nodes
	if len(end.root) > 0 {
		first := end.root[0]
		if first.Proven && (first.MatePlies > 0 || first.MatePlies < 0 && end.root[len(end.root)-1].Proven) {
			end.matePlies = first.MatePlies
		}
	}
	return end
}

// orderRoot sorts the root moves best first: wins by the shortest, open
// moves by the nodes their last depth cost (the hardest to refute first),
// losses by the longest.
func orderRoot(moves []rootEntry) []chess.RootMove {
	sorted := append([]rootEntry(nil), moves...)
	class := func(m rootEntry) int {
		switch {
		case m.rm.Proven && m.rm.MatePlies > 0:
			return 0
		case m.rm.Proven:
			return 2
		}
		return 1
	}
	sort.SliceStable(sorted, func(i, j int) bool {
		a, b := sorted[i], sorted[j]
		if ca, cb := class(a), class(b); ca != cb {
			return ca < cb
		}
		if a.rm.Proven {
			return a.rm.MatePlies < b.rm.MatePlies // shortest win, longest loss
		}
		return a.cost > b.cost
	})
	out := make([]chess.RootMove, len(sorted))
	for i, m := range sorted {
		out[i] = m.rm
	}
	return out
}

// stopSearch ends a running search and handles its result (which sends the
// best move unless a "go infinite" holds it back).
func (e *Engine) stopSearch() {
	if !e.searching {
		return
	}
	e.stop.Store(true)
	e.finishSearch(<-e.searchDone)
}

// finishSearch prints the result of a search and sends the best move (held
// back during a "go infinite"). A found mate is reported with its line,
// continued through the tables where the search ended in a table position.
func (e *Engine) finishSearch(r searchEnd) {
	if !e.searching {
		return
	}
	e.searching = false
	if e.timer != nil {
		e.timer.Stop()
		e.timer = nil
	}
	ms := r.elapsed.Milliseconds()
	if r.matePlies != 0 && len(r.root) > 0 {
		// the first MultiPV root moves: proven ones with their lines continued
		// through the tables, the rest as cp 0 (no mate found behind them);
		// a negative distance means the engine is mated, the list then holds
		// the defences longest first
		counters := fmt.Sprintf("nodes %d nps %d time %d", r.nodes, nps(r.nodes, ms), ms)
		depth := max(r.matePlies, -r.matePlies)
		for i, rm := range r.root {
			if i >= e.multiPV {
				break
			}
			pv := rm.PV
			if rm.Proven {
				if len(pv) == 0 {
					pv = []chess.Move{rm.Move}
				}
				pv = e.extendPV(pv)
			}
			e.send("info depth %d multipv %d score %s %s pv %s", depth, i+1, rootScore(rm, r.matePlies), counters, rootPV(chess.RootMove{Move: rm.Move, PV: pv}))
		}
		e.pendingBest = r.root[0].Move.UCI()
	} else if r.nodes > 0 || r.note == "" {
		what := "no mate"
		if r.aborted {
			what = "stopped, no mate"
		}
		e.send("info string search: %s within %d plies, %s nodes in %.1f s", what, r.plies, egtb.Group(int(r.nodes)), float64(ms)/1000)
		if !e.tableBest && len(r.root) > 0 {
			e.pendingBest = r.root[0].Move.UCI() // the best open move: avoids the moves proved lost
		}
	}
	if r.note != "" {
		e.send("info string search: %s", r.note)
	}
	if !e.infinite {
		e.send("bestmove %s", e.pendingBest)
	}
}

// extendPV plays the search's line and, where it ends in a position the
// tables know, appends the optimal play to mate.
func (e *Engine) extendPV(pv []chess.Move) []chess.Move {
	b := e.board
	for _, m := range pv {
		b.DoMove(m)
	}
	e.loadMaterial(&b)
	if v, ok := e.eval(&b, 1); ok && (v.IsWin() || v.IsLoss()) {
		pv = append(append([]chess.Move(nil), pv...), e.line(b, v)...)
	}
	return pv
}

func pvString(pv []chess.Move) string {
	parts := make([]string, len(pv))
	for i, m := range pv {
		parts[i] = m.UCI()
	}
	return strings.Join(parts, " ")
}

// directTable returns the direct-mapped table for the current algorithm,
// allocating it with the current Hash size (the bucket table is dropped: one
// table at a time). Entries of another algorithm are cleared: the value
// layouts differ, matepn would read mateab's entries as proof numbers.
func (e *Engine) directTable() *tt.Table {
	if e.direct == nil || e.tableMB != e.hashMB {
		e.buckets = nil
		e.direct = tt.New(e.hashMB)
		e.tableMB = e.hashMB
	} else if e.tableAlgo != e.algo {
		e.direct.Clear()
	}
	e.tableAlgo = e.algo
	return e.direct
}

// bucketTable returns the bucket table for mateab.
func (e *Engine) bucketTable() *tt.Buckets {
	if e.buckets == nil || e.tableMB != e.hashMB {
		e.direct = nil
		e.buckets = tt.NewBuckets(e.hashMB)
		e.tableMB = e.hashMB
	} else if e.tableAlgo != e.algo {
		e.buckets.Clear()
	}
	e.tableAlgo = e.algo
	return e.buckets
}

// runMatelist: breadth-first enumeration plus retrograde analysis
// (matelist); the Hash size bounds the positions, a stop ends the
// enumeration at the next ply and resolves what was reached. The graph
// knows every root move exactly, so its list is used as it is.
func runMatelist(root *bitboard.Board, maxPlies, maxPositions int, stop *atomic.Bool, rep *reporter) searchEnd {
	r, err := matelist.Solve(root, maxPlies, maxPositions, stop, func(line string) { rep.text("matelist: " + line) })
	if err != nil {
		return searchEnd{note: err.Error() + " (raise Hash)"}
	}
	return searchEnd{matePlies: r.MatePlies, root: r.Root, plies: r.Plies, nodes: uint64(r.Positions)}
}
