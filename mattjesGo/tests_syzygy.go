package main

import (
	"fmt"
	"math/rand"
	"os"
	"strings"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mateab"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/pairs"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/syzygy"
)

// --- milestone 7: Syzygy WDL reader ---

// syzygyDefaultDir is the author's Syzygy folder (all three- to six-piece tables).
const syzygyDefaultDir = `C:\Users\MaxKlaxx\Desktop\spiele\chess\table`

// syzygyPositions probes every root move of the WDL test positions and
// compares the win/draw/loss counts with the recorded ones (Stockfish with
// Syzygy): the check for the five- and six-piece tables, which have no
// own table to compare with.
// syzygyProbe prints the WDL probe of each FEN (semicolon separated) and
// the oracle's verdict with the four-piece base on top.
// syzygyBuild builds our own WDL tables of the named base materials
// ("KBNK;KQKR", "" = all) in Syzygy's structure with every candidate piece
// order and prints the best order and size per stream next to the Syzygy
// file (docs/m5-endgame-tables.md, "Eigene WDL-Tabellen"). verbose prints
// every candidate.
func syzygyBuild(names string, workers int, opt pairs.Options, verbose bool) {
	set := egtbLoadOrGenerate(workers)
	fmt.Printf("=== own WDL tables: blocks %d bytes, %d pairs per pass, min count %d, %d symbols, symbol length %d ===\n", opt.BlockBytes, opt.PairsPerPass, opt.MinCount, opt.MaxSymbols, opt.MaxSymbolLen)
	var sumOurs, sumSyzygy int64
	start := time.Now()
	for _, t := range set.Base() {
		if names != "" && !strings.Contains(";"+names+";", ";"+t.Mat.Name()+";") {
			continue
		}
		tStart := time.Now()
		src := syzygy.NewSource(set, t, workers)
		var progress func(string)
		if verbose {
			progress = func(line string) { fmt.Println("    " + line) }
		}
		b := syzygy.Build(src, opt, workers, progress)
		size := syzygyFileSize(t.Mat)
		fmt.Printf("%-6s %9d legal, %4.1f%% capture-decided, %4d candidates: ours %8d, syzygy %8d, ratio %.2f (%.1f s)\n", t.Mat.Name(), src.Legal, 100*float64(src.Decided)/float64(max(src.Legal, 1)), b.Candidates, b.Bytes(), size, float64(b.Bytes())/float64(max(size, 1)), time.Since(tStart).Seconds())
		for i, s := range b.Streams {
			fmt.Printf("       stream %d: order %-12s %9d positions, %8d bytes: %s\n", i, s.Order, s.Size, len(s.Packed), s.Stats)
		}
		sumOurs += int64(b.Bytes())
		sumSyzygy += size
	}
	fmt.Printf("total: ours %d, syzygy %d, ratio %.2f (%.1f s)\n\n", sumOurs, sumSyzygy, float64(sumOurs)/float64(max(sumSyzygy, 1)), time.Since(start).Seconds())
}

// syzygyDescribe prints the layout of the compressed streams of the named
// tables ("KBNvK;KQvKR"): symbols, blocks, index, code lengths (the
// yardstick for package pairs).
func syzygyDescribe(dir string, names string) {
	tb, err := syzygy.Open(dir)
	if err != nil {
		fmt.Printf("syzygy: %v\n", err)
		return
	}
	for _, name := range strings.Split(names, ";") {
		lines, ok := tb.Describe(strings.TrimSpace(name))
		if !ok {
			fmt.Printf("%s: no table\n", name)
			continue
		}
		for _, l := range lines {
			fmt.Println(l)
		}
	}
	fmt.Println()
}

func syzygyProbe(dir string, fens string) {
	tb, err := syzygy.Open(dir)
	if err != nil {
		fmt.Printf("syzygy: %v\n", err)
		return
	}
	oracle := mateab.Tables{Set: egtbLoadOrGenerate(12), WDL: tb}
	for _, fen := range strings.Split(fens, ";") {
		b, err := bitboard.FromFEN(strings.TrimSpace(fen))
		if err != nil {
			fmt.Printf("%s: %v\n", fen, err)
			continue
		}
		v, ok := tb.ProbeWDL(&b)
		verdict, lo, hi := oracle.Probe(&b)
		fmt.Printf("%-50s has %-5v probe %d ok %-5v oracle %v %d..%d\n", strings.TrimSpace(fen), tb.Has(&b), v, ok, verdict, lo, hi)
	}
}

func syzygyPositions(dir string) {
	tb, err := syzygy.Open(dir)
	if err != nil {
		fmt.Printf("syzygy: %v\n", err)
		return
	}
	fails := 0
	for _, p := range chess.WdlPositions {
		b, err := bitboard.FromFEN(p.FEN)
		if err != nil {
			panic(err)
		}
		if !tb.Has(&b) {
			fmt.Printf("%-32s no table\n", p.Name)
			continue
		}
		var buf chess.MoveBuffer
		n := b.GenMoves(&buf)
		wins, draws, losses := 0, 0, 0
		for _, m := range buf[:n] {
			c := b
			c.DoMove(m)
			v, ok := tb.ProbeWDL(&c)
			if !ok {
				fmt.Printf("%-32s move %s: no table\n", p.Name, m.UCI())
				break
			}
			switch {
			case v <= syzygy.BlessedLoss: // the opponent loses: our win
				wins++
			case v >= syzygy.CursedWin:
				losses++
			default:
				draws++
			}
		}
		status := "ok"
		if wins != p.Wins || draws != p.Draws || losses != p.Losses {
			status = fmt.Sprintf("MISMATCH, expected %d/%d/%d", p.Wins, p.Draws, p.Losses)
			fails++
		}
		fmt.Printf("%-32s %2d/%2d/%2d %s\n", p.Name, wins, draws, losses, status)
	}
	if fails == 0 {
		fmt.Println("--- syzygy: every test position agrees with the recorded move counts")
	} else {
		fmt.Printf("--- syzygy: %d MISMATCHES\n", fails)
	}
}

// syzygyVerify compares Syzygy WDL probes with the engine's own tables on
// random legal positions of the named materials (semicolon separated,
// "KQKR;KRRKN"; three- and four-piece ones from the base, larger ones from
// the cache files) and reports mismatches and the probe rate. Cursed wins
// and blessed losses count as win and loss: our tables ignore the
// fifty-move rule.
func syzygyVerify(dir string, names string, samples int) {
	start := time.Now()
	tb, err := syzygy.Open(dir)
	if err != nil {
		fmt.Printf("syzygy: %v\n", err)
		return
	}
	fmt.Printf("syzygy: %d WDL tables in %s, largest %d pieces, scanned in %s\n", tb.Count(), dir, tb.Largest(), time.Since(start).Round(time.Millisecond))
	syzygyVerifyTables(tb, egtbLoadOrGenerate(12), names, samples)
}

// syzygyOwnWrite writes our own WDL tables of the named base materials ("" =
// all) into the cache folder (egtb.WdlDir) and prints the sizes.
func syzygyOwnWrite(names string, workers int, opt pairs.Options) {
	set := egtbLoadOrGenerate(workers)
	start := time.Now()
	total := 0
	for _, t := range set.Base() {
		if names != "" && !strings.Contains(";"+names+";", ";"+t.Mat.Name()+";") {
			continue
		}
		tStart := time.Now()
		path := set.WdlFile(t.Mat)
		b, err := syzygy.WriteWDL(set, t, path, opt, workers, nil)
		if err != nil {
			panic(err)
		}
		info, _ := os.Stat(path)
		fmt.Printf("%-6s %4d candidates, %8d bytes in %d streams, file %8d bytes (%.1f s)\n", t.Mat.Name(), b.Candidates, b.Bytes(), len(b.Streams), info.Size(), time.Since(tStart).Seconds())
		total += int(info.Size())
	}
	fmt.Printf("written %d bytes to %s in %.1f s\n\n", total, set.WdlPath(), time.Since(start).Seconds())
}

// syzygyOwnVerify probes our own WDL tables in the cache folder through the
// Syzygy prober against the base tables (random positions per material,
// "" = every material with a file).
func syzygyOwnVerify(names string, samples int) {
	set := egtbLoadOrGenerate(12)
	tb := syzygy.New()
	n, err := tb.AddOwn(set.WdlPath())
	if err != nil {
		fmt.Printf("own wdl: %v\n", err)
		return
	}
	tb.SetBase(set) // captures into materials without a file
	fmt.Printf("own wdl: %d tables in %s\n", n, set.WdlPath())
	if names == "" {
		var all []string
		for _, t := range set.Base() {
			if _, err := os.Stat(set.WdlFile(t.Mat)); err == nil {
				all = append(all, t.Mat.Name())
			}
		}
		names = strings.Join(all, ";")
	}
	syzygyVerifyTables(tb, set, names, samples)
}

// syzygyOwnProbe probes single positions (semicolon separated FENs) in our
// own WDL tables and prints the base value next to it, for debugging.
func syzygyOwnProbe(fens string) {
	set := egtbLoadOrGenerate(12)
	tb := syzygy.New()
	if _, err := tb.AddOwn(set.WdlPath()); err != nil {
		fmt.Printf("own wdl: %v\n", err)
		return
	}
	tb.SetBase(set)
	for _, fen := range strings.Split(fens, ";") {
		b, err := bitboard.FromFEN(strings.TrimSpace(fen))
		if err != nil {
			fmt.Printf("%s: %v\n", fen, err)
			continue
		}
		own, ok1 := set.Lookup(&b)
		v, ok2 := tb.ProbeWDL(&b)
		fmt.Printf("%s: base %v (%v), own wdl %d (%v)\n", fen, own, ok1, v, ok2)
	}
	fmt.Println()
}

// syzygyVerifyTables compares the WDL tables with the own tables of the
// named materials on random positions.
func syzygyVerifyTables(tb *syzygy.Tables, set *egtb.Set, names string, samples int) {
	start := time.Now()
	rng := rand.New(rand.NewSource(1))
	totalMismatch := 0
	for _, name := range strings.Split(names, ";") {
		name = strings.TrimSpace(name)
		m, err := egtb.Parse(name)
		if err != nil {
			fmt.Printf("%s: %v\n", name, err)
			continue
		}
		if m.Pieces() > 4 && set.Find(m.Name()) == nil {
			t := set.AddMaterial(m)
			if err := set.LoadTable(t, set.TablePath(t)); err != nil {
				fmt.Printf("%s: own table not loaded (%v), skipped\n", name, err)
				continue
			}
		}
		pieces := []chess.Piece{chess.WhiteKing, chess.BlackKing}
		for _, p := range m.White {
			pieces = append(pieces, chess.White|p)
		}
		for _, p := range m.Black {
			pieces = append(pieces, chess.Black|p)
		}
		squares := make([]chess.Pos, len(pieces))
		mismatch, checked, noTable := 0, 0, 0
		probesBefore := tb.Probes.Load()
		t0 := time.Now()
		for checked < samples {
			// random distinct squares, pawns off the first and last rank
			used := uint64(0)
			ok := true
			for i, p := range pieces {
				for {
					sq := rng.Intn(64)
					if used&(1<<sq) != 0 {
						continue
					}
					if p.Type() == chess.Pawn && (sq < 8 || sq >= 56) {
						continue
					}
					used |= 1 << sq
					squares[i] = chess.Pos(sq)
					break
				}
			}
			if !ok {
				continue
			}
			whiteMove := rng.Intn(2) == 0
			b := bitboard.FromPieces(whiteMove, squares, pieces)
			if b.OpponentInCheck() {
				continue
			}
			own, ok := set.Lookup(&b)
			if !ok {
				continue
			}
			var v int
			func() {
				defer func() {
					if r := recover(); r != nil {
						setup := b.Setup()
						fmt.Printf("  PANIC at %s: %v\n", setup.FEN(), r)
						panic(r)
					}
				}()
				v, ok = tb.ProbeWDL(&b)
			}()
			if !ok {
				noTable++
				if noTable > 10 {
					break
				}
				continue
			}
			checked++
			ownClass, syzClass := 0, 0
			if own.IsWin() {
				ownClass = 1
			} else if own.IsLoss() {
				ownClass = -1
			}
			if v >= syzygy.CursedWin {
				syzClass = 1
			} else if v <= syzygy.BlessedLoss {
				syzClass = -1
			}
			if ownClass != syzClass {
				mismatch++
				if mismatch <= 5 {
					setup := b.Setup()
					fmt.Printf("  MISMATCH %s: own %d (%s), syzygy %d\n", setup.FEN(), own, map[int]string{1: "win", 0: "draw", -1: "loss"}[ownClass], v)
				}
			}
		}
		probes := tb.Probes.Load() - probesBefore
		el := time.Since(t0)
		status := "ok"
		if mismatch > 0 {
			status = fmt.Sprintf("%d MISMATCHES", mismatch)
		}
		if noTable > 10 {
			status = "no Syzygy table"
		}
		fmt.Printf("%-8s %7d positions checked, %s, %d probes (with captures) in %s, %.0f probes/s\n", name, checked, status, probes, el.Round(time.Millisecond), float64(probes)/el.Seconds())
		totalMismatch += mismatch
	}
	if totalMismatch == 0 {
		fmt.Printf("--- syzygy: all materials agree with the own tables, %s\n", time.Since(start).Round(time.Millisecond))
	} else {
		fmt.Printf("--- syzygy: %d MISMATCHES\n", totalMismatch)
	}
}
