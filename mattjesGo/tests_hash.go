package main

import (
	"bytes"
	"fmt"
	"math"
	"slices"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/perft"
)

// --- milestone 2: hash keys ---

// bitboardHashVerify walks all reference positions and compares the incremental
// Zobrist key with a full recomputation at every node.
func bitboardHashVerify(maxNodes uint64) {
	fmt.Printf("zobrist key words: %d (%d bit)\n", bitboard.KeyWords, bitboard.KeyWords*64)
	var mismatches uint64
	var walk func(b *bitboard.Board, depth int) uint64
	walk = func(b *bitboard.Board, depth int) uint64 {
		if b.Key != b.ZobristFull() {
			mismatches++
		}
		var buf chess.MoveBuffer
		n := b.GenMoves(&buf)
		if depth <= 0 {
			return 1
		}
		var total uint64
		s := b.State()
		for i := 0; i < n; i++ {
			b.DoMove(buf[i])
			total += walk(b, depth-1)
			b.UndoMove(buf[i], s)
			if b.Key != b.ZobristFull() {
				mismatches++
			}
		}
		return total
	}
	perft.Run("bitboard / zobrist incremental == full recompute at every node", func(fen string, depth int) (uint64, error) {
		b, err := bitboard.FromFEN(fen)
		if err != nil {
			return 0, err
		}
		before := mismatches
		nodes := walk(&b, depth-1)
		if mismatches != before {
			return 0, fmt.Errorf("%d key mismatches", mismatches-before)
		}
		// walk counts leaf nodes = perft(depth-1) children... report perft(depth) via moves at leaves
		_ = nodes
		return bitboard.PerftRecursive(&b, depth), nil
	}, maxNodes)
	fmt.Printf("total key mismatches: %d\n\n", mismatches)
}

// keySink defeats dead-code elimination in the recompute benchmarks.
var keySink uint64

// bitboardPerftKeyCost measures what computing a key from scratch at every node
// costs, compared with the plain perft (which already maintains the incremental key).
func bitboardPerftKeyCost(maxNodes uint64) {
	type variant struct {
		name string
		key  func(b *bitboard.Board) uint64
	}
	variants := []variant{
		{"crc64 recompute (yacboard style)", (*bitboard.Board).CRC64},
		{"zobrist full recompute", func(b *bitboard.Board) uint64 { return b.ZobristFull().Lo() }},
		{"exact 32-byte key + fold", func(b *bitboard.Board) uint64 { k := b.ExactKey(); return k.Hash() }},
		{"incremental key read (baseline)", func(b *bitboard.Board) uint64 { return b.Key.Lo() }},
	}
	for _, v := range variants {
		var walk func(b *bitboard.Board, depth int) uint64
		walk = func(b *bitboard.Board, depth int) uint64 {
			keySink ^= v.key(b)
			var buf chess.MoveBuffer
			n := b.GenMoves(&buf)
			if depth <= 1 {
				return uint64(n)
			}
			var total uint64
			s := b.State()
			for i := 0; i < n; i++ {
				b.DoMove(buf[i])
				total += walk(b, depth-1)
				b.UndoMove(buf[i], s)
			}
			return total
		}
		perft.Run("bitboard / perft + "+v.name+" per node", func(fen string, depth int) (uint64, error) {
			b, err := bitboard.FromFEN(fen)
			if err != nil {
				return 0, err
			}
			return walk(&b, depth), nil
		}, maxNodes)
	}
	fmt.Printf("(sink %x)\n\n", keySink)
}

// bitboardUniquePositions runs a breadth-first search that keeps only distinct
// positions per ply (exact 32-byte key), then counts how many distinct keys the
// shorter key types produce for the same set. The difference is the number of
// collisions. Returns the last ply's unique positions for further experiments.
func bitboardUniquePositions(fen string, maxDepth int, maxChildren int) []bitboard.ExactKey {
	root, err := bitboard.FromFEN(fen)
	if err != nil {
		panic(err)
	}
	fmt.Printf("=== unique positions per ply ===\n    %s\n", fen)
	fmt.Printf("    %4s %14s %12s %8s %9s | collisions: %8s %8s %8s %8s %8s %8s\n",
		"ply", "children", "unique", "time", "memory", "zob64", "zob128", "crc64", "zob32", "crc32lo", "crc32hi")

	level := []bitboard.ExactKey{root.ExactKey()}
	var buf chess.MoveBuffer
	for ply := 1; ply <= maxDepth; ply++ {
		start := time.Now()
		// count children first to allocate exactly once
		childCount := 0
		for i := range level {
			b := bitboard.BoardFromExactKey(&level[i])
			childCount += b.GenMoves(&buf)
		}
		if childCount > maxChildren {
			fmt.Printf("    %4d %14s   (limit %d children)\n", ply, perftGroup(uint64(childCount)), maxChildren)
			break
		}
		children := make([]bitboard.ExactKey, 0, childCount)
		for i := range level {
			b := bitboard.BoardFromExactKey(&level[i])
			n := b.GenMoves(&buf)
			for j := 0; j < n; j++ {
				child := b
				child.DoMove(buf[j])
				children = append(children, child.ExactKey())
			}
		}
		slices.SortFunc(children, func(a, b bitboard.ExactKey) int { return bytes.Compare(a[:], b[:]) })
		level = slices.Compact(children)
		unique := len(level)

		// collisions of the shorter keys over the unique set
		z64 := make([]uint64, unique)
		z128 := make([]bitboard.Key, unique)
		crc := make([]uint64, unique)
		for i := range level {
			b := bitboard.BoardFromExactKey(&level[i])
			z128[i] = b.Key
			z64[i] = b.Key.Lo()
			crc[i] = b.CRC64()
		}
		col := func(keys []uint64) int { slices.Sort(keys); return unique - len(slices.Compact(keys)) }
		col128 := func(keys []bitboard.Key) int {
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
			return unique - len(slices.Compact(keys))
		}
		trunc := func(keys []uint64, f func(uint64) uint64) []uint64 {
			out := make([]uint64, len(keys))
			for i, k := range keys {
				out[i] = f(k)
			}
			return out
		}
		cZob32 := col(trunc(z64, func(k uint64) uint64 { return k & 0xffffffff }))
		cCrc32lo := col(trunc(crc, func(k uint64) uint64 { return k & 0xffffffff }))
		cCrc32hi := col(trunc(crc, func(k uint64) uint64 { return k >> 32 }))
		cZob128 := col128(z128)
		cZob64 := col(z64)
		cCrc64 := col(crc)

		fmt.Printf("    %4d %14s %12s %8s %9s | %19d %8d %8d %8d %8d %8d\n",
			ply, perftGroup(uint64(childCount)), perftGroup(uint64(unique)), fmtMs(time.Since(start)),
			fmtMB(unique*bitboard.PackedFixedBytes), cZob64, cZob128, cCrc64, cZob32, cCrc32lo, cCrc32hi)
		n := float64(unique)
		fmt.Printf("         expected collisions for %s unique keys: 64 bit %.2e, 32 bit %.1f\n",
			perftGroup(uint64(unique)), n*(n-1)/2/math.Exp2(64), n*(n-1)/2/math.Exp2(32))
	}
	fmt.Println()
	return level
}

// bitboardKeyDistribution checks how evenly the low bits of each key type spread
// the given positions over a table of 2^bits slots, compared with a uniform random hash.
func bitboardKeyDistribution(level []bitboard.ExactKey, bits uint) {
	slots := 1 << bits
	mask := uint64(slots - 1)
	n := len(level)
	lambda := float64(n) / float64(slots)
	fmt.Printf("=== index distribution: %s positions into 2^%d slots (load %.2f) ===\n", perftGroup(uint64(n)), bits, lambda)
	fmt.Printf("    %-28s %10s %10s %8s %8s\n", "key -> index", "empty", "expected", "max", "z-score")

	type variant struct {
		name string
		idx  func(b *bitboard.Board) uint64
	}
	variants := []variant{
		{"zobrist64 low bits", func(b *bitboard.Board) uint64 { return b.Key.Lo() & mask }},
		{"zobrist64 high bits", func(b *bitboard.Board) uint64 { return b.Key.Lo() >> (64 - bits) }},
		{"crc64 low bits", func(b *bitboard.Board) uint64 { return b.CRC64() & mask }},
		{"crc64 high bits", func(b *bitboard.Board) uint64 { return b.CRC64() >> (64 - bits) }},
		{"mix64(crc64) low bits", func(b *bitboard.Board) uint64 { return bitboard.Mix64(b.CRC64()) & mask }},
		{"occupancy low bits (naive)", func(b *bitboard.Board) uint64 { return (b.ByColor[0] | b.ByColor[1]) & mask }},
		{"exact key fold low bits", func(b *bitboard.Board) uint64 { k := b.ExactKey(); return k.Hash() & mask }},
	}
	counts := make([]uint32, slots)
	for _, v := range variants {
		clear(counts)
		for i := range level {
			b := bitboard.BoardFromExactKey(&level[i])
			counts[v.idx(&b)]++
		}
		empty, maxLoad := 0, uint32(0)
		chi2 := 0.0
		for _, c := range counts {
			if c == 0 {
				empty++
			}
			if c > maxLoad {
				maxLoad = c
			}
			d := float64(c) - lambda
			chi2 += d * d / lambda
		}
		df := float64(slots - 1)
		z := (chi2 - df) / math.Sqrt(2*df)
		fmt.Printf("    %-28s %10d %10.0f %8d %8.1f\n", v.name, empty, float64(slots)*math.Exp(-lambda), maxLoad, z)
	}
	fmt.Println("    (z-score near 0 = indistinguishable from uniform random; large = clustered)")
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
