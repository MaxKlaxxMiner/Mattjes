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
)

func main() {
	fmt.Println("Mattjes (Go) - hello world")
	fmt.Printf("%s on %s/%s, %d CPUs\n", runtime.Version(), runtime.GOOS, runtime.GOARCH, runtime.NumCPU())

	// --- tests (enable one at a time) ---
	// perft.RunClassic()
	// perft.RunListBased()
}
