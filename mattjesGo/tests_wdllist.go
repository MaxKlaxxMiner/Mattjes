package main

import (
	"fmt"
	"strings"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mateab"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/syzygy"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/wdllist"
)

// --- milestone 7, step 2: WDL search on the explicit graph ---

// wdllistSolve runs the WDL test positions with the explicit graph: first
// "the side to move wins", then "the opponent wins"; both disproven = draw.
// names as in wdlpnSolve (semicolon separated, "wins", "draws", "" = all).
// Options: B = brute force (the whole reachable graph, no guidance), p =
// the proof-number descent alone (no certificate builder racing it),
// e/E/x/X = epsilon 1/8, 1/2, 1, 2 for the guided descent, w = stored
// numbers almost unclipped (the default clips at 1022 like wdlpn's sat
// codec), l = numbers on the log scale (sums cannot overflow), C = the
// certificate builder without the perpetual-check chase, v = progress
// lines.
func wdllistSolve(names string, options string) {
	has := func(o byte) bool { return strings.IndexByte(options, o) >= 0 }
	selected := func(p chess.WdlPosition) bool {
		switch names {
		case "":
			return true
		case "wins":
			return p.Result != chess.Draw
		case "draws":
			return p.Result == chess.Draw
		}
		for _, n := range strings.Split(names, ";") {
			if strings.TrimSpace(n) == p.Name {
				return true
			}
		}
		return false
	}
	opt := wdllist.Options{BruteForce: has('B'), NoRace: has('p'), NoChase: has('C'), NoDescent: has('P'), MaxPositions: 16 << 20} // the store is allocated per question and search: 16 Mi positions = 512 MB
	if has('t') || has('s') {
		// t = the four-piece tables as oracle: every capture into them is a
		// terminal, which the draw certificates of five-piece positions need;
		// s = the Syzygy WDL tables on top (the author's folder)
		oracle := mateab.Tables{Set: egtbLoadOrGenerate(12)}
		if has('s') {
			tb, err := syzygy.Open(syzygyDefaultDir)
			if err != nil {
				fmt.Printf("syzygy: %v\n", err)
			} else {
				oracle.WDL = tb
			}
		}
		opt.Oracle = oracle
	}
	for _, o := range []struct {
		letter byte
		eps    int
	}{{'e', 1}, {'E', 4}, {'x', 8}, {'X', 16}} {
		if has(o.letter) {
			opt.Epsilon = o.eps
		}
	}
	if has('v') {
		opt.Progress = func(line string) { fmt.Printf("        %s\n", line) }
	}
	capName := fmt.Sprintf("linear, stored numbers clipped at %d", wdllist.DefaultNumberCap)
	if has('w') {
		opt.NumberCap = wdllist.WideNumberCap // the measured alternative: stored numbers almost unclipped
		capName = "linear, wide"
	}
	if has('l') {
		opt.LogNumbers = true
		capName = "log scale"
	}
	mode := fmt.Sprintf("proof-number descent racing the certificate builder, epsilon %d/8, numbers %s", opt.Epsilon, capName)
	if opt.NoRace {
		mode = fmt.Sprintf("proof-number descent alone, epsilon %d/8, numbers %s", opt.Epsilon, capName)
	} else if opt.NoDescent {
		mode = "certificate builder alone"
	}
	if opt.NoChase && !opt.NoRace {
		mode += ", no chase"
	}
	if opt.BruteForce {
		mode = "brute force (reachable graph)"
	}
	if has('s') {
		mode += ", four-piece tables and Syzygy WDL as oracle"
	} else if has('t') {
		mode += ", four-piece tables as oracle"
	}
	fmt.Printf("=== wdllist, %s ===\n", mode)
	ok := true
	totalPositions, totalExpansions := 0, 0
	var totalTime time.Duration
	for i, p := range chess.WdlPositions {
		if !selected(p) {
			continue
		}
		fmt.Printf("[%d] %s  %s  expected %s (%d/%d/%d)\n", i+1, p.Name, p.FEN, p.Result, p.Wins, p.Draws, p.Losses)
		b, err := bitboard.FromFEN(p.FEN)
		if err != nil {
			panic(err)
		}
		start := time.Now()
		got := chess.Draw
		undecided := false
		var pv []chess.Move
		positions, expansions := 0, 0
		for q, attacks := range []bool{true, false} {
			question := "side to move wins"
			if !attacks {
				question = "opponent wins"
			}
			qStart := time.Now()
			r := wdllist.Solve(&b, attacks, opt)
			positions += r.Positions
			expansions += r.Expansions
			verdict := "undecided"
			switch {
			case r.Stuck:
				verdict = "STUCK (bug)"
			case r.Aborted:
				verdict = "aborted"
			case r.Outcome == wdllist.Proven:
				verdict = fmt.Sprintf("proven (win in %d plies)", r.WinPlies)
			case r.Outcome == wdllist.Disproven:
				verdict = fmt.Sprintf("disproven (%d safe)", r.Safe)
			}
			visits := perftGroup(r.Visits) + " visits"
			if r.Method == "certificate" {
				visits = perftGroup(r.ChaseNodes) + " chase nodes"
			}
			fmt.Printf("    q%d %-18s %-28s by %-11s %10s positions, %10s expansions, %22s, %s won, %d fixed points, %d resets, %d exhausts, %s\n", q+1, question+":", verdict, r.Method, perftGroup(uint64(r.Positions)), perftGroup(uint64(r.Expansions)), visits, perftGroup(uint64(r.Won)), r.SafeChecks, r.Resets, r.Exhausts, fmtMs(time.Since(qStart)))
			if r.Outcome == wdllist.Proven {
				got, pv = chess.Win, r.PV
				if !attacks {
					got = chess.Loss
				}
				break
			}
			if r.Outcome != wdllist.Disproven {
				undecided = true
				break
			}
		}
		elapsed := time.Since(start)
		totalPositions += positions
		totalExpansions += expansions
		totalTime += elapsed
		verdict := "ok"
		result := got.String()
		if undecided {
			result = "undecided"
		}
		if undecided || got != p.Result {
			verdict = "FAIL"
			ok = false
		}
		fmt.Printf("    result %s (%s): %s positions, %s expansions in %s\n", result, verdict, perftGroup(uint64(positions)), perftGroup(uint64(expansions)), fmtMs(elapsed))
		if len(pv) > 0 {
			fmt.Printf("    line (%d plies): %s\n", len(pv), pvString(pv))
		}
	}
	fmt.Printf("--- total: %s positions, %s expansions in %s", perftGroup(uint64(totalPositions)), perftGroup(uint64(totalExpansions)), fmtMs(totalTime))
	if ok {
		fmt.Println("  [all ok]")
	} else {
		fmt.Println("  [FAILURES]")
	}
	fmt.Println()
}
