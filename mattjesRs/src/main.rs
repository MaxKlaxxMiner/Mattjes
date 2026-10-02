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

// Experiments are switched on and off in main, so most of the API is unused at any time.
#![allow(dead_code)]

mod bitboard;
mod chess;
mod perft;
mod tests_hash;
mod tests_perft;

fn main() {
    println!(
        "Mattjes (Rust) on {}/{}, {} CPUs\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    );

    // --- milestone 1: move generator + perft (enable one at a time) ---
    tests_perft::bitboard_perft_recursive(200_000_000);
    // tests_perft::bitboard_perft_iterative(200_000_000);
    // tests_perft::bitboard_perft_breadth(200_000_000, 2048);
    // tests_perft::bitboard_perft_breadth_encoded::<bitboard::Packed>(200_000_000, 2048);
    // tests_perft::bitboard_perft_breadth_encoded::<bitboard::PackedFixed>(200_000_000, 2048);
    // tests_perft::bitboard_perft_parallel(4_000_000_000, 0);
    // tests_perft::bitboard_encode_roundtrip();
    // tests_perft::bitboard_divide(chess::START_FEN, 3);

    // --- milestone 2: hash keys ---
    // tests_hash::bitboard_hash_verify(5_000_000);
    // tests_hash::bitboard_unique_positions(chess::START_FEN, 6, 40_000_000);
}
