// Package uci speaks the Universal Chess Interface (milestone 6, stage a):
// positions covered by the endgame tables are answered straight from them,
// with the exact distance to mate and the complete winning line; every root
// move gets its own value, so MultiPV lists them all. Positions without a
// table get a legal move and an "info string" for now; the search algorithms
// come in later stages.
//
// Protocol subset: uci, isready, setoption (MultiPV, Threads), ucinewgame,
// position, go (all parameters ignored, the answer is immediate), stop, quit,
// d (debug: print the FEN).
package uci

import (
	"bufio"
	"fmt"
	"io"
	"os"
	"sort"
	"strconv"
	"strings"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
)

// Engine is the state between commands.
type Engine struct {
	out     *bufio.Writer
	set     *egtb.Set
	board   bitboard.Board
	multiPV int
	workers int
	name    string
}

// Run reads commands from in until quit or EOF. name is the engine name
// reported to the GUI (the Go and Rust binaries differ).
func Run(in io.Reader, out io.Writer, name string, workers int) {
	e := &Engine{out: bufio.NewWriter(out), board: bitboard.New(), multiPV: 1, workers: workers, name: name}
	scanner := bufio.NewScanner(in)
	scanner.Buffer(make([]byte, 1<<16), 1<<20)
	for scanner.Scan() {
		if !e.command(strings.Fields(scanner.Text())) {
			break
		}
		e.out.Flush()
	}
	e.out.Flush()
}

func (e *Engine) send(format string, args ...any) {
	fmt.Fprintf(e.out, format+"\n", args...)
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
		e.send("uciok")
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
		e.search()
	case "stop":
		// the answer is always sent at once, nothing runs in the background yet
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
	// setoption name <id> [value <x>]
	name, value := "", ""
	for i := 0; i < len(words); i++ {
		switch words[i] {
		case "name":
			if i+1 < len(words) {
				name = words[i+1]
			}
		case "value":
			if i+1 < len(words) {
				value = words[i+1]
			}
		}
	}
	n, err := strconv.Atoi(value)
	switch {
	case err != nil:
		e.send("info string option %s: bad value %q", name, value)
	case strings.EqualFold(name, "MultiPV"):
		e.multiPV = max(1, n)
	case strings.EqualFold(name, "Threads"):
		e.workers = max(1, n)
	default:
		e.send("info string unknown option %s", name)
	}
}

// ensureTables loads the four-piece base once (generating it on the first
// start, which takes about 17 s and is reported as info strings).
func (e *Engine) ensureTables() {
	if e.set != nil {
		return
	}
	e.set = egtb.LoadOrGenerate(egtb.DefaultPath(), e.workers, func(line string) {
		e.send("info string %s", line)
		e.out.Flush()
	})
}

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
// MultiPV lines are printed with their complete lines to mate.
func (e *Engine) search() {
	var buf chess.MoveBuffer
	n := e.board.GenMoves(&buf)
	if n == 0 {
		e.send("bestmove 0000")
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
	e.send("bestmove %s", moves[0].move.UCI())
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
// its table from the cache directory if a file exists (never generates).
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
	e.out.Flush()
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
