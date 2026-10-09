package uci

// Stage c of the UCI milestone: positions the tables do not settle go to one
// of the mate searches of milestone 4, running in its own goroutine with the
// tables as oracle. The main loop keeps reading commands; "stop" (or the time
// budget) sets a flag the search polls, the goroutine unwinds and reports
// the last completed depth. The search goroutine never touches the engine's
// set (the main loop registers and loads tables), it only reads it through
// the oracle and prints its progress through the shared writer.

import (
	"fmt"
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

// searchEnd is what the search goroutine hands back: the mate it found (0 =
// none), the deepest completed depth and the counters. The main loop prints
// the final line, because extending the line through the tables may load
// tables, which only the main loop does.
type searchEnd struct {
	matePlies int
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
}

// rootScore renders a root move's score: a proven mate by its length, or
// by the depth as upper bound when the proof tree lost the length; cp 0
// ("no mate within the depth") otherwise.
func rootScore(rm chess.RootMove, depth int) string {
	if rm.Proven {
		n := rm.MatePlies
		if n == 0 {
			n = depth
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

// depth reports a completed depth without a mate: the first MultiPV root
// moves best first, each as "no mate within depth" (score cp 0) with the
// move as its line, so the GUI's list stays complete.
func (r *reporter) depth(depth int, nodes uint64, root []chess.RootMove) {
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
// examination, at most every few seconds.
func (r *reporter) progress(depth int, nodes uint64, current chess.Move) {
	if time.Since(r.last) < progressInterval {
		return
	}
	r.last = time.Now()
	cur := ""
	if current != (chess.Move{}) {
		cur = " currmove " + current.UCI()
	}
	r.e.send("info depth %d%s %s", depth, cur, r.counters(nodes))
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
			r = runMateab(&root, maxPlies, oracle, abTable, stop, rep)
		case "matepn":
			r = runMatepn(&root, maxPlies, oracle, direct, stop, rep)
		case "matelist":
			r = runMatelist(&root, maxPlies, maxPositions, stop, rep)
		}
		r.elapsed = time.Since(rep.start)
		r.aborted = stop.Load()
		e.searchDone <- r
	}()
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
	if r.matePlies > 0 && len(r.root) > 0 {
		// the first MultiPV root moves: proven ones with their lines continued
		// through the tables, the rest as cp 0 (no mate found behind them)
		counters := fmt.Sprintf("nodes %d nps %d time %d", r.nodes, nps(r.nodes, ms), ms)
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
			e.send("info depth %d multipv %d score %s %s pv %s", r.matePlies, i+1, rootScore(rm, r.matePlies), counters, rootPV(chess.RootMove{Move: rm.Move, PV: pv}))
		}
		e.pendingBest = r.root[0].Move.UCI()
	} else if r.nodes > 0 || r.note == "" {
		what := "no mate"
		if r.aborted {
			what = "stopped, no mate"
		}
		e.send("info string search: %s within %d plies, %s nodes in %.1f s", what, r.plies, egtb.Group(int(r.nodes)), float64(ms)/1000)
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

// directTable returns the direct-mapped table for matepn, allocating it with
// the current Hash size (the bucket table is dropped: one table at a time).
func (e *Engine) directTable() *tt.Table {
	if e.direct == nil || e.tableMB != e.hashMB {
		e.buckets = nil
		e.direct = tt.New(e.hashMB)
		e.tableMB = e.hashMB
	}
	return e.direct
}

// bucketTable returns the bucket table for mateab.
func (e *Engine) bucketTable() *tt.Buckets {
	if e.buckets == nil || e.tableMB != e.hashMB {
		e.direct = nil
		e.buckets = tt.NewBuckets(e.hashMB)
		e.tableMB = e.hashMB
	}
	return e.buckets
}

// runMateab: depth-first with iterative deepening (mateab); the bucket table
// keeps proven mates over refutations (a direct table below bucketsFromMB).
func runMateab(root *bitboard.Board, maxPlies int, oracle mateab.Oracle, table mateab.Table, stop *atomic.Bool, rep *reporter) searchEnd {
	s := mateab.New(oracle, table)
	s.Stop, s.ProgressEvery = stop, progressEvery
	var base uint64
	depth := 1
	s.Progress = func(nodes uint64) { rep.progress(depth, base+nodes, s.CurrentMove()) }
	r := s.Solve(root, maxPlies, func(plies int, r mateab.Result) {
		base, depth = r.Nodes, plies+2
		if r.MatePlies == 0 {
			rep.depth(plies, r.Nodes, r.Root)
		}
	})
	return searchEnd{matePlies: r.MatePlies, root: r.Root, plies: r.Plies, nodes: r.Nodes}
}

// runMatepn: df-pn with iterative deepening (matepn, the measured best
// settings: mobility, epsilon 1/2, final entries), direct-mapped table.
func runMatepn(root *bitboard.Board, maxPlies int, oracle mateab.Oracle, table *tt.Table, stop *atomic.Bool, rep *reporter) searchEnd {
	s := matepn.New(oracle, table, matepn.Saturating{Bits: table.ValueBits()}, true)
	s.Epsilon, s.Final, s.Stop, s.ProgressEvery = 4, true, stop, progressEvery
	var base uint64
	depth := 1
	s.Progress = func(nodes uint64) { rep.progress(depth, base+nodes, s.CurrentMove()) }
	r := s.SolveShortest(root, maxPlies, func(plies int, r matepn.Result) {
		base, depth = r.Nodes, plies+2
		if !r.Proven {
			rep.depth(plies, r.Nodes, r.Root)
		}
	})
	end := searchEnd{root: r.Root, plies: r.Plies, nodes: r.Nodes}
	if r.Proven {
		end.matePlies = r.MatePlies
		if end.matePlies == 0 {
			// proven within the depth, but the proof tree lost entries: the
			// depth is an upper bound of the mate length
			end.matePlies = r.Plies
			end.note = "proof tree incomplete (table entries replaced), the mate is at most that long"
		}
	}
	return end
}

// runMatelist: breadth-first enumeration plus retrograde analysis
// (matelist); the Hash size bounds the positions, a stop ends the
// enumeration at the next ply and resolves what was reached.
func runMatelist(root *bitboard.Board, maxPlies, maxPositions int, stop *atomic.Bool, rep *reporter) searchEnd {
	r, err := matelist.Solve(root, maxPlies, maxPositions, stop, func(line string) { rep.text("matelist: " + line) })
	if err != nil {
		return searchEnd{note: err.Error() + " (raise Hash)"}
	}
	return searchEnd{matePlies: r.MatePlies, root: r.Root, plies: r.Plies, nodes: uint64(r.Positions)}
}
