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
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mateab"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/pairs"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/syzygy"
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
	forceGen   bool   // EgtbForceGen: go infinite generates every material the options allow, one after the other (sweep.go)
	generate5  bool   // EgtbGenerate5: build missing five-piece tables during go infinite
	generate6  bool   // EgtbGenerate6: the same for six pieces
	cacheDir   string // EgtbPath
	syzygyPath string // SyzygyPath: a folder with Syzygy WDL tables, empty = none
	syzygy     *syzygy.Tables
	algo       string // Search: the algorithm for positions the tables do not settle
	hashMB     int    // Hash: the transposition table of the search
	mobility   string // MatePnMobility: on (leaves start with their move count), off (1/1), lazy (move count when entered)
	goal       string // MatePnGoal: mate (the shortest mate, depth by depth) or win (any won table position proves, the mate length is an upper bound)
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
	mat   egtb.Material
	ctrl  *egtb.Control
	sweep bool // EgtbForceGen: every material the options allow, one after the other (sweep.go); mat is the first
}

// Run reads commands from in until quit or EOF. name is the engine name
// reported to the GUI (the Go and Rust binaries differ).
func Run(in io.Reader, out io.Writer, name string, workers int) {
	e := &Engine{out: bufio.NewWriter(out), board: bitboard.New(), multiPV: 1, workers: workers, name: name, cacheDir: egtb.DefaultCacheDir(),
		algo: defaultAlgo, hashMB: defaultHashMB, mobility: defaultMobility, goal: defaultGoal, jobDone: make(chan *egtb.Set, 1), searchDone: make(chan searchEnd, 1), demandTick: make(chan struct{}, 1)}
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
		e.send("option name Search type combo default %s var none var mateab var matepn var matelist var fastwdl", defaultAlgo)
		e.send("option name MatePnMobility type combo default %s var on var off var lazy", defaultMobility)
		e.send("option name MatePnGoal type combo default %s var mate var win", defaultGoal)
		e.send("option name EgtbWriteCache type check default false")
		e.send("option name EgtbForceGen type check default false")
		e.send("option name EgtbGenerate5 type check default false")
		e.send("option name EgtbGenerate6 type check default false")
		e.send("option name EgtbPath type string default %s", e.cacheDir)
		e.send("option name SyzygyPath type string default <empty>")
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
		e.spare = nil
		if e.forceGen && p.infinite && (e.generate5 || e.generate6) && e.startSweep() {
			// EgtbForceGen: go infinite runs the sweep over every material the
			// options allow instead of a search (every thread and the hash
			// table's memory are its), stop pauses it, the next go infinite
			// resumes it; with every file in place the go runs as usual
			return true
		}
		// a missing table of the root material is generated in the background
		// beside the search (every "go", not only infinite); when it is ready,
		// finishJob answers again with it. A position the tables settle needs
		// none (a paused job stays paused)
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
			if e.job.sweep {
				e.send("info string egtb: sweep paused, the next go infinite with EgtbForceGen resumes it")
			} else {
				e.send("info string egtb: generation of %s paused, the next go resumes it", e.job.mat.Name())
			}
		}
		e.stopSearch()
		if e.job != nil && e.job.sweep {
			e.registerOwnFiles() // the files written so far serve the searches meanwhile (after the search stopped: it reads the tables)
		}
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
	case strings.EqualFold(name, "MatePnGoal") && isGoal(value):
		e.goal = strings.ToLower(value) // the next search clears the table when the goal changed (tableKey)
	case strings.EqualFold(name, "EgtbWriteCache"):
		e.writeCache = flag
		if flag && e.set != nil {
			e.saveTables()
		}
	case strings.EqualFold(name, "EgtbGenerate5"):
		e.generate5 = flag
	case strings.EqualFold(name, "EgtbGenerate6"):
		e.generate6 = flag
	case strings.EqualFold(name, "EgtbForceGen"):
		e.forceGen = flag
	case strings.EqualFold(name, "EgtbPath"):
		e.cacheDir = value
		if e.set != nil {
			e.set.CacheDir = value
			e.attachWdl()
		}
	case strings.EqualFold(name, "SyzygyPath"):
		// the WDL tables become part of the search oracle (Syzygy answers
		// win, draw or loss without a distance, the fifty-move rule is
		// ignored); the files are mapped on first use
		e.syzygyPath = value
		e.attachWdl()
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
	e.attachWdl()
}

// attachWdl builds the WDL tables of the search oracle: the Syzygy files of
// SyzygyPath, when set, and our own files in the cache folder for every
// other material (the originals win), with the base answering captures into
// materials without a file. Called once the base is loaded and whenever
// SyzygyPath or the cache folder changes.
func (e *Engine) attachWdl() {
	if e.set == nil {
		return // before the base is loaded: ensureTables calls again
	}
	if e.syzygy != nil {
		e.syzygy.Close()
		e.syzygy = nil
	}
	var tb *syzygy.Tables
	if e.syzygyPath != "" && e.syzygyPath != "<empty>" {
		t, err := syzygy.Open(e.syzygyPath)
		if err != nil {
			e.send("info string syzygy: %v", err)
		} else {
			tb = t
			e.send("info string syzygy: %d WDL tables in %s, up to %d pieces", tb.Count(), e.syzygyPath, tb.Largest())
		}
	}
	if tb == nil {
		tb = syzygy.New()
	}
	if n, err := tb.AddOwn(e.set.WdlPath()); err == nil && n > 0 {
		e.send("info string wdl: %d own tables in %s", n, e.set.WdlPath())
	}
	if n, err := tb.AddDTM(e.set.DtmPath()); err == nil && n > 0 {
		e.send("info string dtm: %d own tables in %s (step %d, the search resolves exact distances)", n, e.set.DtmPath(), egtb.FileStep)
	}
	tb.SetBase(e.set)
	e.syzygy = tb
	e.flush()
}

// writeDtm writes the own DTM table of an exact table to the cache folder
// when the file is missing (the distance buckets; read after the WDL
// verdict, so the WDL file comes first). The DTM samples its own candidate
// orders: the WDL's best order packs the DTM a quarter larger
// (docs/m5-endgame-tables.md, "Schneller schreiben").
func writeDtm(set *egtb.Set, t *egtb.Table, workers int, progress func(string)) {
	path := set.DtmFile(t.Mat)
	if !fileMissing(path) || t.Values == nil || !t.Exact() {
		return
	}
	start := time.Now()
	var b *syzygy.Built
	var err error
	ticking("DTM of "+t.Mat.Name(), progress, func(sink func(string)) {
		b, err = syzygy.WriteDTM(set, t, path, pairs.Options{Workers: workers}, syzygy.BuildOptions{}, workers, sink)
	})
	if err != nil {
		progress("DTM: " + err.Error())
		return
	}
	progress(fmt.Sprintf("DTM written to %s (%s bytes, %d candidate orders, %s)", path, egtb.Group(b.Bytes()), b.Candidates, time.Since(start).Round(time.Second)))
}

// ticking runs build with a progress sink and reports, as long as it runs,
// one line every progressInterval: "<label>: 37 candidate samples, 2
// finalist streams built, 25s" (the builder's own lines, one per sample and
// per built stream, would be hundreds, and a finalist stream can take half a
// minute without one).
func ticking(label string, progress func(string), build func(sink func(string))) {
	var mu sync.Mutex
	samples, finalists := 0, 0
	sink := func(line string) {
		mu.Lock()
		if strings.Contains(line, " sample ") {
			samples++
		} else {
			finalists++
		}
		mu.Unlock()
	}
	done := make(chan struct{})
	start := time.Now()
	go func() {
		tick := time.NewTicker(progressInterval)
		defer tick.Stop()
		for {
			select {
			case <-done:
				return
			case <-tick.C:
				mu.Lock()
				s, f := samples, finalists
				mu.Unlock()
				progress(fmt.Sprintf("%s: %d candidate samples, %d finalist streams built, %s", label, s, f, time.Since(start).Round(time.Second)))
			}
		}
	}()
	build(sink)
	close(done)
}

// writeWdl writes the own WDL table of an exact table to the cache folder
// when the file is missing: the candidate orders are judged on samples and
// the finalists packed (seconds for the base, minutes for five pieces). A
// material the Syzygy files cover gets no own file: the prober would never
// read it (the DTM file is written regardless, it is read after either).
func writeWdl(set *egtb.Set, t *egtb.Table, tb *syzygy.Tables, workers int, progress func(string)) {
	path := set.WdlFile(t.Mat)
	if !fileMissing(path) || t.Values == nil || !t.Exact() {
		return
	}
	if tb != nil && tb.HasOriginal(t.Mat) {
		progress(fmt.Sprintf("WDL of %s not written, Syzygy has it", t.Mat.Name()))
		return
	}
	start := time.Now()
	var b *syzygy.Built
	var err error
	ticking("WDL of "+t.Mat.Name(), progress, func(sink func(string)) {
		b, err = syzygy.WriteWDL(set, t, path, pairs.Options{Workers: workers}, syzygy.BuildOptions{}, workers, sink)
	})
	if err != nil {
		progress("WDL: " + err.Error())
		return
	}
	progress(fmt.Sprintf("WDL written to %s (%s bytes, %d candidate orders, %s)", path, egtb.Group(b.Bytes()), b.Candidates, time.Since(start).Round(time.Second)))
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
	known bool // the own tables know the distance (value and pv)
	pv    []chess.Move
	// wdl is set when only the Syzygy WDL tables know the move (the
	// outcome from the root side's view, no distance, no line)
	wdl mateab.Verdict
}

// wdlOnly: the move is known by the WDL tables only (no distance).
func (r rootMove) wdlOnly() bool { return !r.known && r.wdl != mateab.Unknown }

// wdlScore renders a WDL-only outcome as the WDL search does.
func wdlScore(v mateab.Verdict) string {
	switch v {
	case mateab.Win:
		return "cp 9999"
	case mateab.Loss:
		return "cp -9999"
	}
	return "cp 0"
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
			// the Syzygy WDL tables know the outcome without a distance
			// (v is for the opponent, to move in the child): the WDL search
			// has nothing left to do for such a move, the mate searches
			// skip a known draw or loss and still search a known win for
			// its distance
			if e.syzygy != nil {
				if v, hit := e.syzygy.ProbeWDL(&child); hit {
					switch {
					case v >= syzygy.CursedWin:
						r.wdl = mateab.Loss
					case v <= syzygy.BlessedLoss:
						r.wdl = mateab.Win
					default:
						r.wdl = mateab.Draw
					}
				}
			}
			if !(r.wdl != mateab.Unknown && (e.algo == "fastwdl" || r.wdl != mateab.Win)) {
				allKnown = false
			}
		}
		moves = append(moves, r)
	}
	sort.SliceStable(moves, func(i, j int) bool { return rank(moves[i]) < rank(moves[j]) })
	if !moves[0].known && !moves[0].wdlOnly() {
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
		if r.wdlOnly() {
			e.send("info depth %d multipv %d score %s pv %s", depth, i+1, wdlScore(r.wdl), r.move.UCI())
			continue
		}
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
	e.pendingBest, e.tableBest = moves[0].move.UCI(), moves[0].known || moves[0].wdlOnly()
	e.rootTable = e.rootTable[:0]
	for _, r := range moves {
		if r.known || r.wdlOnly() {
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
	if e.job != nil && e.job.sweep {
		return true // a paused sweep stays as it is (its files serve the searches), nothing else starts
	}
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
		return (t == nil || !t.Available()) && !e.hasProduct(m)
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
		if (mt.Mat.Pieces() == 5 && e.generate5 || mt.Mat.Pieces() == 6 && e.generate6) && !e.hasProduct(mt.Mat) {
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
	syz := e.syzygy // the job thread only asks it which materials the Syzygy files cover
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
		// every computed table gets its log line; a dependency the job had
		// to compute (or found as an exact file) is written right away, both
		// exactly (RawPath) and quantised, so that a job interrupted before
		// the requested table loses nothing and a later generation loads the
		// exact file instead of computing it again (the quantised file
		// cannot serve as a dependency); the requested table is written below
		done := func(gt *egtb.Table, st egtb.Stats) {
			if !st.Loaded {
				if err := egtb.AppendLog(set.CacheDir, set.LogLine(gt, st, workers, "uci")); err != nil {
					progress(err.Error())
				}
			}
			if write {
				// from the exact values, before anything quantises them: the WDL
				// file, then the distance buckets (which replace the quantised .bin)
				writeWdl(set, gt, syz, workers, progress)
				writeDtm(set, gt, workers, progress)
			}
			if gt == t || !write {
				return
			}
			if err := set.SaveBoth(gt, func(line string) { progress(strings.TrimPrefix(line, "egtb: ")) }); err != nil {
				progress(err.Error())
			}
		}
		st := set.GenerateNotify(t, workers, j.ctrl, progress, done)
		if st.Aborted {
			return
		}
		for _, gt := range set.Tables[len(set.Base()):] {
			if gt.Values == nil {
				continue
			}
			// the quantised .bin is no longer written (2026-10-22): the WDL and
			// DTM files of the done callback replace it, existing .bin files
			// still load
			gt.Fill() // drops the generator's bitset
		}
		e.jobDone <- set
	}()
	return true
}

// fileMissing reports whether nothing exists at path.
func fileMissing(path string) bool {
	_, err := os.Stat(path)
	return err != nil
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
	if e.job.sweep {
		name = "sweep"
	}
	e.job = nil
	e.set.Adopt(set)
	e.registerOwnFiles()
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
	case r.wdlOnly():
		// a WDL win after the wins with a distance, a WDL loss before the
		// losses with one (its distance is unknown, so it may be the longest)
		switch r.wdl {
		case mateab.Win:
			return 1<<10 - 1
		case mateab.Loss:
			return 3<<10 - 512
		}
		return 1 << 10
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
	// the quantised file first, else the exact twin of an interrupted job:
	// with EgtbWriteCache the quantised file is made from it now (loaded
	// whole, written, dropped), otherwise it is read block by block like
	// any cache file, only with step 1
	path := e.set.TablePath(t)
	if fileMissing(path) {
		raw := e.set.RawPath(t)
		if fileMissing(raw) {
			return t, false
		}
		if e.writeCache {
			if err := e.set.LoadRawTable(t, raw); err != nil {
				e.send("info string %s: %v", raw, err)
				return t, false
			}
			if err := e.set.SaveTable(t, path); err != nil {
				e.send("info string %s: %v", path, err)
				return t, false
			}
			e.send("info string egtb: written to %s (from %s)", path, raw)
		} else {
			path = raw
		}
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
	entries, err := os.ReadDir(e.set.DtmPath())
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
