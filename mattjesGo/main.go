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
	"runtime"
	"runtime/debug"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

func main() {
	// The GC normally lets the heap grow to twice the live size before it runs.
	// With a 256 MB (or larger) transposition table that would be 256 MB of
	// reserve for nothing; 1 % keeps the headroom small. The hot paths do not
	// allocate, so the more frequent GC cycles cost nothing there.
	debug.SetGCPercent(1)

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

	// --- milestone 4, step 4: df-pn (sizeMB, codec "sat"/"float", options m mobility, e/E/x/X epsilon 1/8 1/2 1 2, f final entries, t tables, i iterative) ---
	// matepnSolve(15, 256, "sat", "mEf") // KQ-KN 181,065 visits in 0.8 s, KR-KR 1,125,947 in 3.6 s
	// matepnSolveNamed("KBB-K", 1024, "sat", "mEf") // proven after 109.5 M visits, 336 s (Rust), proof tree incomplete (27 M replaced)
	// --- five pieces with the endgame tables as oracle: the first case where search and tables must cooperate, open how long ---
	// mateabSolveTablesNamed("KQ-KBN", 1024, true)
	// matepnSolveNamed("KQ-KBN", 1024, "sat", "mEft")

	// --- list-based search, the best version for finite spaces: KBB-K (11 s, 6.35 M positions), then KQ-KBN with a 16 GB store (400 M positions at 75 %) ---
	// matelistSolve("KBB-K", 0, 200, 30_000_000)
	// matelistSolve("KQ-KBN", 0, 200, 400_000_000) // 387 M positions at ply 16, aborted at ply 17 after 960 s, 42 GB peak; the space is 700-750 M

	// --- milestone 5, measurement series: five- and six-piece tables (time, memory, size, checksum), cache dir mattjes-egtb-cache/ ---
	egtbMeasure([]string{"KQKBN"}, 12, true) // generated in 47 s, 845 MB process; now loads the file, checks KQ-KBN = win in 77 and verifies every position forward
	// matepnSolve(12, 256, "sat", "mEfi") // iterative deepening: the shortest mate, cost of all depths (KQ-KN 7.9 M visits)
	// mateabSolve(17, 256, false) // the same positions with the depth-first search, for comparison
	_ = chess.StartFEN // keep the import for the commented experiments above
}
