package main

import (
	"fmt"
	"math/bits"
	"strings"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mateab"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/matelist"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/perft"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/tt"
)

// --- milestone 4, step 2: mate search with mate window, no TT ---

// mateabSolve runs the reference mate positions up to maxMateIn moves with the
// depth-first search, without a table (sizeMB 0) or with a direct-mapped or
// bucket table of sizeMB. The mate must appear exactly at depth 2*MateIn-1
// plies: earlier would contradict the tablebase, later or never is a miss.
// Prints nodes, time, table hits and the principal variation per depth. The
// table is cleared per position, so Go and Rust can be compared node by node.
func mateabSolve(maxMateIn int, sizeMB int, bucketed bool) {
	runMateab(func(p chess.MatePosition) bool { return p.MateIn <= maxMateIn }, sizeMB, bucketed, nil)
}

// mateabSolveNamed runs a single reference position by name, for the long ones.
func mateabSolveNamed(name string, sizeMB int, bucketed bool) {
	runMateab(func(p chess.MatePosition) bool { return p.Name == name }, sizeMB, bucketed, nil)
}

// mateabSolveTables runs the reference positions with the endgame tables as
// oracle (milestone 5): positions with up to four pieces are answered at the
// root, the five-piece ones as soon as a capture reaches a table.
func mateabSolveTables(maxMateIn int, sizeMB int, bucketed bool) {
	set := egtbLoadOrGenerate(12)
	runMateab(func(p chess.MatePosition) bool { return p.MateIn <= maxMateIn }, sizeMB, bucketed, set)
}

func mateabSolveTablesNamed(name string, sizeMB int, bucketed bool) {
	set := egtbLoadOrGenerate(12)
	runMateab(func(p chess.MatePosition) bool { return p.Name == name }, sizeMB, bucketed, set)
}

func runMateab(selected func(p chess.MatePosition) bool, sizeMB int, bucketed bool, tables *egtb.Set) {
	var table transTable
	title := "no TT"
	switch {
	case sizeMB > 0 && bucketed:
		table = tt.NewBuckets(sizeMB)
		title = fmt.Sprintf("4-way bucket TT %d MB", sizeMB)
	case sizeMB > 0:
		table = tt.New(sizeMB)
		title = fmt.Sprintf("direct-mapped TT %d MB", sizeMB)
	}
	var oracle mateab.Oracle = mateab.Material{}
	if tables != nil {
		oracle = mateab.Tables{Set: tables}
		title += ", endgame tables"
	}
	fmt.Printf("=== mateab / mate search with mate window, iterative deepening, %s ===\n", title)
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
		var s *mateab.Searcher
		if table != nil {
			if mateab.ValueBits > bits.Len64(table.MaxValue()) {
				panic("table too small for the mateab value layout")
			}
			table.Clear()
			table.ResetStats()
			s = mateab.New(oracle, table)
		} else {
			s = mateab.New(oracle, nil)
		}
		start := time.Now()
		lastNodes := uint64(0)
		depthStart := start
		s.Progress = func(nodes uint64) {
			line := fmt.Sprintf("              ... %s nodes, %s", perftGroup(nodes), fmtMs(time.Since(depthStart)))
			if table != nil {
				st := table.Counters()
				line += fmt.Sprintf(", tt %s stores, %s replaced", perftGroup(st.Stores), perftGroup(st.Replaced))
			}
			fmt.Println(line)
		}
		r := s.Solve(&b, 2*p.MateIn-1, func(plies int, r mateab.Result) {
			now := time.Now()
			if r.MatePlies != 0 {
				pv := pvString(r.PV)
				if len(r.PV) < r.MatePlies {
					pv += " ..."
				}
				fmt.Printf("    depth %2d: mate in %d  %14s nodes %9s  pv %s\n", plies, (r.MatePlies+1)/2, perftGroup(r.Nodes-lastNodes), fmtMs(now.Sub(depthStart)), pv)
			} else {
				fmt.Printf("    depth %2d: no mate    %14s nodes %9s\n", plies, perftGroup(r.Nodes-lastNodes), fmtMs(now.Sub(depthStart)))
			}
			lastNodes = r.Nodes
			depthStart = now
		})
		elapsed := time.Since(start)
		totalNodes += r.Nodes
		totalTime += elapsed
		switch {
		case r.MatePlies == 0:
			fmt.Printf("    FAIL: no mate found within %d plies\n", 2*p.MateIn-1)
			ok = false
		case r.MatePlies != 2*p.MateIn-1:
			fmt.Printf("    FAIL: mate in %d plies, expected %d\n", r.MatePlies, 2*p.MateIn-1)
			ok = false
		default:
			fmt.Printf("    ok: %s nodes in %s", perftGroup(r.Nodes), fmtMs(elapsed))
			if tables != nil {
				fmt.Printf(", %s table hits", perftGroup(r.OracleHits))
			}
			if table != nil {
				st := table.Counters()
				fmt.Printf(", tt: %s probes, %s hits ending the node, %s stores, %s replaced, fill %.1f%%",
					perftGroup(st.Probes), perftGroup(r.TTHits), perftGroup(st.Stores), perftGroup(st.Replaced), 100*float64(table.Used())/float64(table.Slots()))
			}
			fmt.Println()
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

// --- milestone 4: list-based search (breadth-first enumeration + retrograde) ---

// matelistSolve runs the reference positions up to maxMateIn (or a single one by
// name when name is not empty) with the list-based search: every reachable
// position enumerated once, mate distances resolved backwards. Positions whose
// reachable graph exceeds maxPositions are reported as skipped.
func matelistSolve(name string, maxMateIn int, maxPlies int, maxPositions int) {
	fmt.Printf("=== matelist / breadth-first enumeration + retrograde, horizon %d plies, up to %s positions ===\n", maxPlies, perftGroup(uint64(maxPositions)))
	ok := true
	var totalTime time.Duration
	for i, p := range chess.MatePositions {
		if (name != "" && p.Name != name) || (name == "" && p.MateIn > maxMateIn) {
			continue
		}
		fmt.Printf("[%d] %s  %s  mate in %d\n", i+1, p.Name, p.FEN, p.MateIn)
		b, err := bitboard.FromFEN(p.FEN)
		if err != nil {
			panic(err)
		}
		start := time.Now()
		r, err := matelist.Solve(&b, maxPlies, maxPositions, func(line string) {
			fmt.Printf("    %s  %s\n", line, fmtMs(time.Since(start)))
		})
		elapsed := time.Since(start)
		totalTime += elapsed
		if err != nil {
			fmt.Printf("    skipped: %v (%s)\n", err, fmtMs(elapsed))
			continue
		}
		fmt.Printf("    %s positions (%s expanded), %s edges, %d plies, %s resolved, longest mate %d plies\n",
			perftGroup(uint64(r.Positions)), perftGroup(uint64(r.Expanded)), perftGroup(uint64(r.Edges)), r.Plies, perftGroup(uint64(r.Resolved)), r.MaxLevel)
		switch {
		case r.MatePlies == 0:
			fmt.Printf("    FAIL: no mate found (%s)\n", fmtMs(elapsed))
			ok = false
		case r.MatePlies != 2*p.MateIn-1:
			fmt.Printf("    FAIL: mate in %d plies, expected %d (%s)\n", r.MatePlies, 2*p.MateIn-1, fmtMs(elapsed))
			ok = false
		default:
			fmt.Printf("    ok: mate in %d in %s, proof DAG %s of %s positions (%.1f%%)  pv %s\n", (r.MatePlies+1)/2, fmtMs(elapsed),
				perftGroup(uint64(r.ProofPositions)), perftGroup(uint64(r.Positions)), 100*float64(r.ProofPositions)/float64(r.Positions), pvString(r.PV))
		}
	}
	fmt.Printf("--- total: %s", fmtMs(totalTime))
	if ok {
		fmt.Println("  [all ok]")
	} else {
		fmt.Println("  [FAILURES]")
	}
	fmt.Println()
}

func pvString(pv []chess.Move) string {
	parts := make([]string, len(pv))
	for i, m := range pv {
		parts[i] = m.UCI()
	}
	return strings.Join(parts, " ")
}

// --- milestone 4, step 1: generator extensions for the mate search ---

// bitboardPerftDetailed compares the leaf classification (captures, en passant,
// castles, promotions, checks, discovered and double checks, mates) with the
// chessprogramming.org tables for every position that has them. This verifies
// GivesCheck/checkersAfter and HasMoves against foreign data.
func bitboardPerftDetailed(maxNodes uint64) {
	fmt.Println("=== bitboard / perft detail columns vs chessprogramming.org ===")
	ok := true
	var totalNodes uint64
	var totalTime time.Duration
	for i, p := range chess.PerftPositions {
		if p.Details == nil {
			continue
		}
		fmt.Printf("[%d] %s\n    %s\n", i+1, p.Name, p.FEN)
		fmt.Printf("    %5s %16s %12s %8s %10s %10s %11s %9s %7s %9s  %s\n", "depth", "nodes", "captures", "ep", "castles", "promos", "checks", "discov", "double", "mates", "")
		b, err := bitboard.FromFEN(p.FEN)
		if err != nil {
			panic(err)
		}
		for depth := 1; depth <= len(p.Nodes) && depth <= len(p.Details); depth++ {
			if p.Nodes[depth-1] > maxNodes {
				break
			}
			start := time.Now()
			d := bitboard.PerftDetailed(&b, depth)
			elapsed := time.Since(start)
			totalNodes += d.Nodes
			totalTime += elapsed

			want := p.Details[depth-1]
			status := "ok"
			fail := func(name string, got, exp uint64) {
				if exp != chess.Unknown && got != exp {
					status = fmt.Sprintf("FAIL %s got %d expected %d", name, got, exp)
					ok = false
				}
			}
			fail("nodes", d.Nodes, p.Nodes[depth-1])
			fail("captures", d.Captures, want.Captures)
			fail("ep", d.EnPassant, want.EnPassant)
			fail("castles", d.Castles, want.Castles)
			fail("promotions", d.Promotions, want.Promotions)
			fail("checks", d.Checks, want.Checks)
			fail("discovery", d.DiscoveryChecks, want.DiscoveryChecks)
			fail("double", d.DoubleChecks, want.DoubleChecks)
			fail("checkmates", d.Checkmates, want.Checkmates)
			fmt.Printf("    %5d %16s %12s %8s %10s %10s %11s %9s %7s %9s  %s %s\n",
				depth, perftGroup(d.Nodes), perftGroup(d.Captures), perftGroup(d.EnPassant), perftGroup(d.Castles), perftGroup(d.Promotions),
				perftGroup(d.Checks), perftGroup(d.DiscoveryChecks), perftGroup(d.DoubleChecks), perftGroup(d.Checkmates), status, fmtMs(elapsed))
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

// bitboardMateVerify walks all reference positions and checks at every node that
// HasMoves, IsMate and IsStalemate agree with GenMoves + InCheck, and for every
// move that GivesCheck agrees with playing the move and asking InCheck, and
// that GenChecks returns exactly the checking moves.
func bitboardMateVerify(maxNodes uint64) {
	var mismatches uint64
	var walk func(b *bitboard.Board, depth int)
	walk = func(b *bitboard.Board, depth int) {
		var buf, checks chess.MoveBuffer
		n := b.GenMoves(&buf)
		inCheck := b.InCheck()
		if b.HasMoves() != (n > 0) || b.IsMate() != (n == 0 && inCheck) || b.IsStalemate() != (n == 0 && !inCheck) {
			mismatches++
		}
		nChecks := b.GenChecks(&checks)
		found := 0
		s := b.State()
		for i := 0; i < n; i++ {
			gives := b.GivesCheck(buf[i])
			b.DoMove(buf[i])
			if gives != b.InCheck() {
				mismatches++
			}
			if gives {
				if found < nChecks && checks[found] == buf[i] {
					found++
				} else {
					mismatches++
				}
			}
			if depth > 1 {
				walk(b, depth-1)
			}
			b.UndoMove(buf[i], s)
		}
		if found != nChecks {
			mismatches++
		}
	}
	perft.Run("bitboard / HasMoves, IsMate, GivesCheck, GenChecks == GenMoves + DoMove + InCheck at every node", func(fen string, depth int) (uint64, error) {
		b, err := bitboard.FromFEN(fen)
		if err != nil {
			return 0, err
		}
		before := mismatches
		walk(&b, depth)
		if mismatches != before {
			return 0, fmt.Errorf("%d mismatches", mismatches-before)
		}
		return bitboard.PerftRecursive(&b, depth), nil
	}, maxNodes)
	fmt.Printf("total mismatches: %d\n\n", mismatches)
}
