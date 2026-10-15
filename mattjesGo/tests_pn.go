package main

import (
	"fmt"
	"strings"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mateab"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/matepn"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/tt"
)

// --- milestone 4, step 4: df-pn ---

// matepnSolve runs the reference positions up to maxMateIn moves with df-pn:
// a direct-mapped table of sizeMB, pn/dn packed by codec ("sat" or "float"),
// optional mobility initialisation, optional endgame tables as oracle.
// iterative = iterative deepening over odd depths (finds the shortest mate),
// otherwise a single search at the known depth 2*MateIn-1 (the yardstick run).
// The yardstick is the proof DAG of each position (chess.MatePosition.
// ProofPositions, counted by matelist): nodes / proof = how much more than
// the proof was searched.
//
// Options is a set of letters: m = mobility initialisation, l = lazy mobility
// (1/1 leaves, the move count when a node is entered; overrides m), e/E/x/X =
// 1+epsilon thresholds with epsilon 1/8, 1/2, 1, 2, f = depth-free final
// entries, t = endgame tables as oracle (cached tables of the positions' own
// materials included; beyond the base they hold buckets of egtb.FileStep
// moves, the search resolves the exact distance from the bounds), i =
// iterative deepening.
func matepnSolve(maxMateIn int, sizeMB int, codec string, options string) {
	runMatepn(chess.MatePositions, func(p chess.MatePosition) bool { return p.MateIn <= maxMateIn }, sizeMB, codec, options)
}

func matepnSolveNamed(name string, sizeMB int, codec string, options string) {
	runMatepn(chess.MatePositions, func(p chess.MatePosition) bool { return p.Name == name }, sizeMB, codec, options)
}

// matepnSolveFEN runs a single position that is not in chess.MatePositions
// (the regression list must not change), such as the five-piece KRR-KN of
// test-positions.md with its cached table.
func matepnSolveFEN(name, fen string, mateIn int, sizeMB int, codec string, options string) {
	runMatepn([]chess.MatePosition{{Name: name, FEN: fen, MateIn: mateIn}}, func(chess.MatePosition) bool { return true }, sizeMB, codec, options)
}

func runMatepn(positions []chess.MatePosition, selected func(p chess.MatePosition) bool, sizeMB int, codecName string, options string) {
	has := func(o byte) bool { return strings.IndexByte(options, o) >= 0 }
	lazy, final, tables, iterative, mated := has('l'), has('f'), has('t'), has('i'), has('M')
	mobility := has('m') && !lazy
	mobilityName := "off"
	switch {
	case lazy:
		mobilityName = "lazy"
	case mobility:
		mobilityName = "on"
	}
	epsilon := 0 // in eighths: e = 1/8, E = 1/2, x = 1, X = 2
	for _, o := range []struct {
		letter byte
		eps    int
	}{{'e', 1}, {'E', 4}, {'x', 8}, {'X', 16}} {
		if has(o.letter) {
			epsilon = o.eps
		}
	}
	table := tt.New(sizeMB)
	var codec matepn.Codec
	switch codecName {
	case "sat":
		codec = matepn.Saturating{Bits: table.ValueBits()}
	case "float":
		codec = matepn.Float{Bits: table.ValueBits()}
	default:
		panic("codec sat or float")
	}
	var oracle mateab.Oracle = mateab.Material{}
	title := "material oracle"
	var set *egtb.Set
	if tables {
		set = egtbLoadOrGenerate(12)
		egtbLoadForPositions(set, positions, selected)
		oracle = mateab.Tables{Set: set}
		title = "endgame tables"
	}
	mode := "single depth 2N-1"
	if iterative {
		mode = "iterative deepening"
	}
	fmt.Printf("=== matepn / df-pn, %s, direct-mapped TT %d MB (%d value bits), codec %s, mobility %s, epsilon %d/8, final %v, mated %v, %s ===\n", mode, sizeMB, table.ValueBits(), codec.Name(), mobilityName, epsilon, final, mated, title)
	ok := true
	var totalNodes uint64
	var totalTime time.Duration
	for i, p := range positions {
		if !selected(p) {
			continue
		}
		fmt.Printf("[%d] %s  %s  mate in %d\n", i+1, p.Name, p.FEN, p.MateIn)
		b, err := bitboard.FromFEN(p.FEN)
		if err != nil {
			panic(err)
		}
		table.Clear()
		table.ResetStats()
		s := matepn.New(oracle, table, codec, mobility)
		s.LazyMobility = lazy
		s.Epsilon, s.Final, s.Mated = epsilon, final, mated
		start := time.Now()
		depthStart := start
		s.Progress = func(nodes uint64) {
			st := table.Counters()
			fmt.Printf("              ... %s nodes, %s, tt %s stores, %s replaced\n", perftGroup(nodes), fmtMs(time.Since(depthStart)), perftGroup(st.Stores), perftGroup(st.Replaced))
		}
		var r matepn.Result
		if iterative {
			last := uint64(0)
			r = s.SolveShortest(&b, 2*p.MateIn-1, func(plies int, r matepn.Result) {
				verdict := "no mate"
				if r.Proven {
					verdict = fmt.Sprintf("mate in %d", (plies+1)/2)
				}
				fmt.Printf("    depth %2d: %-10s %14s nodes %9s\n", plies, verdict, perftGroup(r.Nodes-last), fmtMs(time.Since(depthStart)))
				last = r.Nodes
				depthStart = time.Now()
			})
		} else {
			r = s.Solve(&b, 2*p.MateIn-1)
		}
		elapsed := time.Since(start)
		totalNodes += r.Nodes
		totalTime += elapsed
		st := table.Counters()
		switch {
		case r.Proven:
			fmt.Printf("    proven: %s nodes in %s, %s leaves, %s tt hits, %s final hits, %s stores, %s replaced, fill %.1f%%\n",
				perftGroup(r.Nodes), fmtMs(elapsed), perftGroup(r.Leaves), perftGroup(r.TTHits), perftGroup(r.FinalHits), perftGroup(st.Stores), perftGroup(st.Replaced), 100*float64(table.Used())/float64(table.Slots()))
			if set != nil {
				// the packed tables beyond the base (docs/m5-endgame-tables.md, "Blockformat")
				if hits, misses, distinct, blocks := set.CacheStats(); hits+misses > 0 {
					fmt.Printf("    block cache %d MB: %s hits, %s misses, %s of %s blocks touched\n", egtb.CacheBytes>>20, perftGroup(hits), perftGroup(misses), perftGroup(distinct), perftGroup(blocks))
					set.ResetCacheStats()
				}
			}
			if r.MatePlies > 0 {
				verdict := "the shortest"
				if r.MatePlies != 2*p.MateIn-1 {
					verdict = fmt.Sprintf("shortest is %d", 2*p.MateIn-1)
					if iterative {
						verdict = "FAIL: " + verdict
						ok = false
					}
				}
				fmt.Printf("    proof tree: mate in %d plies (%s)  pv %s\n", r.MatePlies, verdict, pvString(r.PV))
			} else {
				fmt.Println("    proof tree: incomplete in the table (entries overwritten)")
			}
			if p.ProofPositions > 0 {
				fmt.Printf("    yardstick: proof DAG %s positions, nodes / proof = %.1f\n", perftGroup(uint64(p.ProofPositions)), float64(r.Nodes)/float64(p.ProofPositions))
			}
		case r.Disproven:
			fmt.Printf("    FAIL: disproven after %s nodes in %s\n", perftGroup(r.Nodes), fmtMs(elapsed))
			ok = false
		default:
			fmt.Printf("    FAIL: neither proven nor disproven after %s nodes\n", perftGroup(r.Nodes))
			ok = false
		}
	}
	fmt.Printf("--- total: %s nodes in %s", perftGroup(totalNodes), fmtMs(totalTime))
	if ok {
		fmt.Println("  [all ok]")
	} else {
		fmt.Println("  [FAILURES]")
	}
	fmt.Println()
}
