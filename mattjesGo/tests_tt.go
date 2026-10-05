package main

import (
	"fmt"
	"math/bits"
	"os"
	"path/filepath"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/perft"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/tt"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/ttstore"
)

// --- milestone 3: transposition tables (measurements in docs/m3-transposition-table.md) ---

// transTable is what the experiments need from a table; both tt layouts implement it.
type transTable interface {
	Probe(k tt.Key) (uint64, bool)
	Store(k tt.Key, value uint64)
	MaxValue() uint64
	Clear()
	ResetStats()
	Slots() int
	Used() int
	Counters() *tt.Stats
}

// perftDepthSalt is XORed into both key words, so (position, remaining depth) is
// simply another 128-bit key: same position, different depth, different entry.
var perftDepthSalt = func() (s [32]tt.Key) {
	x := uint64(0x9E3779B97F4A7C15)
	for i := range s {
		for w := range s[i] {
			x ^= x << 13
			x ^= x >> 7
			x ^= x << 17
			s[i][w] = x
		}
	}
	return
}()

// perftTT is PerftRecursive with a transposition table: a subtree whose
// (position, depth) was already counted is taken from the table. Leaves are still
// bulk-counted at depth 1, so entries start at depth 2. The value is the node
// count; subtrees larger than the value width (24 bits at 16 Mi slots) are not
// stored, which costs almost nothing because there are so few of them.
func perftTT(b *bitboard.Board, depth int, t transTable) uint64 {
	var buf chess.MoveBuffer
	n := b.GenMoves(&buf)
	if depth <= 1 {
		return uint64(n)
	}
	k := tt.Key{b.Key[0] ^ perftDepthSalt[depth][0], b.Key[1] ^ perftDepthSalt[depth][1]}
	if count, ok := t.Probe(k); ok {
		return count
	}
	var total uint64
	s := b.State()
	for i := 0; i < n; i++ {
		b.DoMove(buf[i])
		total += perftTT(b, depth-1, t)
		b.UndoMove(buf[i], s)
	}
	if total <= t.MaxValue() {
		t.Store(k, total)
	}
	return total
}

// bitboardPerftTT runs the reference perft suite with a transposition table of the
// given size and layout. The table is cleared before every (position, depth)
// outside the measured time, so the speedup comes only from transpositions inside
// one run. Prints hit rate, replacements and the near-miss statistics for the
// key-size question.
func bitboardPerftTT(maxNodes uint64, sizeMB int, bucketed bool) {
	var t transTable
	name := "direct-mapped"
	if bucketed {
		t = tt.NewBuckets(sizeMB)
		name = "4-way bucket"
	} else {
		t = tt.New(sizeMB)
	}
	perft.RunPrepared(fmt.Sprintf("bitboard / perft with %s TT, %d MB = %s entries, %d value bits", name, sizeMB, perftGroup(uint64(t.Slots())), bits.Len64(t.MaxValue())), t.Clear, func(fen string, depth int) (uint64, error) {
		b, err := bitboard.FromFEN(fen)
		if err != nil {
			return 0, err
		}
		return perftTT(&b, depth, t), nil
	}, maxNodes)
	printTTStats(t.Counters(), t.Used(), t.Slots())
}

func printTTStats(s *tt.Stats, used, slots int) {
	hitRate := 0.0
	if s.Probes > 0 {
		hitRate = 100 * float64(s.Hits) / float64(s.Probes)
	}
	fmt.Printf("    probes %s, hits %s (%.1f%%), stores %s, replaced %s, fill after last run %.1f%%\n",
		perftGroup(s.Probes), perftGroup(s.Hits), hitRate, perftGroup(s.Stores), perftGroup(s.Replaced), 100*float64(used)/float64(slots))
	fmt.Printf("    foreign entries seen %s, false hits a table with only 16 / 32 / 48 check bits would have had: %s / %s / %s\n\n",
		perftGroup(s.Foreign), perftGroup(s.NearMiss[0]), perftGroup(s.NearMiss[1]), perftGroup(s.NearMiss[2]))
}

// ttFilePath is shared by Go and Rust so a table saved by one can be loaded by the other.
func ttFilePath() string { return filepath.Join(os.TempDir(), "mattjes-perft.tt") }

// bitboardPerftTTPersist measures the persistence cycle: cold perft run, save,
// load into a fresh table, warm run (which should only touch the root), and the
// rejection of a file with a wrong fingerprint. The file is kept for
// bitboardPerftTTLoad (and for the other language).
func bitboardPerftTTPersist(fen string, depth int, sizeMB int) {
	path := ttFilePath()
	fp := bitboard.ZobristFingerprint()
	fmt.Printf("=== perft TT persistence ===\n    %s\n    depth %d, %d MB direct-mapped table, zobrist fingerprint %016x\n", fen, depth, sizeMB, fp)
	b, err := bitboard.FromFEN(fen)
	if err != nil {
		panic(err)
	}

	t := tt.New(sizeMB)
	start := time.Now()
	cold := perftTT(&b, depth, t)
	fmt.Printf("    cold run:  %s nodes in %s, %s stores, fill %.1f%%\n", perftGroup(cold), fmtMs(time.Since(start)), perftGroup(t.Stores), 100*float64(t.Used())/float64(t.Slots()))

	start = time.Now()
	if err := t.Save(path, fp); err != nil {
		panic(err)
	}
	info, _ := os.Stat(path)
	fmt.Printf("    save:      %s in %s -> %s\n", fmtMB(int(info.Size())), fmtMs(time.Since(start)), path)

	start = time.Now()
	t2, err := tt.Load(path, fp)
	if err != nil {
		panic(err)
	}
	fmt.Printf("    load:      %s in %s\n", fmtMB(t2.Bytes()), fmtMs(time.Since(start)))

	start = time.Now()
	warm := perftTT(&b, depth, t2)
	fmt.Printf("    warm run:  %s nodes in %s, %s probes, %s hits\n", perftGroup(warm), fmtMs(time.Since(start)), perftGroup(t2.Probes), perftGroup(t2.Hits))

	if _, err := tt.Load(path, fp^1); err != nil {
		fmt.Printf("    wrong fingerprint rejected: %v\n", err)
	} else {
		fmt.Println("    FAIL: wrong fingerprint accepted")
	}
	if cold == warm {
		fmt.Println("    [all ok]")
	} else {
		fmt.Println("    [FAILURES]")
	}
	fmt.Println()
}

// bitboardPerftTTLoad loads the file left by bitboardPerftTTPersist (from either
// language) and runs the warm perft; the count must match the reference suite.
func bitboardPerftTTLoad(fen string, depth int) {
	path := ttFilePath()
	fmt.Printf("=== perft TT load ===\n    %s depth %d from %s\n", fen, depth, path)
	start := time.Now()
	t, err := tt.Load(path, bitboard.ZobristFingerprint())
	if err != nil {
		fmt.Printf("    load failed: %v\n\n", err)
		return
	}
	fmt.Printf("    load:      %s in %s\n", fmtMB(t.Bytes()), fmtMs(time.Since(start)))
	b, err := bitboard.FromFEN(fen)
	if err != nil {
		panic(err)
	}
	start = time.Now()
	warm := perftTT(&b, depth, t)
	fmt.Printf("    warm run:  %s nodes in %s, %s probes, %s hits\n", perftGroup(warm), fmtMs(time.Since(start)), perftGroup(t.Probes), perftGroup(t.Hits))
	expected := uint64(0)
	for _, p := range chess.PerftPositions {
		if p.FEN == fen && depth <= len(p.Nodes) {
			expected = p.Nodes[depth-1]
		}
	}
	switch {
	case expected == 0:
		fmt.Println("    (no reference value)")
	case expected == warm:
		fmt.Println("    [all ok]")
	default:
		fmt.Printf("    FAIL expected %s\n", perftGroup(expected))
	}
	fmt.Println()
}

// bitboardUniquePositionsHashed is bitboardUniquePositions with a ttstore set
// instead of sort+compact: a child is kept only if its 128-bit key is new. Only
// the distinct positions are ever stored, not all children. The counts must
// still match OEIS A083276 (a 128-bit collision would lose a position). The set
// has a fixed size per ply, estimated from the child count; if it runs full the
// ply is reported and the run stops.
func bitboardUniquePositionsHashed(fen string, maxDepth int, maxChildren int) {
	root, err := bitboard.FromFEN(fen)
	if err != nil {
		panic(err)
	}
	isStart := fen == chess.StartFEN
	fmt.Printf("=== unique positions per ply, ttstore set ===\n    %s\n", fen)
	fmt.Printf("    %4s %14s %12s %10s %8s %9s %9s\n", "ply", "children", "unique", "reference", "time", "records", "set")

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
		// The store has a fixed size, so the ply is sized up front: distinct positions
		// are about half of the children from ply 3 on. If the estimate is too small
		// (plies 1 and 2 have no transpositions at all) the ply is redone with twice
		// the slots.
		slots := 16
		for slots*ttstore.MaxLoadPercent/100 < childCount/2 {
			slots *= 2
		}
		var set *ttstore.Store
		var next [][bitboard.PackedFixedBytes]byte
		for retry := true; retry; slots *= 2 {
			set = ttstore.NewSlots(slots)
			next = make([][bitboard.PackedFixedBytes]byte, 0, childCount/2)
			retry = false
		expand:
			for i := range level {
				b, _ := bitboard.DecodePackedFixed(level[i][:])
				n := b.GenMoves(&buf)
				for j := 0; j < n; j++ {
					child := b
					child.DoMove(buf[j])
					isNew, ok := set.Insert(child.Key)
					if !ok {
						retry = true
						break expand
					}
					if isNew {
						next = append(next, positionRecord(&child))
					}
				}
			}
			if retry {
				fmt.Printf("    %4d %14s   (set with %s slots full, retrying with twice the size)\n", ply, perftGroup(uint64(childCount)), perftGroup(uint64(slots)))
			}
		}
		level = next
		unique := len(level)

		reference := "-"
		if isStart && ply-1 < len(uniqueReference) {
			reference = perftGroup(uint64(uniqueReference[ply-1]))
			if uniqueReference[ply-1] != unique {
				reference += " FAIL"
				ok = false
			}
		}
		fmt.Printf("    %4d %14s %12s %10s %8s %9s %9s\n",
			ply, perftGroup(uint64(childCount)), perftGroup(uint64(unique)), reference, fmtMs(time.Since(start)),
			fmtMB(unique*bitboard.PackedFixedBytes), fmtMB(set.Bytes()))
	}
	if ok {
		fmt.Println("    [all ok]")
	} else {
		fmt.Println("    [FAILURES]")
	}
	fmt.Println()
}

// bitboardStoreRoundtrip checks ttstore against a plain map and its Save/Load.
func bitboardStoreRoundtrip() {
	fmt.Println("=== ttstore roundtrip ===")
	path := filepath.Join(os.TempDir(), "mattjes-store.tts")
	fp := bitboard.ZobristFingerprint()
	root, _ := bitboard.FromFEN(chess.StartFEN)

	s := ttstore.New(4) // 262,144 slots, accepts 196,608 keys
	ref := map[bitboard.Key]uint64{}
	fails := 0
	var walk func(b *bitboard.Board, depth int)
	walk = func(b *bitboard.Board, depth int) {
		if _, ok := s.Put(b.Key, uint64(depth)); !ok {
			fails++
		}
		ref[b.Key] = uint64(depth)
		if depth == 0 {
			return
		}
		var buf chess.MoveBuffer
		n := b.GenMoves(&buf)
		st := b.State()
		for i := 0; i < n; i++ {
			b.DoMove(buf[i])
			walk(b, depth-1)
			b.UndoMove(buf[i], st)
		}
	}
	walk(&root, 4)
	if s.Len() != len(ref) {
		fails++
	}
	for k, v := range ref {
		if got, ok := s.Get(k); !ok || got != v {
			fails++
		}
	}
	if err := s.Save(path, fp); err != nil {
		panic(err)
	}
	s2, err := ttstore.Load(path, fp)
	if err != nil {
		panic(err)
	}
	for k, v := range ref {
		if got, ok := s2.Get(k); !ok || got != v {
			fails++
		}
	}
	os.Remove(path)
	fmt.Printf("    %s keys, %s slots, %s, %d value bits, %d failures\n", perftGroup(uint64(s.Len())), perftGroup(uint64(s.Slots())), fmtMB(s.Bytes()), s.ValueBits(), fails)
	if fails == 0 {
		fmt.Println("    [all ok]")
	} else {
		fmt.Println("    [FAILURES]")
	}
	fmt.Println()
}
