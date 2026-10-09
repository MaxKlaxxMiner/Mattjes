// Package uci speaks the Universal Chess Interface (milestone 6, stages a and
// b): positions covered by the endgame tables are answered straight from them,
// with the exact distance to mate and the complete winning line; every root
// move gets its own value, so MultiPV lists them all. Positions without a
// table get a legal move and an "info string" for now; the search algorithms
// come in a later stage.
//
// Tables beyond the four-piece base are loaded from the cache directory when
// a file exists. When the options allow it, a missing table is generated in
// the background, but only while a "go infinite" is running: "stop" pauses
// the generation, the next "go infinite" resumes it, and once the table is
// complete the held-back answer is recomputed with it.
//
// Protocol subset: uci, isready, setoption, ucinewgame, position, go (only
// "infinite" is looked at, the answer is immediate), stop, quit, d (debug:
// print the FEN).
package uci

import (
	"bufio"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"slices"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
)

// Version is reported in "id name"; it follows the milestone that is being
// worked on.
const Version = "0.6"

// About is the banner line printed at startup (unprompted, as most engines do).
const About = "Mattjes " + Version + " by Max Klaxx Miner, a mate and draw search engine with its own endgame tables, GPLv3"

// progressInterval limits the per-level progress of a background generation
// to one info string every few seconds (a table has up to 254 levels).
const progressInterval = 5 * time.Second

// Engine is the state between commands.
type Engine struct {
	out     *bufio.Writer
	outMu   sync.Mutex // the generation goroutine reports through the same writer
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
	// a "go infinite" holds the best move back until "stop"
	infinite    bool
	pendingBest string
	job         *job
	jobDone     chan *egtb.Set
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
	e := &Engine{out: bufio.NewWriter(out), board: bitboard.New(), multiPV: 1, workers: workers, name: name, cacheDir: egtb.DefaultCacheDir(), jobDone: make(chan *egtb.Set, 1)}
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
				e.flush()
				return
			}
		case set := <-e.jobDone:
			e.finishJob(set)
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
	case "position":
		e.position(words[1:])
	case "go":
		e.ensureTables()
		e.infinite = slices.Contains(words, "infinite")
		e.search()
		if e.infinite {
			e.maybeGenerate()
		} else {
			e.send("bestmove %s", e.pendingBest)
		}
	case "stop":
		if e.job != nil && !e.job.ctrl.Paused() {
			e.job.ctrl.Pause()
			e.send("info string egtb: generation of %s paused, the next go infinite resumes it", e.job.mat.Name())
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
// is kept in pendingBest; the caller decides when to send it.
func (e *Engine) search() {
	var buf chess.MoveBuffer
	n := e.board.GenMoves(&buf)
	if n == 0 {
		e.pendingBest = "0000"
		return
	}
	e.loadMaterial(&e.board)
	moves := make([]rootMove, 0, n)
	for _, m := range buf[:n] {
		child := e.board
		child.DoMove(m)
		e.loadMaterial(&child)
		cv, ok := e.eval(&child, 1)
		r := rootMove{move: m, known: ok}
		if ok {
			r.value = parentValue(cv)
			r.pv = append([]chess.Move{m}, e.line(child, cv)...)
		}
		moves = append(moves, r)
	}
	sort.SliceStable(moves, func(i, j int) bool { return rank(moves[i]) < rank(moves[j]) })
	if !moves[0].known {
		e.send("info string no endgame table for this material (%d pieces)", pieceCount(&e.board))
	}
	for i := 0; i < len(moves) && i < e.multiPV; i++ {
		r := moves[i]
		if !r.known {
			e.send("info depth 1 multipv %d score cp 0 pv %s", i+1, r.move.UCI())
			continue
		}
		var sb strings.Builder
		for _, m := range r.pv {
			sb.WriteByte(' ')
			sb.WriteString(m.UCI())
		}
		e.send("info depth %d multipv %d score %s pv%s", len(r.pv), i+1, score(r.value), sb.String())
	}
	e.pendingBest = moves[0].move.UCI()
}

// maybeGenerate starts (or resumes) the background generation of the root
// material's table during a "go infinite", when the options allow it.
func (e *Engine) maybeGenerate() {
	m, ok := egtb.MaterialOf(&e.board)
	if !ok || m.Pieces() <= 4 || e.board.Castling != 0 {
		return
	}
	if t := e.set.Find(m.Name()); t != nil && t.Values != nil {
		return
	}
	if m.Pieces() == 5 && !e.generate5 || m.Pieces() == 6 && !e.generate6 {
		e.send("info string egtb: no table for %s (EgtbGenerate%d would build it during go infinite)", m.Name(), m.Pieces())
		return
	}
	if e.job != nil {
		if e.job.mat.Name() == m.Name() {
			if e.job.ctrl.Paused() {
				e.job.ctrl.Resume()
				e.send("info string egtb: generation of %s resumed", m.Name())
			}
			return
		}
		e.job.ctrl.Abort()
		e.send("info string egtb: generation of %s dropped", e.job.mat.Name())
		e.job = nil
	}
	e.startJob(m)
}

func (e *Engine) startJob(m egtb.Material) {
	j := &job{mat: m, ctrl: egtb.NewControl()}
	e.job = j
	set := e.set.Fork()
	t := set.AddMaterial(m)
	e.send("info string egtb: generating %s (%d MB table, about %d MB RAM), stop pauses it", m.Name(), t.Size>>20, set.GenerationBytes(t)>>20)
	workers, write := e.workers, e.writeCache
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
}

// finishJob takes the generated tables over and, during a "go infinite",
// answers again with them.
func (e *Engine) finishJob(set *egtb.Set) {
	if e.job == nil {
		return
	}
	name := e.job.mat.Name()
	e.job = nil
	e.set.Adopt(set)
	if committed, working, ok := egtb.PeakMemory(); ok {
		e.send("info string egtb: %s ready, peak memory %s committed / %s working set", name, egtb.FormatBytes(committed), egtb.FormatBytes(working))
	} else {
		e.send("info string egtb: %s ready", name)
	}
	if e.infinite {
		e.search()
	}
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
	if !ok || e.set.Find(m.Name()) != nil {
		return
	}
	t := e.set.AddMaterial(m)
	path := e.set.TablePath(t)
	if _, err := os.Stat(path); err != nil {
		return
	}
	if err := e.set.LoadTable(t, path); err != nil {
		e.send("info string %s: %v", path, err)
		return
	}
	e.send("info string egtb: loaded %s", path)
	e.flush()
}

// eval values a position from the side to move's view: from the table, or,
// for positions the tables leave out (an en passant right), from its children
// up to depth plies deep. ok is false when no table covers it.
func (e *Engine) eval(b *bitboard.Board, depth int) (egtb.Value, bool) {
	if v, ok := e.set.Lookup(b); ok {
		return v, true
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
