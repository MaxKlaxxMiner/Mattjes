package main

import (
	"fmt"
	"strings"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
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
// Options is a set of letters: m = mobility initialisation, e/E/x/X = 1+epsilon
// thresholds with epsilon 1/8, 1/2, 1, 2, f = depth-free final entries,
// t = endgame tables as oracle, i = iterative deepening.
func matepnSolve(maxMateIn int, sizeMB int, codec string, options string) {
	runMatepn(func(p chess.MatePosition) bool { return p.MateIn <= maxMateIn }, sizeMB, codec, options)
}

func matepnSolveNamed(name string, sizeMB int, codec string, options string) {
	runMatepn(func(p chess.MatePosition) bool { return p.Name == name }, sizeMB, codec, options)
}

func runMatepn(selected func(p chess.MatePosition) bool, sizeMB int, codecName string, options string) {
	has := func(o byte) bool { return strings.IndexByte(options, o) >= 0 }
	mobility, final, tables, iterative := has('m'), has('f'), has('t'), has('i')
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
	if tables {
		oracle = mateab.Tables{Set: egtbLoadOrGenerate(12)}
		title = "endgame tables"
	}
	mode := "single depth 2N-1"
	if iterative {
		mode = "iterative deepening"
	}
	fmt.Printf("=== matepn / df-pn, %s, direct-mapped TT %d MB (%d value bits), codec %s, mobility %v, epsilon %d/8, final %v, %s ===\n", mode, sizeMB, table.ValueBits(), codec.Name(), mobility, epsilon, final, title)
	ok := true
	var totalNodes uint64
	var totalTime time.Duration
	for i, p := range chess.MatePositions {
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
		s.Epsilon, s.Final = epsilon, final
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
