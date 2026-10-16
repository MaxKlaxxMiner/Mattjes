// Package uci speaks the Universal Chess Interface (milestone 6): positions
// covered by the endgame tables are answered straight from them, with the
// exact distance to mate and the complete winning line; every root move gets
// its own value, so MultiPV lists them all. Positions the tables do not settle
// go to one of the mate searches (option Search: mateab, matepn, matelist) in
// its own thread, with the tables as oracle (search.go).
//
// Tables beyond the four-piece base are loaded from the cache directory when
// a file exists. When the options allow it, a missing table is generated in
// the background, but only while a "go infinite" is running: "stop" pauses
// the generation, the next "go infinite" resumes it, and once the table is
// complete the held-back answer is recomputed with it.
//
// Protocol subset: uci, isready, setoption, ucinewgame, position, go
// (infinite, movetime, wtime/btime/winc/binc, mate, depth), stop, quit, d
// (debug: print the FEN).
package uci

import (
	"bufio"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime/debug"
	"sort"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/tt"
)

// Version is reported in "id name"; it follows the milestone that is being
// worked on.
const Version = "0.6"

// About is the banner line printed at startup (unprompted, as most engines do).
const About = "Mattjes " + Version + " by Max Klaxx Miner, a mate and draw search engine with its own endgame tables, GPLv3"

// progressInterval limits the per-level progress of a background generation
// and the node counts of a running search to one line every few seconds.
const progressInterval = 5 * time.Second

// Engine is the state between commands.
type Engine struct {
	out     *bufio.Writer
	outMu   sync.Mutex // the generation and search threads report through the same writer
	set     *egtb.Set
	board   bitboard.Board
	multiPV int
	workers int
	name    string
	// options
	writeCache bool   // EgtbWriteCache: write generated tables to the cache directory
	generate5  bool   // EgtbGenerate5: build missing five-piece tables during go infinite
	generate6  bool   // EgtbGenerate6: the same for six pieces
	cacheDir   string // EgtbPath
	algo       string // Search: the algorithm for positions the tables do not settle
	hashMB     int    // Hash: the transposition table of the search
	mobility   string // MatePnMobility: on (leaves start with their move count), off (1/1), lazy (move count when entered)
	// the transposition tables, allocated on the first search, dropped when a
	// generation starts (the generator needs the memory)
	direct    *tt.Table   // matepn (direct-mapped: proofs must not be evicted by larger numbers)
	buckets   *tt.Buckets // mateab (buckets keep mates over refutations)
	tableMB   int
	tableAlgo string // the algorithm whose entries the table holds (their value layouts differ)
	// a "go infinite" holds the best move back until "stop"
	infinite    bool
	pendingBest string
	tableBest   bool          // pendingBest has a table value (a search without result keeps it)
	rootTable   []rootMove    // the root moves the table stage knows exactly, handed to the search's root driver
	lastGo      goParams      // the parameters of the last "go": a search is repeated with them when a generation finishes beside it
	spare       *egtb.Set     // a fork taken before a search for a generation that the search's demand starts meanwhile (nil = none wanted)
	demandTick  chan struct{} // the search's reporter knocks every few seconds: time to look at the demand for tables
	deadline    time.Time     // when the running timed search ends (the repeat gets the rest)
	restart     bool          // the running search is stopped to be repeated: no result lines, no bestmove
	job         *job
	jobDone     chan *egtb.Set
	// the running search, if any
	searching  bool
	stop       *atomic.Bool
	timer      *time.Timer
	searchDone chan searchEnd
}

// job is a background generation of one material (with its dependencies) in
// a forked set.
type job struct {
	mat  egtb.Material
	ctrl *egtb.Control
}

// Run reads commands from in until quit or EOF. name is the engine name
// reported to the GUI (the Go and Rust binaries differ).
func Run(in io.Reader, out io.Writer, name string, workers int) {
	e := &Engine{out: bufio.NewWriter(out), board: bitboard.New(), multiPV: 1, workers: workers, name: name, cacheDir: egtb.DefaultCacheDir(),
		algo: defaultAlgo, hashMB: defaultHashMB, mobility: defaultMobility, jobDone: make(chan *egtb.Set, 1), searchDone: make(chan searchEnd, 1), demandTick: make(chan struct{}, 1)}
	e.send("%s", About)
	e.flush()
	lines := make(chan string)
	go func() {
		scanner := bufio.NewScanner(in)
		scanner.Buffer(make([]byte, 1<<16), 1<<20)
		for scanner.Scan() {
			lines <- scanner.Text()
		}
		close(lines)
	}()
	for {
		select {
		case line, ok := <-lines:
			if !ok || !e.command(strings.Fields(line)) {
				if e.job != nil {
					e.job.ctrl.Abort()
				}
				if e.searching {
					e.stop.Store(true)
				}
				e.flush()
				return
			}
		case set := <-e.jobDone:
			e.finishJob(set)
		case r := <-e.searchDone:
			e.finishSearch(r)
		case <-e.demandTick:
			e.onDemandTick()
		}
		e.flush()
	}
}

func (e *Engine) send(format string, args ...any) {
	e.outMu.Lock()
	fmt.Fprintf(e.out, format+"\n", args...)
	e.outMu.Unlock()
}

func (e *Engine) flush() {
	e.outMu.Lock()
	e.out.Flush()
	e.outMu.Unlock()
}

// command handles one line; false means quit.
func (e *Engine) command(words []string) bool {
	if len(words) == 0 {
		return true
	}
	switch words[0] {
	case "uci":
		e.send("id name %s", e.name)
		e.send("id author Max Klaxx Miner")
		e.send("option name MultiPV type spin default 1 min 1 max 256")
		e.send("option name Threads type spin default %d min 1 max 64", e.workers)
		e.send("option name Hash type spin default %d min %d max 65536", defaultHashMB, minHashMB)
		e.send("option name Search type combo default %s var none var mateab var matepn var matelist", defaultAlgo)
		e.send("option name MatePnMobility type combo default %s var on var off var lazy", defaultMobility)
		e.send("option name EgtbWriteCache type check default false")
		e.send("option name EgtbGenerate5 type check default false")
		e.send("option name EgtbGenerate6 type check default false")
		e.send("option name EgtbPath type string default %s", e.cacheDir)
		e.send("uciok")
	case "xboard", "protover", "new", "force":
		// Winboard probes from a GUI's auto-detection: stay silent, so that only UCI answers are seen
	case "isready":
		e.ensureTables()
		e.send("readyok")
	case "setoption":
		e.setOption(words[1:])
	case "ucinewgame":
		e.clearTables()
		if e.set != nil {
			e.set.ResetMissing() // the demand for tables belongs to the old game
		}
	case "position":
		e.position(words[1:])
	case "go":
		e.ensureTables()
		e.stopSearch() // a GUI sends stop first; if not, the old search ends here
		p := e.parseGo(words[1:])
		e.infinite = p.infinite
		e.lastGo = p
		settled := e.search()
		// a missing table of the root material is generated in the background
		// beside the search (every "go", not only infinite); when it is ready,
		// finishJob answers again with it. A position the tables settle needs
		// none (a paused job stays paused)
		e.spare = nil
		if !settled && !e.maybeGenerate() && (e.generate5 || e.generate6) && pieceCount(&e.board) > 4 {
			// nothing to generate yet: the search's demand may name a table
			// meanwhile (onDemandTick), and that job needs a copy of the set
			// taken while the main loop still owns it
			e.spare = e.set.Fork()
		}
		if !settled && e.algo != "none" {
			e.startSearch(p)
			return true
		}
		if !e.infinite {
			e.send("bestmove %s", e.pendingBest)
		}
	case "stop":
		if e.job != nil && !e.job.ctrl.Paused() {
			e.job.ctrl.Pause()
			e.send("info string egtb: generation of %s paused, the next go resumes it", e.job.mat.Name())
		}
		e.stopSearch()
		if e.infinite {
			e.send("bestmove %s", e.pendingBest)
			e.infinite = false
		}
	case "d":
		e.send("%s", e.board.FEN())
	case "quit":
		return false
	default:
		e.send("info string unknown command %s", words[0])
	}
	return true
}

func (e *Engine) setOption(words []string) {
	// setoption name <id> [value <x>]; the value may contain spaces (a path)
	name, value := "", ""
	for i := 0; i < len(words); i++ {
		switch words[i] {
		case "name":
			if i+1 < len(words) {
				name = words[i+1]
			}
		case "value":
			value = strings.Join(words[i+1:], " ")
			i = len(words)
		}
	}
	n, err := strconv.Atoi(value)
	flag := strings.EqualFold(value, "true")
	switch {
	case strings.EqualFold(name, "MultiPV") && err == nil:
		e.multiPV = max(1, n)
	case strings.EqualFold(name, "Threads") && err == nil:
		e.workers = max(1, n)
	case strings.EqualFold(name, "Hash") && err == nil:
		e.hashMB = max(minHashMB, n) // applied by the next search
	case strings.EqualFold(name, "Search") && isAlgo(value):
		e.algo = strings.ToLower(value)
	case strings.EqualFold(name, "MatePnMobility") && isMobility(value):
		e.mobility = strings.ToLower(value)
	case strings.EqualFold(name, "EgtbWriteCache"):
		e.writeCache = flag
		if flag && e.set != nil {
			e.saveTables()
		}
	case strings.EqualFold(name, "EgtbGenerate5"):
		e.generate5 = flag
	case strings.EqualFold(name, "EgtbGenerate6"):
		e.generate6 = flag
	case strings.EqualFold(name, "EgtbPath"):
		e.cacheDir = value
		if e.set != nil {
			e.set.CacheDir = value
		}
	default:
		e.send("info string unknown option %s or bad value %q", name, value)
	}
}

// ensureTables loads the four-piece base once (generating it on the first
// start, which takes about 17 s and is reported as info strings; written to
// the cache only with EgtbWriteCache).
func (e *Engine) ensureTables() {
	if e.set != nil {
		return
	}
	e.set = egtb.LoadOrGenerate(filepath.Join(e.cacheDir, egtb.BaseFileName), e.workers, e.writeCache, func(line string) {
		e.send("info string %s", compact(line))
		e.flush()
	})
}

// saveTables writes every table that lives in RAM only (generated while
// EgtbWriteCache was off) to the cache directory, the base included.
func (e *Engine) saveTables() {
	write := func(path string, save func() error) {
		if _, err := os.Stat(path); err == nil {
			return
		}
		if err := save(); err != nil {
			e.send("info string egtb: %v", err)
		} else {
			e.send("info string egtb: written to %s", path)
		}
		e.flush()
	}
	if e.set.Base()[0].Values != nil {
		base := e.set.BasePath()
		write(base, func() error { return e.set.Save(base) })
	}
	for _, t := range e.set.Tables[len(e.set.Base()):] {
		if t.Values != nil {
			path := e.set.TablePath(t)
			write(path, func() error { return e.set.SaveTable(t, path) })
		}
	}
}

// compact joins the words of a console status line with single spaces: the
// column alignment of the generator is noise in a GUI's proportional font.
func compact(line string) string { return strings.Join(strings.Fields(line), " ") }

// position [startpos | fen <6 fields>] [moves <uci>...]
func (e *Engine) position(words []string) {
	i := 0
	switch {
	case i < len(words) && words[i] == "startpos":
		e.board = bitboard.New()
		i++
	case i < len(words) && words[i] == "fen":
		j := i + 1
		for j < len(words) && words[j] != "moves" {
			j++
		}
		b, err := bitboard.FromFEN(strings.Join(words[i+1:j], " "))
		if err != nil {
			e.send("info string bad fen: %v", err)
			return
		}
		e.board = b
		i = j
	}
	if i < len(words) && words[i] == "moves" {
		for _, s := range words[i+1:] {
			m, ok := e.findMove(s)
			if !ok {
				e.send("info string illegal move %s in %s", s, e.board.FEN())
				return
			}
			e.board.DoMove(m)
		}
	}
}

func (e *Engine) findMove(s string) (chess.Move, bool) {
	var buf chess.MoveBuffer
	n := e.board.GenMoves(&buf)
	for _, m := range buf[:n] {
		if m.UCI() == s {
			return m, true
		}
	}
	return chess.Move{}, false
}

// rootMove is one root move with its value from the side to move's view.
type rootMove struct {
	move  chess.Move
	value egtb.Value
	known bool
	pv    []chess.Move
}

// search answers the current position from the tables: every root move is
// valued, sorted (shortest win, draw, unknown, longest loss) and the first
// MultiPV lines are printed with their complete lines to mate. The best move
// is kept in pendingBest; the caller decides when to send it. Returns whether
// the tables settle the position: no moves, a known win, or every move known.
// Otherwise a mate may hide behind the unknown moves and the search is asked.
func (e *Engine) search() bool {
	var buf chess.MoveBuffer
	n := e.board.GenMoves(&buf)
	if n == 0 {
		e.pendingBest = "0000"
		return true
	}
	e.loadMaterial(&e.board)
	if e.algo != "none" {
		e.loadDependencies(&e.board)
	}
	moves := make([]rootMove, 0, n)
	allKnown := true
	for _, m := range buf[:n] {
		child := e.board
		child.DoMove(m)
		e.loadMaterial(&child)
		cv, ok := e.eval(&child, 1)
		r := rootMove{move: m, known: ok}
		if ok {
			r.value = parentValue(cv)
			r.pv = append([]chess.Move{m}, e.line(child, cv)...)
		} else {
			allKnown = false
		}
		moves = append(moves, r)
	}
	sort.SliceStable(moves, func(i, j int) bool { return rank(moves[i]) < rank(moves[j]) })
	if !moves[0].known {
		e.send("info string no endgame table for this material (%d pieces)", pieceCount(&e.board))
	}
	// one depth for the whole block (the longest line), as the search's
	// blocks have it: GUIs order MultiPV lines by depth or drop the ones that
	// look older, so a draw with its one-move line must not say depth 1
	depth := 1
	for i := 0; i < len(moves) && i < e.multiPV; i++ {
		depth = max(depth, len(moves[i].pv))
	}
	for i := 0; i < len(moves) && i < e.multiPV; i++ {
		r := moves[i]
		if !r.known {
			e.send("info depth %d multipv %d score cp 0 pv %s", depth, i+1, r.move.UCI())
			continue
		}
		var sb strings.Builder
		for _, m := range r.pv {
			sb.WriteByte(' ')
			sb.WriteString(m.UCI())
		}
		e.send("info depth %d multipv %d score %s pv%s", depth, i+1, score(r.value), sb.String())
	}
	e.pendingBest, e.tableBest = moves[0].move.UCI(), moves[0].known
	e.rootTable = e.rootTable[:0]
	for _, r := range moves {
		if r.known {
			e.rootTable = append(e.rootTable, r)
		}
	}
	// settled only when every move is known: a known win alone proves
	// nothing about the other moves (a quiet move could mate faster), the
	// root driver checks them with the table values as its starting point
	return allKnown
}

// maybeGenerate starts (or resumes) the background generation of the root
// material's table on every "go", when the options allow it; it runs beside
// the search with backgroundWorkers threads and finishJob answers again when
// it is ready. Returns whether a generation is running afterwards.
func (e *Engine) maybeGenerate() bool {
	m, ok := e.generationTarget()
	if !ok {
		return false // nothing missing that the options allow: the search runs alone, quietly
	}
	if e.job != nil {
		if e.job.mat.Name() == m.Name() {
			if e.job.ctrl.Paused() {
				e.job.ctrl.Resume()
				e.send("info string egtb: generation of %s resumed", m.Name())
			}
			return true
		}
		e.job.ctrl.Abort()
		e.send("info string egtb: generation of %s dropped", e.job.mat.Name())
		e.job = nil
	}
	return e.startJob(m)
}

// generationTarget picks the material worth generating: the root material
// when its table is missing and the options allow its piece count, else the
// material without a table the searches asked for most (Set.MissingTables,
// "Erzeugung nach Bedarf" in docs/m6-uci.md: a six-piece root without
// EgtbGenerate6 still gets the five-piece tables its captures lead into).
func (e *Engine) generationTarget() (egtb.Material, bool) {
	missing := func(m egtb.Material) bool {
		if m.Pieces() <= 4 || !(m.Pieces() == 5 && e.generate5 || m.Pieces() == 6 && e.generate6) {
			return false
		}
		t := e.set.Find(m.Name())
		return t == nil || !t.Available()
	}
	if m, ok := egtb.MaterialOf(&e.board); ok && e.board.Castling == 0 && missing(m) {
		return m, true
	}
	for _, mt := range e.set.MissingTables(0) {
		if missing(mt.Mat) {
			e.send("info string egtb: the search asked %s times for %s, generating it", egtb.Group(int(mt.Probes)), mt.Mat.Name())
			return mt.Mat, true
		}
	}
	return egtb.Material{}, false
}

// onDemandTick runs in the main loop every few seconds of a search (the
// reporter knocks): when no generation runs and a spare copy of the set is
// at hand, the material the search asked for most, within the options'
// piece limits, is generated beside it ("Erzeugung nach Bedarf").
func (e *Engine) onDemandTick() {
	if !e.searching || e.restart || e.job != nil || e.spare == nil {
		return
	}
	for _, mt := range e.set.MissingTables(0) {
		if mt.Mat.Pieces() == 5 && e.generate5 || mt.Mat.Pieces() == 6 && e.generate6 {
			e.send("info string egtb: the search asked %s times for %s, generating it", egtb.Group(int(mt.Probes)), mt.Mat.Name())
			e.startJobFrom(mt.Mat, e.spare)
			e.spare = nil
			return
		}
	}
}

func (e *Engine) startJob(m egtb.Material) bool {
	return e.startJobFrom(m, e.set.Fork())
}

// startJobFrom generates m in the background on set, a fork of the loaded
// tables (taken by the caller while the main loop owned them).
func (e *Engine) startJobFrom(m egtb.Material, set *egtb.Set) bool {
	e.set.ForgetMissing(m) // decided: the next demand decision looks at the rest
	t := set.AddMaterial(m)
	need, measured := set.EstimateBytes(t)
	how := "estimated"
	if measured {
		how = "measured"
	}
	if total, avail, ok := egtb.AvailableMemory(); ok {
		if uint64(need) > total {
			e.send("info string egtb: %s needs about %s (%s), the machine has %s: not started", m.Name(), egtb.FormatBytes(uint64(need)), how, egtb.FormatBytes(total))
			return false
		}
		e.send("info string egtb: generating %s (%d MB table, about %s RAM %s, %s of %s free), stop pauses it", m.Name(), t.Size>>20, egtb.FormatBytes(uint64(need)), how, egtb.FormatBytes(avail), egtb.FormatBytes(total))
		if uint64(need) > avail {
			e.send("info string egtb: WARNING: more than the free memory, expect swapping")
		}
	} else {
		e.send("info string egtb: generating %s (%d MB table, about %s RAM %s), stop pauses it", m.Name(), t.Size>>20, egtb.FormatBytes(uint64(need)), how)
	}
	// the search keeps its hash table unless the generation would not fit
	// beside it (the estimate against the free memory, which the table
	// already took from)
	if _, avail, ok := egtb.AvailableMemory(); ok && uint64(need) > avail && e.tableMB > 0 {
		e.send("info string egtb: the search's hash table (%d MB) is released for the generation", e.tableMB)
		e.dropTables()
	}
	j := &job{mat: m, ctrl: egtb.NewControl()}
	e.job = j
	workers, write := e.backgroundWorkers(), e.writeCache
	e.send("info string egtb: %d workers beside the search thread (%d cores)", workers, egtb.PhysicalCores())
	go func() {
		// level lines come up to 254 times per table: at most one every few seconds
		var lastLevel time.Time
		progress := func(line string) {
			if strings.HasPrefix(line, " ") {
				if time.Since(lastLevel) < progressInterval {
					return
				}
				lastLevel = time.Now()
			}
			e.send("info string egtb: %s", compact(line))
			e.flush()
		}
		st := set.GenerateControlled(t, workers, j.ctrl, progress)
		if st.Aborted {
			return
		}
		if err := egtb.AppendLog(set.CacheDir, set.LogLine(t, st, workers, "uci")); err != nil {
			progress(err.Error())
		}
		for _, gt := range set.Tables[len(set.Base()):] {
			if gt.Values == nil {
				continue
			}
			if path := set.TablePath(gt); write {
				if _, err := os.Stat(path); err != nil {
					if err := set.SaveTable(gt, path); err != nil {
						progress(err.Error())
					} else {
						progress("written to " + path)
					}
				}
			}
			gt.Fill() // drops the generator's bitset
		}
		e.jobDone <- set
	}()
	return true
}

// finishJob takes the generated tables over and, when a search was running
// beside the generation (or a "go infinite" is open), answers again with
// them: the table stage, and the search once more with the rest of the time
// budget where the tables still do not settle the position. The running
// search has to stop first, because the set changes under it (the Rust
// search thread even owns it); its partial result is discarded.
func (e *Engine) finishJob(set *egtb.Set) {
	if e.job == nil {
		return
	}
	again := e.searching || e.infinite
	e.restart = e.searching
	e.stopSearch()
	e.restart = false
	name := e.job.mat.Name()
	e.job = nil
	e.set.Adopt(set)
	if committed, working, ok := egtb.PeakMemory(); ok {
		e.send("info string egtb: %s ready, peak memory %s committed / %s working set", name, egtb.FormatBytes(committed), egtb.FormatBytes(working))
	} else {
		e.send("info string egtb: %s ready", name)
	}
	if !again {
		return
	}
	// the next table the searches asked for, if any, starts right away (the
	// restarted search pauses it when the new table answers everything)
	e.maybeGenerate()
	if !e.search() && e.algo != "none" {
		p := e.lastGo
		if !p.infinite && p.budget > 0 {
			p.budget = max(time.Until(e.deadline), 50*time.Millisecond)
		}
		e.startSearch(p)
	} else if !e.infinite {
		e.send("bestmove %s", e.pendingBest)
	}
}

// backgroundWorkers is the thread count of a generation beside a running
// search: the search needs a core of its own, and an SMT sibling would share
// one with it, so physical cores minus one, bounded by Threads, at least one.
func (e *Engine) backgroundWorkers() int {
	cores := egtb.PhysicalCores()
	if cores <= 0 {
		cores = e.workers
	}
	return max(1, min(e.workers, cores)-1)
}

// rank orders root moves: wins by shortest distance, then draws, then moves
// without a table, then losses by longest distance.
func rank(r rootMove) int {
	switch {
	case !r.known:
		return 2 << 10
	case r.value.IsWin():
		return r.value.Plies()
	case r.value.IsLoss():
		return 3<<10 - r.value.Plies()
	}
	return 1 << 10
}

// score renders a value as UCI score from the root side's view.
func score(v egtb.Value) string {
	switch {
	case v.IsWin():
		return fmt.Sprintf("mate %d", (v.Plies()+1)/2)
	case v.IsLoss():
		return fmt.Sprintf("mate -%d", v.Plies()/2)
	}
	return "cp 0"
}

// parentValue converts a child's value (its side to move) to the parent's.
func parentValue(cv egtb.Value) egtb.Value {
	switch {
	case cv.IsLoss():
		return egtb.WinIn(cv.Plies() + 1)
	case cv.IsWin():
		return egtb.LossIn(cv.Plies() + 1)
	}
	return egtb.Draw
}

// loadMaterial registers the material of a position beyond the base and loads
// its table from the cache directory if a file exists (never generates here).
func (e *Engine) loadMaterial(b *bitboard.Board) {
	if pieceCount(b) <= 4 || b.Castling != 0 {
		return
	}
	m, ok := egtb.MaterialOf(b)
	if !ok {
		return
	}
	e.loadTable(m)
}

// loadTable registers the material and loads its cache file, with one line
// per table loaded.
func (e *Engine) loadTable(m egtb.Material) {
	t, loaded := e.openTable(m)
	if !loaded {
		return
	}
	path := e.set.TablePath(t)
	if t.Exact() {
		e.send("info string egtb: loaded %s", path)
	} else {
		// cache files beyond the base hold buckets of egtb.FileStep moves:
		// the table stage does not report them, the search resolves them
		e.send("info string egtb: loaded %s (step %d, the search resolves exact distances)", path, t.Step)
	}
	e.flush()
}

// openTable registers the material (if new) and loads its cache file unless
// the table is already available or no file exists. A table registered
// earlier without a file is retried, the file may have appeared since.
// Returns whether a file was loaded now.
func (e *Engine) openTable(m egtb.Material) (*egtb.Table, bool) {
	t := e.set.Find(m.Name())
	if t != nil && t.Available() {
		return t, false
	}
	if t == nil {
		t = e.set.AddMaterial(m)
	}
	path := e.set.TablePath(t)
	if _, err := os.Stat(path); err != nil {
		return t, false
	}
	if err := e.set.LoadTable(t, path); err != nil {
		e.send("info string %s: %v", path, err)
		return t, false
	}
	return t, true
}

// loadDependencies loads every cached table beyond the base that a search
// below this position can reach by captures and promotions, whatever the size
// of the root: an eleven-piece pawn ending reaches the five-piece tables too.
// The candidates are the files in the cache directory (one listing, sorted by
// name), filtered by material, so an empty directory costs nothing and a
// table is loaded only when its file exists (header and block index only,
// the blocks follow on demand). One summary line instead of one per table.
// The search thread must not touch the set, so this happens before it starts.
func (e *Engine) loadDependencies(b *bitboard.Board) {
	white, black, ok := egtb.PiecesOf(b)
	if !ok || 2+len(white)+len(black) <= 4 {
		return
	}
	entries, err := os.ReadDir(e.set.CacheDir)
	if err != nil {
		return
	}
	loaded, step := 0, 1
	for _, ent := range entries {
		m, ok := egtb.ParseFileName(ent.Name())
		if !ok || !m.ReachableFrom(white, black) {
			continue
		}
		if t, ok := e.openTable(m); ok {
			loaded++
			step = max(step, t.Step)
		}
	}
	if loaded == 0 {
		return
	}
	if step <= 1 {
		e.send("info string egtb: loaded %d cached tables the search can reach", loaded)
	} else {
		e.send("info string egtb: loaded %d cached tables the search can reach (step %d, the search resolves exact distances)", loaded, step)
	}
	e.flush()
}

// eval values a position from the side to move's view: from the table, or,
// for positions the tables leave out (an en passant right), from its children
// up to depth plies deep. ok is false when no table covers it, or when the
// table only bounds the distance (quantised): the table stage reports exact
// values only, such positions go to the search, which has the bounds as its
// oracle.
func (e *Engine) eval(b *bitboard.Board, depth int) (egtb.Value, bool) {
	if v, lo, hi, ok := e.set.LookupBounds(b); ok {
		return v, lo == hi
	}
	var buf chess.MoveBuffer
	n := b.GenMoves(&buf)
	if n == 0 {
		if b.InCheck() {
			return egtb.LossIn(0), true
		}
		return egtb.Draw, true
	}
	if depth == 0 {
		return egtb.Draw, false
	}
	minLoss, maxWin, allWin := 1<<30, -1, true
	for _, m := range buf[:n] {
		child := *b
		child.DoMove(m)
		cv, ok := e.eval(&child, depth-1)
		if !ok {
			return egtb.Draw, false
		}
		switch {
		case cv.IsLoss():
			minLoss = min(minLoss, cv.Plies())
			allWin = false
		case cv.IsWin():
			maxWin = max(maxWin, cv.Plies())
		default:
			allWin = false
		}
	}
	switch {
	case minLoss < 1<<30:
		return egtb.WinIn(minLoss + 1), true
	case allWin:
		return egtb.LossIn(maxWin + 1), true
	}
	return egtb.Draw, true
}

// line follows the optimal play from a position with value v: the winner
// takes a child lost in v-1, the loser a child won in v-1 (the longest
// defence). Draws give an empty line. Stops at mate or where the tables end.
func (e *Engine) line(b bitboard.Board, v egtb.Value) []chess.Move {
	var line []chess.Move
	for (v.IsWin() || v.IsLoss()) && v.Plies() > 0 && len(line) < egtb.MaxPlies {
		var buf chess.MoveBuffer
		n := b.GenMoves(&buf)
		found := false
		var best chess.Move
		var bestVal egtb.Value
		for _, m := range buf[:n] {
			child := b
			child.DoMove(m)
			e.loadMaterial(&child)
			cv, ok := e.eval(&child, 1)
			if !ok {
				continue
			}
			if v.IsWin() && cv.IsLoss() && cv.Plies() == v.Plies()-1 || v.IsLoss() && cv.IsWin() && cv.Plies() == v.Plies()-1 {
				best, bestVal, found = m, cv, true
				break
			}
		}
		if !found {
			break
		}
		line = append(line, best)
		b.DoMove(best)
		v = bestVal
	}
	return line
}

func pieceCount(b *bitboard.Board) int {
	n := 0
	for _, p := range b.Squares {
		if p != chess.None {
			n++
		}
	}
	return n
}

// dropTables frees the search's transposition tables (before a generation).
func (e *Engine) dropTables() {
	if e.direct == nil && e.buckets == nil {
		return
	}
	e.direct, e.buckets = nil, nil
	debug.FreeOSMemory()
}

// clearTables empties the search's transposition tables (ucinewgame).
func (e *Engine) clearTables() {
	if e.direct != nil {
		e.direct.Clear()
	}
	if e.buckets != nil {
		e.buckets.Clear()
	}
}
