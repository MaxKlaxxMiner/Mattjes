package main

import (
	"fmt"
	"strconv"
	"strings"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mateab"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/rollout"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/syzygy"
)

// --- rollouts with Patricia's network as the policy (package rollout) ---

// testInt is testArg as a number (def when missing or not a number).
func testInt(i int, def int) int {
	if v, err := strconv.Atoi(testArg(i, "")); err == nil {
		return v
	}
	return def
}

// rolloutOracle builds the oracle of the test bench options: t = the
// four-piece tables, s = Syzygy WDL on top, else dead material only.
func rolloutOracle(options string) mateab.Oracle {
	has := func(o byte) bool { return strings.IndexByte(options, o) >= 0 }
	if !has('t') && !has('s') {
		return nil
	}
	oracle := mateab.Tables{Set: egtbLoadOrGenerate(12)}
	if has('s') {
		tb, err := syzygy.Open(syzygyDefaultDir)
		if err != nil {
			fmt.Printf("syzygy: %v\n", err)
		} else {
			oracle.WDL = tb
		}
	}
	return oracle
}

// rolloutPositions plays every root move of the selected WDL test positions
// out (names as in wdllistSolve) and compares the outcomes with the
// recorded win/draw/loss counts: the quality of the rollout as a guess.
// Options: t/s the oracle, v the line of every root move. maxPlies caps a
// rollout.
func rolloutPositions(names string, options string, maxPlies int) {
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
	opt := rollout.Options{MaxPlies: maxPlies, Oracle: rolloutOracle(options), Depth: 1}
	if has('d') {
		opt.Depth = 2 // the move choice by the opponent's best reply
	}
	oracleName := "dead material only"
	if has('s') {
		oracleName = "four-piece tables and Syzygy WDL"
	} else if has('t') {
		oracleName = "four-piece tables"
	}
	fmt.Printf("=== rollouts, Patricia's net as the policy of both sides, depth %d, %s, at most %d plies ===\n", opt.Depth, oracleName, maxPlies)
	totalMoves, totalExact := 0, 0
	var totalTime time.Duration
	for i, p := range chess.WdlPositions {
		if !selected(p) {
			continue
		}
		b, err := bitboard.FromFEN(p.FEN)
		if err != nil {
			panic(err)
		}
		start := time.Now()
		var buf chess.MoveBuffer
		n := b.GenMoves(&buf)
		var count [4]int // by Outcome for the root's side
		tableHits, plies, evals := 0, 0, 0
		for _, m := range buf[:n] {
			c := b
			c.DoMove(m)
			r := rollout.Play(&c, opt)
			// the outcome is for the opponent (to move after m): invert it
			o := r.Outcome
			switch o {
			case rollout.Win:
				o = rollout.Loss
			case rollout.Loss:
				o = rollout.Win
			}
			count[o]++
			if r.Reason == rollout.Table && len(r.Line) == 0 {
				tableHits++
			}
			plies += len(r.Line)
			evals += r.Evals
			if has('v') {
				fmt.Printf("      %-6s %-7s %-10s %3d plies  %s\n", m.UCI(), o, r.Reason, len(r.Line), pvString(r.Line))
			}
		}
		elapsed := time.Since(start)
		totalTime += elapsed
		// how many of the guesses can be right at best: the overlap of the
		// guessed and the recorded counts per class
		exact := min(count[rollout.Win], p.Wins) + min(count[rollout.Draw], p.Draws) + min(count[rollout.Loss], p.Losses)
		totalMoves += n
		totalExact += exact
		fmt.Printf("[%d] %-34s guessed %2d/%2d/%2d (%d unknown) expected %2d/%2d/%2d, at most %2d of %2d right, %2d table hits at once, %5.1f plies avg, %7d evals, %s\n",
			i+1, p.Name, count[rollout.Win], count[rollout.Draw], count[rollout.Loss], count[rollout.Unknown], p.Wins, p.Draws, p.Losses, exact, n, tableHits, float64(plies)/float64(max(n, 1)), evals, fmtMs(elapsed))
	}
	fmt.Printf("--- rollouts: at most %d of %d root moves guessed right, %s\n\n", totalExact, totalMoves, fmtMs(totalTime))
}

// rolloutLine plays one FEN out and prints the line.
func rolloutLine(fen string, options string, maxPlies int) {
	b, err := bitboard.FromFEN(fen)
	if err != nil {
		fmt.Println(err)
		return
	}
	start := time.Now()
	depth := 1
	if strings.IndexByte(options, 'd') >= 0 {
		depth = 2
	}
	r := rollout.Play(&b, rollout.Options{MaxPlies: maxPlies, Oracle: rolloutOracle(options), Depth: depth})
	fmt.Printf("%s\n  %s by %s after %d plies, %d evals, %s\n  %s\n", fen, r.Outcome, r.Reason, len(r.Line), r.Evals, fmtMs(time.Since(start)), pvString(r.Line))
}
