package main

import (
	"fmt"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/perft"
)

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
