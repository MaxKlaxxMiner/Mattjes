// Mattjes - a chess engine specialized in mate and draw search.
// Copyright (C) 2026 Max Klaxx Miner
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

package main

import (
	"fmt"
	"os"
	"runtime"
	"runtime/debug"
	"runtime/pprof"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/pairs"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/syzygy"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/uci"
)

func main() {
	// The GC normally lets the heap grow to twice the live size before it runs.
	// With a 256 MB (or larger) transposition table that would be 256 MB of
	// reserve for nothing; 1 % keeps the headroom small. The hot paths do not
	// allocate, so the more frequent GC cycles cost nothing there.
	debug.SetGCPercent(1)

	// The bare binary is the UCI engine, as a chess GUI expects (package uci,
	// threads via the UCI option). Arguments select tests and actions:
	//   test                                              the experiment block in experiments()
	//   egtb-list <pieces>                                names of all materials with that many pieces
	//   egtb-measure <name>... [--verify] [--workers N]   generate or load the tables, log to the cache dir
	if len(os.Args) == 1 {
		// MATTJES_CPUPROFILE=<file> writes a CPU profile of the UCI session
		// (go tool pprof -top runMattjesGo.exe <file>), for measuring the
		// searches on the benchmark positions of test-positions.md.
		if path := os.Getenv("MATTJES_CPUPROFILE"); path != "" {
			if f, err := os.Create(path); err == nil {
				if pprof.StartCPUProfile(f) == nil {
					defer pprof.StopCPUProfile()
				}
			}
		}
		uci.Run(os.Stdin, os.Stdout, "Mattjes "+uci.Version+" (Go)", runtime.NumCPU())
		return
	}
	runCommand(os.Args[1:])
}

// testArg returns the i-th argument after "test" (or def), so that an
// experiment can take its parameters from the command line.
func testArg(i int, def string) string {
	if len(os.Args) > 2+i {
		return os.Args[2+i]
	}
	return def
}

// experiments is the console test bench: enable one line at a time, build,
// then start the binary with "test".
func experiments() {
	fmt.Printf("Mattjes (Go) %s on %s/%s, %d CPUs\n\n", runtime.Version(), runtime.GOOS, runtime.GOARCH, runtime.NumCPU())

	// --- milestone 1: move generator + perft (enable one at a time) ---
	// bitboardPerftRecursive(200_000_000)
	// bitboardPerftIterative(200_000_000)
	// bitboardPerftBreadth(200_000_000, 2048)
	// bitboardPerftBreadthPacked(200_000_000, 2048)
	// bitboardPerftBreadthPackedFixed(200_000_000, 2048)
	// bitboardPerftParallel(4_000_000_000, 0)
	// bitboardEncodeRoundtrip()
	// bitboardDivide(chess.StartFEN, 3)

	// --- milestone 2: hash keys ---
	// bitboardHashVerify(5_000_000)
	// bitboardUniquePositions(chess.StartFEN, 6, 40_000_000)

	// --- milestone 3: transposition tables ---
	// bitboardPerftTT(4_000_000_000, 256, false)
	// bitboardPerftTT(4_000_000_000, 256, true)
	// bitboardPerftTTPersist(chess.StartFEN, 7, 256)
	// bitboardPerftTTLoad(chess.StartFEN, 7) // also reads the file written by the Rust build
	// bitboardUniquePositionsHashed(chess.StartFEN, 6, 40_000_000)
	// bitboardStoreRoundtrip()

	// --- milestone 4, step 1: mate search generator extensions ---
	// bitboardPerftDetailed(200_000_000)
	// bitboardMateVerify(5_000_000)

	// --- milestone 4, step 2 + 3: mate search without / with TT ---
	// mateabSolve(7, 0, false)
	// mateabSolve(17, 256, false)
	// mateabSolve(17, 256, true)
	// mateabSolveNamed("KBN-K", 1024, true) // mate in 31: 1.4 G nodes, 5 to 6 minutes

	// --- milestone 4: list-based search, 10 to 50 s per four-piece position, about 3 GB (the pawn test exceeds 30 M positions at ply 9) ---
	// matelistSolve("KRR-K", 0, 200, 30_000_000)
	// matelistSolve("KBN-K", 0, 200, 30_000_000)

	// --- milestone 5: endgame tables ---
	// egtbIndexRoundtrip(7) // 16 s
	// egtbGenerate([]string{"KBNK", "KQKR", "KRKR", "KQKN"}, 12, false, true)
	// egtbGenerate([]string{"all"}, 12, false, true) // 17 s + 9 s verification
	// egtbLoadOrGenerate(12)
	// mateabSolveTables(31, 0, false) // four-piece positions are root hits, pawns 464,248 nodes (identical to Rust)
	// mateabSolveTablesNamed("KQ-KBN", 1024, true) // mate in 39 with five pieces: open how long, user runs it
	// egtbProbe("8/7k/1p6/1P6/7K/8/8/8 w - - 0 1", "8/7k/1p6/1P6/7K/8/8/8 b - - 0 1") // KP-KP study: win in 49 plies / draw, with the line from the tables
	// matelistSolve("KP-KP", 0, 200, 30_000_000) // independent check of the table value: mate in 25 in 4 s, 2.9 M positions

	// --- milestone 4, step 4: df-pn (sizeMB, codec "sat"/"float", options m mobility, l lazy mobility, e/E/x/X epsilon 1/8 1/2 1 2, f final entries, t tables, i iterative) ---
	// matepnSolve(15, 256, "sat", "mEf") // KQ-KN 368,362 visits in 1.2 s, KR-KR 1,206,343 in 2.9 s (the visits swing 4x with the table size, m4-mate-search.md step 5)
	// --- mobility experiment (docs/handout-matepn-mobility.md): the three leaf initialisations, visits must match Rust per variant ---
	// matepnSolve(15, 256, "sat", "mEf")  // A: mobility, 1,672,777 visits in total (the default, won the benchmark: 70.5 s)
	// matepnSolve(15, 256, "sat", "Ef")   // B: 1/1 leaves, 3,227,469 visits (benchmark 114.4 s)
	// matepnSolve(15, 256, "sat", "lEf")  // C: lazy, the move count when a node is entered, 8,694,695 visits (benchmark 83.8 s)
	// matepnSolveNamed("KBB-K", 1024, "sat", "mEf") // proven after 109.5 M visits, 336 s (Rust), proof tree incomplete (27 M replaced)
	// --- five pieces with the endgame tables as oracle: the first case where search and tables must cooperate, open how long ---
	// mateabSolveTablesNamed("KQ-KBN", 1024, true)
	// matepnSolveNamed("KQ-KBN", 1024, "sat", "mEft")

	// --- list-based search, the best version for finite spaces: KBB-K (11 s, 6.35 M positions), then KQ-KBN with a 16 GB store (400 M positions at 75 %) ---
	// matelistSolve("KBB-K", 0, 200, 30_000_000)
	// matelistSolve("KQ-KBN", 0, 200, 400_000_000) // 387 M positions at ply 16, aborted at ply 17 after 960 s, 42 GB peak; the space is 700-750 M

	// --- milestone 5, measurement series: five- and six-piece tables (time, memory, size, checksum), cache dir mattjes-egtb-cache/ ---
	// egtbLoadOrGenerate(12) // makes sure the four-piece base exists (17 s once, then 0.2 s); larger materials via "egtb-measure <name>"
	// --- interval oracle (docs/m5-endgame-tables.md, quantised cache files): KRR-KN with 5-KRRKN.bin (buckets of 8 moves) = the search resolves the 16-ply window; without the file 87,503 visits (Go = Rust) ---
	// --- block cache of the packed table (docs/m5-endgame-tables.md, "Blockformat"): hits, misses and the working set; egtb.CacheBytes = 32 << 20 before the call measures a small budget ---
	// matepnSolveFEN("KRR-KN after Nh2", "8/8/4k3/8/8/8/RK5n/2R5 w - - 2 2", 7, 256, "sat", "mEft") // 9,988 visits, 3,997 of 14,784 blocks touched (Go = Rust)
	_ = egtb.CacheBytes
	// --- replacement by value (docs/m4-mate-search.md, step 7): b = buckets with priority classes (open < disproven < proven < final), the direct table for comparison; visits must match Rust ---
	// --- proof goal "win" (docs/m4-mate-search.md, step 8): w = a won table position proves the node whatever its distance, T = four-piece base only; without tables the goals coincide (same 1,370,235 visits) ---
	// matepnSolve(15, 256, "sat", "mEfbw")
	// matepnSolveNamed("KQ-KBN", 1024, "sat", "mEfbTwi") // five pieces with the four-piece tables as oracle: no forced capture into a won table within 21 plies (5.6 M visits, 32 s, Go = Rust), the cost grows 1.8x per two plies; the author runs it further
	// matepnSolveFEN("KRR-KN after Nh2", "8/8/4k3/8/8/8/RK5n/2R5 w - - 2 2", 7, 256, "sat", "mEfbTwi") // the knight must fall: win proven at depth 11 after 294,072 visits, mate in at most 37 plies (one defence ends in a won KR-KN), Go = Rust
	// matepnSolveFEN("KRR-KN after Nh2", "8/8/4k3/8/8/8/RK5n/2R5 w - - 2 2", 7, 256, "sat", "mEfbTi")  // the mate goal for comparison: mate in 13 plies at depth 13 after 1,243,719 visits

	// --- milestone 7: WDL search (docs/m7-wdl-search-design.md), names "" = all chess.WdlPositions, options m mobility, e/E/x/X epsilon, b buckets, T four-piece base as oracle ---
	// "test <names> <options> <codec>" overrides the arguments from the command line, so variants run without a rebuild
	// 2026-10-21: epsilon 1 (x) is the setting for disproofs, 1/2 (E) ran forever on KP-K rook pawn; sat beats float (docs/m7-wdl-search.md)
	// wdlpnSolve(testArg(0, ""), 256, testArg(2, "sat"), testArg(1, "mxb"))
	// --- step 2: explicit graph (docs/m7-wdl-search.md), "test <names> <options>": B brute force (reachable graph), e/E/x/X epsilon, v progress lines ---
	// options t = four-piece tables as oracle, s = Syzygy WDL on top (syzygyDefaultDir)
	// option n = Patricia's net in the certificate builder's policy, i = the descent's numbers from the net (docs/nnue-patricia.md)
	// wdllistSolve(testArg(0, ""), testArg(1, "x"))
	// --- package pairs (Syzygy's scheme, own container) on the WDL streams in tests/wdl: "test <names> <blockBytes> <indexBits> <pairsPerPass> <minCount> <maxSymbols> <maxSymbolLen>" ---
	// syzygyDescribe(syzygyDefaultDir, "KBNvK;KQvKR;KRvKN;KPvKP;KBBvK;KBvKP") // the layout of the Syzygy streams (symbols, blocks, piece order)
	// --- own WDL tables in Syzygy's structure (syzygy/write.go): every candidate piece order per stream, best kept; "test <names> <blockBytes> <indexBits> <pairsPerPass> <minCount> <maxSymbols> <maxSymbolLen> [v]" ---
	// 2026-10-22: KQK;KBNK;KBBK;KRKN;KPK = 170,282 bytes against 173,776 Syzygy (0.98) in 28 s; "" = the whole base (minutes)
	// "test <names> <sampleTarget> <finalists> [v]": the candidate preselection on sampled windows (sampleTarget 0 = 2 Mi: every base stream is built completely; 65536 judges the 4-piece streams on 3 % and builds the finalists)
	syzygyBuild(testArg(0, ""), 12, pairs.Options{Workers: 12, DictSample: testInt(4, 0)}, syzygy.BuildOptions{SampleTarget: testInt(1, 0), Finalists: testInt(2, 0)}, testArg(3, "") == "v") // KQKBN 2026-10-22: 1,188,612 bytes (1.13) in 240 s after 63 s generation
	// the own WDL files of the base: written to mattjes-egtb-cache/wdl/ ("" = all 35, minutes), then probed through the Syzygy prober against the base
	// 2026-10-22: 14 materials written and verified (100,000 random positions each, all agree); "test" without names writes and verifies all 35 (minutes)
	// syzygyOwnWrite(testArg(0, ""), 12, pairs.Options{Workers: 12}, syzygy.BuildOptions{}, testArg(1, "")) // "test <names> <dir>": dir "" = mattjes-egtb-cache/wdl; 2026-10-22: all 35 in 264 s, 1,322,480 bytes
	// syzygyOwnVerify(testArg(0, ""), testInt(1, 20000))
	// syzygyDecodeCheck(testArg(0, ""), testInt(1, 8), testInt(2, 20000)) // the index decoder (syzygy/decode.go) against encode: 2026-10-22 every index of all 35 materials round-trips
	// syzygyOwnProbe(testArg(0, "K7/1r6/8/6k1/8/8/6R1/8 b - - 0 1;k7/1R6/8/6K1/8/8/6r1/8 w - - 0 1")) // single positions: base value next to the own file's answer
	// "test <names> <blockBytes> <indexBits> <pairsPerPass> <minCount> <maxSymbols> <maxSymbolLen> <dictSample>": every position verified, random probes timed; dictSample > 0 = the dictionary from windows, the stream tokenised in one pass
	// pairsMeasure("tests/wdl", testArg(0, ""), pairs.Options{BlockBytes: testInt(1, 0), IndexBits: testInt(2, 0), PairsPerPass: testInt(3, 64), MinCount: testInt(4, 8), MaxSymbols: testInt(5, 4095), MaxSymbolLen: testInt(6, 256), DictSample: testInt(7, 0), Workers: 12}, true)
	// pairsSyzygy(syzygyDefaultDir, testArg(0, "KBNvK;KQvKR;KRvKN;KPvKP;KBBvK;KBvKP;KQvKQ;KRvKB"), pairs.Options{BlockBytes: testInt(1, 0), IndexBits: testInt(2, 0), PairsPerPass: testInt(3, 64), MinCount: testInt(4, 8), MaxSymbols: testInt(5, 4095), MaxSymbolLen: testInt(6, 256), Workers: 12})
	// --- own WDL tables (docs/m5-endgame-tables.md, "Eigene WDL-Tabellen"): WDL derived from the base, captures as don't care, packed with pairs per digit order (n/r/k/K) against the Syzygy files; the first order also with lz and dumped to tests/wdl for zstd/xz ---
	// 2026-10-22, order n: lz 3.05 MB (2-bit), zstd 1.36 MB, xz 1.09 MB against 1.26 MB Syzygy for all 35 materials
	// egtbWdlSize(12, testArg(0, "n;r;k;K"), "tests/wdl", pairs.Options{BlockBytes: testInt(1, 32), IndexBits: testInt(2, 0), PairsPerPass: testInt(3, 64), MinCount: testInt(4, 8), MaxSymbols: testInt(5, 4096), MaxSymbolLen: testInt(6, 256), Workers: 12}, testArg(7, "") == "v")
	// --- rollouts (package rollout, docs/nnue-patricia.md): "test <names> <options> <maxPlies>", options t/s oracle, d depth 2, v lines; every root move played out ---
	// rolloutPositions(testArg(0, ""), testArg(1, "s"), testInt(2, 200)) // 2026-10-22: 561 of 567 root moves with Syzygy, 478 (depth 2: 516) with the four-piece tables, 284 without
	// rolloutLine(testArg(0, "8/8/4k3/8/8/8/RK6/2R2n2 b - - 1 1"), testArg(1, "s"), testInt(2, 200))
	// syzygyProbe(syzygyDefaultDir, testArg(0, "3k4/p7/8/8/8/8/P4rr1/K6Q w - - 0 1")) // single probes, semicolon separated (seven pieces: no table, Unknown)
	// --- Patricia's network (package nnue): "test <fenfile>" prints the reference lines of tests/patricia-eval.sh, then the incremental check and the throughput ---
	// nnueVerify(testArg(0, "tests/nnue-smoke.txt")) // 2026-10-22: 55 positions identical to the reference build (docs/nnue-patricia.md)
	// nnueIncremental(3)
	// nnueBench(3)
	// --- Syzygy WDL reader: "test <dir> <materials>" compares probes with the own tables on random legal positions ---
	// syzygyVerify(testArg(0, syzygyDefaultDir), testArg(1, "KQKR;KRKN;KPK;KPKP;KRRKN;KBNKQ;KQKRR;KQKRP;KPPKP;KRPKR"), 20000) // 2026-10-22: 25 materials x 20,000 positions agree (docs/m7-wdl-search.md)
	// syzygyPositions(testArg(0, syzygyDefaultDir)) // the root moves of the WDL test positions against the recorded win/draw/loss counts: all 31 agree
	// matepnSolve(15, 256, "sat", "mEfb") // 1,370,235 visits in total (KQ-KN 271,120, KR-KR 1,023,102), 0 proofs evicted, every proof tree complete; identical from 256 MB up (KR-KR 1,022,904 from 1 GB)
	// matepnSolve(15, 256, "sat", "mEf") // the direct table: 1,156,948 visits, 772 proofs and 3,285 final entries of KR-KR evicted, its proof tree incomplete; swings 5x with the size
	// for _, mb := range []int{64, 128, 256, 512, 1024, 2048} { matepnSolveNamed("KQ-KN", mb, "sat", "mEfb"); matepnSolveNamed("KR-KR", mb, "sat", "mEfb") } // the size series of step 7
	// matepnSolveNamed("pawns", 64, "sat", "mEfiM") // M = even depths too (the side to move is mated), the UCI setting: 7,933 nodes instead of 7,800
	// egtbMeasure([]string{"KQKBN"}, 12, true) // generated in 47 s, 845 MB process; loads the file, checks KQ-KBN = win in 77 and verifies every position forward
	// matepnSolve(12, 256, "sat", "mEfi") // iterative deepening: the shortest mate, cost of all depths (KQ-KN 7.9 M visits)
	// mateabSolve(17, 256, false) // the same positions with the depth-first search, for comparison
	_ = chess.StartFEN // keep the import for the commented experiments above
}
