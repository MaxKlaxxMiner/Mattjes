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
mod egtb;
mod mateab;
mod matelist;
mod matepn;
mod perft;
mod tests_egtb;
mod tests_hash;
mod tests_mate;
mod tests_perft;
mod tests_pn;
mod tests_tt;
mod tt;
mod ttstore;

fn main() {
    println!(
        "Mattjes (Rust) on {}/{}, {} CPUs\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    );

    // --- milestone 1: move generator + perft (enable one at a time) ---
    // tests_perft::bitboard_perft_recursive(200_000_000);
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

    // --- milestone 3: transposition tables ---
    // tests_tt::bitboard_perft_tt(4_000_000_000, 256, false);
    // tests_tt::bitboard_perft_tt(4_000_000_000, 256, true);
    // tests_tt::bitboard_perft_tt_persist(chess::START_FEN, 7, 256);
    // tests_tt::bitboard_perft_tt_load(chess::START_FEN, 7); // also reads the file written by the Go build
    // tests_tt::bitboard_unique_positions_hashed(chess::START_FEN, 6, 40_000_000);
    // tests_tt::bitboard_store_roundtrip();

    // --- milestone 4, step 1: mate search generator extensions ---
    // tests_mate::bitboard_perft_detailed(200_000_000);
    // tests_mate::bitboard_mate_verify(5_000_000);

    // --- milestone 4, step 2 + 3: mate search without / with TT ---
    // tests_mate::mateab_solve(7, 0, false);
    // tests_mate::mateab_solve(17, 256, false);
    // tests_mate::mateab_solve(17, 256, true);
    // tests_mate::mateab_solve_named("KBN-K", 1024, true); // mate in 31: 1.4 G nodes, 5 to 6 minutes

    // --- milestone 4: list-based search, 10 to 50 s per four-piece position, about 3 GB (the pawn test exceeds 30 M positions at ply 9) ---
    // tests_mate::matelist_solve("KRR-K", 0, 200, 30_000_000);
    // tests_mate::matelist_solve("KBN-K", 0, 200, 30_000_000);

    // --- milestone 5: endgame tables ---
    // tests_egtb::egtb_index_roundtrip(7); // 16 s
    // tests_egtb::egtb_generate(&["all"], 12, false, true); // 15 s + 7 s verification
    // tests_egtb::egtb_load_or_generate(12);
    // tests_mate::mateab_solve_tables(31, 0, false); // four-piece positions are root hits, pawns 464,248 nodes (identical to Go)
    // tests_mate::mateab_solve_tables_named("KQ-KBN", 1024, true); // mate in 39 with five pieces: open how long, user runs it

    // --- milestone 4, step 4: df-pn (size_mb, codec "sat"/"float", options m mobility, e/E/x/X epsilon 1/8 1/2 1 2, f final entries, t tables, i iterative) ---
    // tests_pn::matepn_solve(15, 256, "sat", "mEf"); // KQ-KN 181,065 visits, KR-KR 1,125,947, identical to Go
    // tests_pn::matepn_solve_named("KBB-K", 1024, "sat", "mEf"); // proven after 109.5 M visits, 336 s, proof tree incomplete (27 M replaced)
    // --- five pieces with the endgame tables as oracle: the first case where search and tables must cooperate, open how long ---
    // tests_mate::mateab_solve_tables_named("KQ-KBN", 1024, true);
    // tests_pn::matepn_solve_named("KQ-KBN", 1024, "sat", "mEft");

    // --- list-based search, the best version for finite spaces: KBB-K (11 s, 6.35 M positions), then KQ-KBN with a 16 GB store (400 M positions at 75 %) ---
    tests_mate::matelist_solve("KBB-K", 0, 200, 30_000_000);
    // tests_mate::matelist_solve("KQ-KBN", 0, 200, 400_000_000); // 387 M positions at ply 16, aborted at ply 17 after 960 s, 42 GB peak; the space is 700-750 M
    // tests_pn::matepn_solve(12, 256, "sat", "mEfi"); // iterative deepening: the shortest mate, cost of all depths
}
