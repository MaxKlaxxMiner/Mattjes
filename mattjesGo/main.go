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

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

func main() {
	fmt.Printf("Mattjes (Go) %s on %s/%s, %d CPUs\n\n", runtime.Version(), runtime.GOOS, runtime.GOARCH, runtime.NumCPU())

	// --- milestone 1: move generator + perft (enable one at a time) ---
	// mailboxPerftRecursive(200_000_000)
	// mailboxPerftIterative(200_000_000)
	// mailboxPerftBreadth(200_000_000, 2048)
	// mailboxPerftParallel(1_000_000_000, 0)
	// mailboxDivide(chess.StartFEN, 3)

	// bitboardPerftRecursive(200_000_000)
	// bitboardPerftIterative(200_000_000)
	// bitboardPerftBreadth(200_000_000, 2048)
	// bitboardPerftParallel(4_000_000_000, 0)
	// bitboardEncodeRoundtrip()
	// bitboardPerftBreadthFastFen(200_000_000, 2048)
	// bitboardPerftBreadthPacked(200_000_000, 2048)
	// bitboardPerftBreadthPackedFixed(200_000_000, 2048)

	// --- milestone 2: hash keys ---
	bitboardPerftRecursive(200_000_000)
	// bitboardHashVerify(5_000_000)
	// bitboardPerftKeyCost(20_000_000)
	// level := bitboardUniquePositions(chess.StartFEN, 6, 40_000_000)
	// bitboardKeyDistribution(level, 22)
	_ = chess.StartFEN // keep the import for the commented experiments above
	// bitboardUniquePositions("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1", 5, 40_000_000)
	// bitboardDivide(chess.StartFEN, 3)
}
