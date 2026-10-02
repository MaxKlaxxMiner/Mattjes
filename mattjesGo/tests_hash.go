package main

import (
	"bytes"
	"fmt"
	"slices"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/perft"
)

// --- milestone 2: hash keys (the comparison experiments live in docs/m2-hash-keys.md;
// kept here are the two regression tests) ---

// bitboardHashVerify walks all reference positions and compares the incremental
// Zobrist key with a full recomputation at every node, forwards and backwards.
func bitboardHashVerify(maxNodes uint64) {
	var mismatches uint64
	var walk func(b *bitboard.Board, depth int)
	walk = func(b *bitboard.Board, depth int) {
		if b.Key != b.ZobristFull() {
			mismatches++
		}
		if depth <= 0 {
			return
		}
		var buf chess.MoveBuffer
		n := b.GenMoves(&buf)
		s := b.State()
		for i := 0; i < n; i++ {
			b.DoMove(buf[i])
			walk(b, depth-1)
			b.UndoMove(buf[i], s)
			if b.Key != b.ZobristFull() {
				mismatches++
			}
		}
	}
	perft.Run("bitboard / zobrist incremental == full recompute at every node", func(fen string, depth int) (uint64, error) {
		b, err := bitboard.FromFEN(fen)
		if err != nil {
			return 0, err
		}
		before := mismatches
		walk(&b, depth-1)
		if mismatches != before {
			return 0, fmt.Errorf("%d key mismatches", mismatches-before)
		}
		return bitboard.PerftRecursive(&b, depth), nil
	}, maxNodes)
	fmt.Printf("total key mismatches: %d\n\n", mismatches)
}

// uniqueReference are the distinct positions after n plies from the start position
// (OEIS A083276, en passant only counted when a legal capture exists).
var uniqueReference = []int{20, 400, 5362, 72078, 822518, 9417681, 96400068}

// positionRecord is the exact identity of a position: its PackedFixed record with
// the move counters zeroed.
func positionRecord(b *bitboard.Board) [bitboard.PackedFixedBytes]byte {
	c := *b
	c.HalfmoveClock, c.MoveNumber = 0, 0
	return c.PackedFixedRecord()
}

// bitboardUniquePositions runs a breadth-first search that keeps only distinct
// positions per ply (exact record), compares the counts with OEIS A083276 for the
// start position and reports Zobrist collisions among the distinct positions.
// This is the prototype of the list-based search with deduplication.
func bitboardUniquePositions(fen string, maxDepth int, maxChildren int) {
	root, err := bitboard.FromFEN(fen)
	if err != nil {
		panic(err)
	}
	isStart := fen == chess.StartFEN
	fmt.Printf("=== unique positions per ply ===\n    %s\n", fen)
	fmt.Printf("    %4s %14s %12s %10s %8s %9s %10s\n", "ply", "children", "unique", "reference", "time", "memory", "zobrist")

	ok := true
	level := [][bitboard.PackedFixedBytes]byte{positionRecord(&root)}
	var buf chess.MoveBuffer
	for ply := 1; ply <= maxDepth; ply++ {
		start := time.Now()
		childCount := 0
		for i := range level {
			b, _ := bitboard.DecodePackedFixed(level[i][:])
			childCount += b.GenMoves(&buf)
		}
		if childCount > maxChildren {
			fmt.Printf("    %4d %14s   (limit %d children)\n", ply, perftGroup(uint64(childCount)), maxChildren)
			break
		}
		children := make([][bitboard.PackedFixedBytes]byte, 0, childCount)
		for i := range level {
			b, _ := bitboard.DecodePackedFixed(level[i][:])
			n := b.GenMoves(&buf)
			for j := 0; j < n; j++ {
				child := b
				child.DoMove(buf[j])
				children = append(children, positionRecord(&child))
			}
		}
		slices.SortFunc(children, func(a, b [bitboard.PackedFixedBytes]byte) int { return bytes.Compare(a[:], b[:]) })
		level = slices.Compact(children)
		unique := len(level)

		keys := make([]bitboard.Key, unique)
		for i := range level {
			b, _ := bitboard.DecodePackedFixed(level[i][:])
			keys[i] = b.Key
		}
		slices.SortFunc(keys, func(a, b bitboard.Key) int {
			for i := range a {
				if a[i] != b[i] {
					if a[i] < b[i] {
						return -1
					}
					return 1
				}
			}
			return 0
		})
		collisions := unique - len(slices.Compact(keys))

		reference := "-"
		if isStart && ply-1 < len(uniqueReference) {
			reference = perftGroup(uint64(uniqueReference[ply-1]))
			if uniqueReference[ply-1] != unique {
				reference += " FAIL"
				ok = false
			}
		}
		fmt.Printf("    %4d %14s %12s %10s %8s %9s %10d\n",
			ply, perftGroup(uint64(childCount)), perftGroup(uint64(unique)), reference, fmtMs(time.Since(start)),
			fmtMB(unique*bitboard.PackedFixedBytes), collisions)
	}
	if ok {
		fmt.Println("    [all ok]")
	} else {
		fmt.Println("    [FAILURES]")
	}
	fmt.Println()
}

func perftGroup(n uint64) string {
	s := fmt.Sprintf("%d", n)
	out := make([]byte, 0, len(s)+len(s)/3)
	for i := range s {
		if i > 0 && (len(s)-i)%3 == 0 {
			out = append(out, ',')
		}
		out = append(out, s[i])
	}
	return string(out)
}

func fmtMs(d time.Duration) string {
	if d < time.Second {
		return fmt.Sprintf("%.0f ms", float64(d.Microseconds())/1000)
	}
	return fmt.Sprintf("%.2f s", d.Seconds())
}

func fmtMB(bytes int) string {
	if bytes < 1<<20 {
		return fmt.Sprintf("%.1f KB", float64(bytes)/1024)
	}
	return fmt.Sprintf("%.1f MB", float64(bytes)/(1<<20))
}
