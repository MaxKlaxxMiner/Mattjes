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
mod lz;
mod mateab;
mod matelist;
mod matepn;
mod perft;
mod tests_egtb;
mod tests_hash;
mod nnue;
mod pairs;
mod rollout;
mod tests_pairs;
mod tests_mate;
mod tests_nnue;
mod tests_rollout;
mod tests_perft;
mod tests_pn;
mod tests_tt;
mod tests_wdl;
mod tests_syzygy;
mod tests_wdllist;
mod tt;
mod ttstore;
mod uci;
mod syzygy;
mod wdllist;
mod wdlpn;

fn main() {
    // The bare binary is the UCI engine, as a chess GUI expects (module uci,
    // threads via the UCI option). Arguments select tests and actions:
    //   test                                              the experiment block in experiments()
    //   egtb-list <pieces>                                names of all materials with that many pieces
    //   egtb-measure <name>... [--verify] [--workers N]   generate or load the tables, log to the cache dir
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    if args.is_empty() {
        uci::run(std::io::stdin(), std::io::stdout(), &format!("Mattjes {} (Rust)", uci::VERSION), cpus);
        return;
    }
    {
        let option = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse::<usize>().ok());
        let workers = option("--workers").unwrap_or(12);
        match args[0].as_str() {
            "test" => experiments(),
            "egtb-list" => tests_egtb::egtb_list(args.get(1).and_then(|s| s.parse().ok()).unwrap_or(5)),
            "egtb-measure" => {
                let verify = args.iter().any(|a| a == "--verify");
                let mut names = Vec::new();
                let mut skip = false;
                for a in &args[1..] {
                    if skip {
                        skip = false;
                    } else if a == "--workers" {
                        skip = true;
                    } else if !a.starts_with("--") {
                        names.push(a.as_str());
                    }
                }
                tests_egtb::egtb_measure(&names, workers, verify);
            }
            other => {
                eprintln!("unknown command {}", other);
                std::process::exit(2);
            }
        }
    }
}

/// The i-th argument after "test" (or `def`), so that an experiment can take
/// its parameters from the command line.
fn test_arg(i: usize, def: &str) -> String {
    std::env::args().nth(2 + i).unwrap_or_else(|| def.to_string())
}

/// The console test bench: enable one line at a time, build, then start the
/// binary with "test".
/// test_arg as a number (def when missing or not a number).
fn test_int(i: usize, def: usize) -> usize {
    test_arg(i, "").parse().unwrap_or(def)
}

fn experiments() {
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

    // --- milestone 4, step 4: df-pn (size_mb, codec "sat"/"float", options m mobility, l lazy mobility, e/E/x/X epsilon 1/8 1/2 1 2, f final entries, t tables, i iterative) ---
    // tests_pn::matepn_solve(15, 256, "sat", "mEf"); // KQ-KN 368,362 visits, KR-KR 1,206,343, identical to Go (the visits swing 4x with the table size, m4-mate-search.md step 5)
    // --- mobility experiment (docs/handout-matepn-mobility.md): the three leaf initialisations, visits must match Go per variant ---
    // tests_pn::matepn_solve(15, 256, "sat", "mEf"); // A: mobility, 1,672,777 visits in total (the default, won the benchmark: 70.5 s)
    // tests_pn::matepn_solve(15, 256, "sat", "Ef"); // B: 1/1 leaves, 3,227,469 visits (benchmark 114.4 s)
    // tests_pn::matepn_solve(15, 256, "sat", "lEf"); // C: lazy, the move count when a node is entered, 8,694,695 visits (benchmark 83.8 s)
    // tests_pn::matepn_solve_named("KBB-K", 1024, "sat", "mEf"); // proven after 109.5 M visits, 336 s, proof tree incomplete (27 M replaced)
    // --- five pieces with the endgame tables as oracle: the first case where search and tables must cooperate, open how long ---
    // tests_mate::mateab_solve_tables_named("KQ-KBN", 1024, true);
    // tests_pn::matepn_solve_named("KQ-KBN", 1024, "sat", "mEft");

    // --- list-based search, the best version for finite spaces: KBB-K (11 s, 6.35 M positions), then KQ-KBN with a 16 GB store (400 M positions at 75 %) ---
    // tests_mate::matelist_solve("KBB-K", 0, 200, 30_000_000);
    // tests_mate::matelist_solve("KQ-KBN", 0, 200, 400_000_000); // 387 M positions at ply 16, aborted at ply 17 after 960 s, 42 GB peak; the space is 700-750 M

    // --- milestone 5, measurement series: five- and six-piece tables (time, size, checksum), cache dir mattjes-egtb-cache/ ---
    // tests_egtb::egtb_load_or_generate(12); // makes sure the four-piece base exists (15 s once, then 0.2 s); larger materials via "egtb-measure <name>"
    // --- interval oracle (docs/m5-endgame-tables.md, quantised cache files): KRR-KN with 5-KRRKN.bin (buckets of 8 moves) = the search resolves the 16-ply window; without the file 87,503 visits (Go = Rust) ---
    // --- block cache of the packed table (docs/m5-endgame-tables.md, "Blockformat"): hits, misses and the working set ---
    // tests_pn::matepn_solve_fen("KRR-KN after Nh2", "8/8/4k3/8/8/8/RK5n/2R5 w - - 2 2", 7, 256, "sat", "mEft"); // 9,988 visits, 3,997 of 14,784 blocks touched (Go = Rust); without the file 87,503 visits
    // --- replacement by value (docs/m4-mate-search.md, step 7): b = buckets with priority classes (open < disproven < proven < final), the direct table for comparison; visits must match Go ---
    // --- proof goal "win" (docs/m4-mate-search.md, step 8): w = a won table position proves the node whatever its distance, T = four-piece base only; without tables the goals coincide (same 1,370,235 visits) ---
    // tests_pn::matepn_solve(15, 256, "sat", "mEfbw");
    // tests_pn::matepn_solve_named("KQ-KBN", 1024, "sat", "mEfbTwi"); // five pieces with the four-piece tables as oracle: no forced capture into a won table within 21 plies (5.6 M visits, 26 s, Go = Rust), the cost grows 1.8x per two plies; the author runs it further
    // tests_pn::matepn_solve_fen("KRR-KN after Nh2", "8/8/4k3/8/8/8/RK5n/2R5 w - - 2 2", 7, 256, "sat", "mEfbTwi"); // the knight must fall: win proven at depth 11 after 294,072 visits, mate in at most 37 plies (one defence ends in a won KR-KN), Go = Rust
    // tests_pn::matepn_solve_fen("KRR-KN after Nh2", "8/8/4k3/8/8/8/RK5n/2R5 w - - 2 2", 7, 256, "sat", "mEfbTi"); // the mate goal for comparison: mate in 13 plies at depth 13 after 1,243,719 visits

    // --- milestone 7: WDL search (docs/m7-wdl-search-design.md), names "" = all WDL_POSITIONS, options m mobility, e/E/x/X epsilon, b buckets, T four-piece base as oracle ---
    // "test <names> <options> <codec>" overrides the arguments from the command line, so variants run without a rebuild
    // 2026-10-21: epsilon 1 (x) is the setting for disproofs, 1/2 (E) ran forever on KP-K rook pawn; sat beats float (docs/m7-wdl-search.md); visits must match Go
    // tests_wdl::wdlpn_solve(&test_arg(0, ""), 256, &test_arg(2, "sat"), &test_arg(1, "mxb"));
    // --- step 2: explicit graph (docs/m7-wdl-search.md), "test <names> <options>": B brute force, p descent alone, e/E/x/X epsilon, w/l number variants, v progress; expansions must match Go ---
    // options t = four-piece tables as oracle, s = Syzygy WDL on top (SYZYGY_DEFAULT_DIR)
    // option n = Patricia's net in the certificate builder's policy, i = the descent's numbers from the net (docs/nnue-patricia.md)
    // tests_wdllist::wdllist_solve(&test_arg(0, ""), &test_arg(1, "x"));
    // --- package pairs (Syzygy's scheme, own container) on the WDL streams the Go testbench dumps to tests/wdl: "test <names> <blockBytes> <indexBits> <pairsPerPass> <minCount> <maxSymbols> <maxSymbolLen>"; lines identical to Go (checksum column) ---
    tests_pairs::pairs_measure(
        "tests/wdl",
        &test_arg(0, ""),
        pairs::Options { block_bytes: test_int(1, 0), index_bits: test_int(2, 0), pairs_per_pass: test_int(3, 64), min_count: test_int(4, 8), max_symbols: test_int(5, 4095), max_symbol_len: test_int(6, 256), workers: 12 },
        true,
    );
    // --- rollouts (module rollout, docs/nnue-patricia.md): "test <names> <options> <maxPlies>", options t/s oracle, d depth 2, v lines; every root move played out ---
    // tests_rollout::rollout_positions(&test_arg(0, ""), &test_arg(1, "s"), test_int(2, 200)); // 2026-10-22: 561 of 567 root moves with Syzygy, 478 (depth 2: 516) with the four-piece tables, 284 without
    // tests_rollout::rollout_line(&test_arg(0, "8/8/4k3/8/8/8/RK6/2R2n2 b - - 1 1"), &test_arg(1, "s"), test_int(2, 200));
    // --- Patricia's network (module nnue): "test <fenfile>" prints the reference lines of tests/patricia-eval.sh, then the incremental check and the throughput ---
    // tests_nnue::nnue_verify(&test_arg(0, "tests/nnue-smoke.txt")); // 2026-10-22: 55 positions identical to the reference build (docs/nnue-patricia.md)
    // tests_nnue::nnue_incremental(3);
    // tests_nnue::nnue_bench(3);
    // --- Syzygy WDL reader: "test <dir> <materials>" compares probes with the own tables on random legal positions ---
    // tests_syzygy::syzygy_verify(&test_arg(0, tests_syzygy::SYZYGY_DEFAULT_DIR), &test_arg(1, "KQKR;KRKN;KPK;KPKP;KQKP;KPPK;KRRKN;KBNKQ"), 20000); // 2026-10-22: all agree, Go and Rust
    // tests_syzygy::syzygy_positions(&test_arg(0, tests_syzygy::SYZYGY_DEFAULT_DIR)); // the root moves of the WDL test positions against the recorded win/draw/loss counts: all 31 agree
    // tests_syzygy::syzygy_probe(tests_syzygy::SYZYGY_DEFAULT_DIR, &test_arg(0, "3k4/p7/8/8/8/8/P4rr1/K6Q w - - 0 1")); // single probes, semicolon separated (seven pieces: no table, Unknown)
    // tests_pn::matepn_solve(15, 256, "sat", "mEfb"); // 1,370,235 visits in total (KQ-KN 271,120, KR-KR 1,023,102), 0 proofs evicted, every proof tree complete; identical from 256 MB up (KR-KR 1,022,904 from 1 GB)
    // tests_pn::matepn_solve(15, 256, "sat", "mEf"); // the direct table: 1,156,948 visits, 772 proofs and 3,285 final entries of KR-KR evicted, its proof tree incomplete; swings 5x with the size
    // tests_egtb::egtb_measure(&["KQKBN"], 12, true); // generated in 45 s, checksum 7b54498535f836cb, byte-identical to Go
    // tests_pn::matepn_solve(12, 256, "sat", "mEfi"); // iterative deepening: the shortest mate, cost of all depths
}
