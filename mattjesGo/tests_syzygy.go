package main

import (
	"fmt"
	"math/rand"
	"os"
	"path/filepath"
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
func syzygyBuild(names string, workers int, opt pairs.Options, bo syzygy.BuildOptions, mode string, verbose bool) {
	set := egtbLoadOrGenerate(workers)
	fmt.Printf("=== own %s tables: blocks %d bytes, %d pairs per pass, min count %d, %d symbols, symbol length %d ===\n", mode, opt.BlockBytes, opt.PairsPerPass, opt.MinCount, opt.MaxSymbols, opt.MaxSymbolLen)
	var sumOurs, sumSyzygy int64
	start := time.Now()
	// materials beyond the base are generated exactly in memory (a minute for a five-piece table; the quantised cache file cannot serve)
	tables := append([]*egtb.Table(nil), set.Base()...)
	var canonical []string // the names as the tables carry them ("KQKBN" is KBNKQ)
	for _, name := range strings.Split(names, ";") {
		if m, err := egtb.Parse(strings.TrimSpace(name)); err == nil {
			canonical = append(canonical, m.Name())
		}
	}
	if names != "" {
		names = strings.Join(canonical, ";")
	}
	for _, name := range strings.Split(names, ";") {
		if m, err := egtb.Parse(strings.TrimSpace(name)); err == nil && m.Pieces() > 4 {
			t := set.AddMaterial(m)
			st := set.Generate(t, workers, func(line string) {
				if line[0] != ' ' {
					fmt.Println(line)
				}
			})
			fmt.Printf("%s: generated in %.1f s\n", m.Name(), st.Duration.Seconds())
			tables = append(tables, t)
		}
	}
	for _, t := range tables {
		if names != "" && !strings.Contains(";"+names+";", ";"+t.Mat.Name()+";") {
			continue
		}
		tStart := time.Now()
		// mode: wdl, or the quantised distances (step 8) as dtm (wins and losses
		// apart, draws stored), dtmd (draws free), dtmu (bucket only, draws free)
		var src *syzygy.Source
		var size int64
		switch mode {
		case "dtm":
			src = syzygy.NewSourceDTM(set, t, egtb.FileStep, true, false, workers)
		case "dtmd":
			src = syzygy.NewSourceDTM(set, t, egtb.FileStep, true, true, workers)
		case "dtmu", "dtmw":
			src = syzygy.NewSourceDTM(set, t, egtb.FileStep, false, true, workers)
		default:
			src = syzygy.NewSource(set, t, workers)
		}
		if mode == "wdl" {
			size = syzygyFileSize(t.Mat)
		} else if info, err := os.Stat(set.TablePath(t)); err == nil {
			size = info.Size() // our quantised DTM file, when present
			fmt.Printf("%s: Syzygy DTZ %d bytes, our quantised DTM file %d bytes\n", t.Mat.Name(), syzygyDtzSize(t.Mat), size)
		}
		var progress func(string)
		if verbose {
			progress = func(line string) { fmt.Println("    " + line) }
		}
		tbo := bo
		if mode == "dtmw" {
			// the DTM with the orders the WDL chose (no sampling): the WDL is built first
			wStart := time.Now()
			wdl := syzygy.Build(syzygy.NewSource(set, t, workers), opt, bo, workers, progress)
			fmt.Printf("%-6s WDL %8d bytes in %.1f s, orders %v\n", t.Mat.Name(), wdl.Bytes(), time.Since(wStart).Seconds(), wdl.Orders())
			tbo.Orders = wdl.Orders()
			tStart = time.Now()
		}
		b := syzygy.Build(src, opt, tbo, workers, progress)
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
// all) into dir ("" = the cache folder's wdl subfolder) and prints the sizes.
func syzygyOwnWrite(names string, workers int, opt pairs.Options, bo syzygy.BuildOptions, dir string) {
	set := egtbLoadOrGenerate(workers)
	start := time.Now()
	total := 0
	for _, t := range set.Base() {
		if names != "" && !strings.Contains(";"+names+";", ";"+t.Mat.Name()+";") {
			continue
		}
		tStart := time.Now()
		path := set.WdlFile(t.Mat)
		if dir != "" {
			path = filepath.Join(dir, filepath.Base(path))
		}
		b, err := syzygy.WriteWDL(set, t, path, opt, bo, workers, nil)
		if err != nil {
			panic(err)
		}
		info, _ := os.Stat(path)
		fmt.Printf("%-6s %4d candidates, %8d bytes in %d streams, file %8d bytes (%.1f s)\n", t.Mat.Name(), b.Candidates, b.Bytes(), len(b.Streams), info.Size(), time.Since(tStart).Seconds())
		total += int(info.Size())
	}
	folder := set.WdlPath()
	if dir != "" {
		folder = dir
	}
	fmt.Printf("written %d bytes to %s in %.1f s\n\n", total, folder, time.Since(start).Seconds())
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

// syzygyDtmCheck writes the WDL and DTM files of a material (generated
// exactly in memory when it is beyond the base) into dir and probes random
// positions through the oracle (base plus the two files) against the exact
// values: the verdict must match and the exact distance lie in the bounds.
func syzygyDtmCheck(name string, dir string, samples int, workers int) {
	m, err := egtb.Parse(name)
	if err != nil {
		panic(err)
	}
	gen := egtbLoadOrGenerate(workers)
	var t *egtb.Table
	if m.Pieces() <= 4 {
		t = gen.Find(m.Name())
	} else {
		t = gen.AddMaterial(m)
		st := gen.Generate(t, workers, func(line string) {
			if line[0] != ' ' {
				fmt.Println(line)
			}
		})
		fmt.Printf("%s: generated in %.1f s\n", m.Name(), st.Duration.Seconds())
	}
	opt, bo := pairs.Options{Workers: workers}, syzygy.BuildOptions{}
	start := time.Now()
	wdlPath := filepath.Join(dir, fmt.Sprintf("%d-%s.wdl", m.Pieces(), m.Name()))
	wb, err := syzygy.WriteWDL(gen, t, wdlPath, opt, bo, workers, nil)
	if err != nil {
		panic(err)
	}
	fmt.Printf("%s: WDL %d bytes in %.1f s\n", m.Name(), wb.Bytes(), time.Since(start).Seconds())
	start = time.Now()
	dtmPath := filepath.Join(dir, fmt.Sprintf("%d-%s.dtm", m.Pieces(), m.Name()))
	db, err := syzygy.WriteDTM(gen, t, dtmPath, opt, bo, workers, nil)
	if err != nil {
		panic(err)
	}
	fmt.Printf("%s: DTM %d bytes in %.1f s (step %d, %d symbols)\n", m.Name(), db.Bytes(), time.Since(start).Seconds(), db.Step, db.Alphabet)
	// the oracle's own set is empty (registered tables without values answer
	// nothing), so every probe goes through the WDL and DTM files; the
	// prober resolves captures with the full base
	tb := syzygy.New()
	if n, err := tb.AddOwn(dir); err != nil || n == 0 {
		panic(fmt.Sprintf("no WDL file registered (%v)", err))
	}
	if n, err := tb.AddDTM(dir); err != nil || n == 0 {
		panic(fmt.Sprintf("no DTM file registered (%v)", err))
	}
	tb.SetBase(gen)
	oracle := mateab.Tables{Set: egtb.NewSet(), WDL: tb}
	pieces := []chess.Piece{chess.WhiteKing, chess.BlackKing}
	pieces = append(pieces, materialPieces(m)...)
	squares := make([]chess.Pos, len(pieces))
	rng := rand.New(rand.NewSource(7))
	checked, bad, bounded := 0, 0, 0
	t0 := time.Now()
	for checked < samples {
		used := uint64(0)
		for i, p := range pieces {
			for {
				sq := rng.Intn(64)
				if used&(1<<sq) != 0 || (p.Type() == chess.Pawn && (sq < 8 || sq >= 56)) {
					continue
				}
				used |= 1 << sq
				squares[i] = chess.Pos(sq)
				break
			}
		}
		b := bitboard.FromPieces(rng.Intn(2) == 0, squares, pieces)
		if b.OpponentInCheck() {
			continue
		}
		exact, ok := gen.Lookup(&b)
		if !ok {
			continue
		}
		verdict, lo, hi := oracle.Probe(&b)
		checked++
		want := mateab.Draw
		if exact.IsWin() {
			want = mateab.Win
		} else if exact.IsLoss() {
			want = mateab.Loss
		}
		plies := exact.Plies()
		if verdict != want || (want != mateab.Draw && (plies < lo || plies > hi)) {
			bad++
			if bad <= 5 {
				fmt.Printf("  MISMATCH %s: exact %s, oracle %d [%d, %d]\n", b.FEN(), exact, verdict, lo, hi)
			}
		}
		if want != mateab.Draw && hi < mateab.Unbounded {
			bounded++
		}
	}
	el := time.Since(t0)
	verdict := "ok"
	if bad > 0 {
		verdict = fmt.Sprintf("%d MISMATCHES", bad)
	}
	fmt.Printf("%s: %d positions checked, %s, %d wins and losses with DTM bounds, %.0f probes/s\n\n", m.Name(), checked, verdict, bounded, float64(checked)/el.Seconds())
}

// syzygyDecodeCheck round-trips random indices of the candidate orders of
// the base materials ("" = all) through the Syzygy index decoder and the
// encoder (the decoder enumerates stream slices for the candidate
// preselection of large tables).
func syzygyDecodeCheck(names string, maxOrders int, samples int) {
	start := time.Now()
	totalBad := 0
	for _, m := range egtb.All() {
		if names != "" && !strings.Contains(";"+names+";", ";"+m.Name()+";") {
			continue
		}
		checked, bad := syzygy.CheckDecoder(m, maxOrders, samples, 1, func(line string) { fmt.Println(line) })
		verdict := "ok"
		if bad > 0 {
			verdict = fmt.Sprintf("%d MISMATCHES", bad)
		}
		fmt.Printf("%-6s %8d indices round-tripped, %s\n", m.Name(), checked, verdict)
		totalBad += bad
	}
	if totalBad == 0 {
		fmt.Printf("--- decoder: every index round-trips, %s\n\n", time.Since(start).Round(time.Millisecond))
	} else {
		fmt.Printf("--- decoder: %d MISMATCHES\n\n", totalBad)
	}
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
