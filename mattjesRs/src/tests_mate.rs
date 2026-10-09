//! Milestone 4, step 1: generator extensions for the mate search.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::bitboard::{self, Board};
use crate::chess::{new_buffer, MatePosition, Move, MATE_POSITIONS, PERFT_POSITIONS, UNKNOWN};
use crate::egtb;
use crate::mateab::{self, Oracle};
use crate::matelist;
use crate::perft;
use crate::tests_egtb;
use crate::tests_hash::{fmt_ms, group};
use crate::tt::{self, TransTable};

// --- milestone 4, step 2 + 3: mate search with mate window, without / with TT ---

/// Runs the reference mate positions up to `max_mate_in` moves with the
/// depth-first search, without a table (`size_mb` 0) or with a direct-mapped or
/// bucket table of `size_mb`. The mate must appear exactly at depth 2*mate_in-1
/// plies: earlier would contradict the tablebase, later or never is a miss.
/// Prints nodes, time, table hits and the principal variation per depth. The
/// table is cleared per position, so Go and Rust can be compared node by node.
pub fn mateab_solve(max_mate_in: u32, size_mb: usize, bucketed: bool) {
    mateab_run(&|p| p.mate_in <= max_mate_in, size_mb, bucketed, None);
}

/// Runs a single reference position by name, for the long ones.
pub fn mateab_solve_named(name: &str, size_mb: usize, bucketed: bool) {
    mateab_run(&|p| p.name == name, size_mb, bucketed, None);
}

/// Runs the reference positions with the endgame tables as oracle (milestone
/// 5): positions with up to four pieces are answered at the root, the
/// five-piece ones as soon as a capture reaches a table.
pub fn mateab_solve_tables(max_mate_in: u32, size_mb: usize, bucketed: bool) {
    let set = tests_egtb::egtb_load_or_generate(12);
    mateab_run(&|p| p.mate_in <= max_mate_in, size_mb, bucketed, Some(&set));
}

pub fn mateab_solve_tables_named(name: &str, size_mb: usize, bucketed: bool) {
    let set = tests_egtb::egtb_load_or_generate(12);
    mateab_run(&|p| p.name == name, size_mb, bucketed, Some(&set));
}

fn mateab_run(selected: &dyn Fn(&MatePosition) -> bool, size_mb: usize, bucketed: bool, tables: Option<&egtb::Set>) {
    match tables {
        None => mateab_run_oracle(selected, size_mb, bucketed, mateab::Material, ""),
        Some(set) => mateab_run_oracle(selected, size_mb, bucketed, mateab::Tables { set }, ", endgame tables"),
    }
}

fn mateab_run_oracle<O: Oracle + Copy>(selected: &dyn Fn(&MatePosition) -> bool, size_mb: usize, bucketed: bool, oracle: O, suffix: &str) {
    if size_mb == 0 {
        run_mateab::<O, tt::Table>(&format!("no TT{}", suffix), oracle, None, selected);
    } else if bucketed {
        run_mateab(&format!("4-way bucket TT {} MB{}", size_mb, suffix), oracle, Some(tt::Buckets::new(size_mb)), selected);
    } else {
        run_mateab(&format!("direct-mapped TT {} MB{}", size_mb, suffix), oracle, Some(tt::Table::new(size_mb)), selected);
    }
}

fn run_mateab<O: Oracle + Copy, T: TransTable + 'static>(title: &str, oracle: O, mut table: Option<T>, selected: &dyn Fn(&MatePosition) -> bool) {
    println!("=== mateab / mate search with mate window, iterative deepening, {} ===", title);
    let with_tables = title.contains("endgame tables");
    if let Some(t) = table.as_ref() {
        assert!(mateab::VALUE_BITS <= t.value_bits(), "table too small for the mateab value layout");
    }
    let mut ok = true;
    let mut total_nodes = 0u64;
    let mut total_time = Duration::ZERO;
    for (i, p) in MATE_POSITIONS.iter().enumerate() {
        if !selected(p) {
            continue;
        }
        println!("[{}] {}  {}  mate in {}", i + 1, p.name, p.fen, p.mate_in);
        let b = Board::from_fen(p.fen).expect("valid FEN");
        if let Some(t) = table.as_mut() {
            t.clear();
            t.reset_stats();
        }
        let mut s = mateab::Searcher::new(oracle, table.take());
        let start = Instant::now();
        let mut last_nodes = 0u64;
        let mut depth_start = start;
        let progress_start = Rc::new(Cell::new(start));
        let ps = progress_start.clone();
        s.progress = Some(Box::new(move |nodes, _current, t: Option<&T>| {
            let mut line = format!("              ... {} nodes, {}", group(nodes), fmt_ms(ps.get().elapsed()));
            if let Some(t) = t {
                let st = t.stats();
                line += &format!(", tt {} stores, {} replaced", group(st.stores), group(st.replaced));
            }
            println!("{}", line);
        }));
        let r = s.solve(&b, 2 * p.mate_in - 1, |plies, r| {
            let now = Instant::now();
            if r.mate_plies != 0 {
                let mut pv = pv_string(&r.pv);
                if r.pv.len() < r.mate_plies as usize {
                    pv += " ...";
                }
                println!("    depth {:>2}: mate in {}  {:>14} nodes {:>9}  pv {}", plies, r.mate_plies.div_ceil(2), group(r.nodes - last_nodes), fmt_ms(now - depth_start), pv);
            } else {
                println!("    depth {:>2}: no mate    {:>14} nodes {:>9}", plies, group(r.nodes - last_nodes), fmt_ms(now - depth_start));
            }
            last_nodes = r.nodes;
            depth_start = now;
            progress_start.set(now);
        });
        let elapsed = start.elapsed();
        total_nodes += r.nodes;
        total_time += elapsed;
        if r.mate_plies == 0 {
            println!("    FAIL: no mate found within {} plies", 2 * p.mate_in - 1);
            ok = false;
        } else if r.mate_plies != 2 * p.mate_in - 1 {
            println!("    FAIL: mate in {} plies, expected {}", r.mate_plies, 2 * p.mate_in - 1);
            ok = false;
        } else {
            print!("    ok: {} nodes in {}", group(r.nodes), fmt_ms(elapsed));
            if with_tables {
                print!(", {} table hits", group(r.oracle_hits));
            }
            if let Some(t) = s.table() {
                let st = t.stats();
                print!(
                    ", tt: {} probes, {} hits ending the node, {} stores, {} replaced, fill {:.1}%",
                    group(st.probes),
                    group(r.tt_hits),
                    group(st.stores),
                    group(st.replaced),
                    100.0 * t.used() as f64 / t.slots() as f64
                );
            }
            println!();
        }
        table = s.into_table();
    }
    print!("--- total: {} nodes in {}", group(total_nodes), fmt_ms(total_time));
    println!("{}\n", if ok { "  [all ok]" } else { "  [FAILURES]" });
}

// --- milestone 4: list-based search (breadth-first enumeration + retrograde) ---

/// Runs the reference positions up to `max_mate_in` (or a single one by name
/// when `name` is not empty) with the list-based search: every reachable position
/// enumerated once, mate distances resolved backwards. Positions whose reachable
/// graph exceeds `max_positions` are reported as skipped.
pub fn matelist_solve(name: &str, max_mate_in: u32, max_plies: u32, max_positions: usize) {
    println!("=== matelist / breadth-first enumeration + retrograde, horizon {} plies, up to {} positions ===", max_plies, group(max_positions as u64));
    let mut ok = true;
    let mut total_time = Duration::ZERO;
    for (i, p) in MATE_POSITIONS.iter().enumerate() {
        if (!name.is_empty() && p.name != name) || (name.is_empty() && p.mate_in > max_mate_in) {
            continue;
        }
        println!("[{}] {}  {}  mate in {}", i + 1, p.name, p.fen, p.mate_in);
        let b = Board::from_fen(p.fen).expect("valid FEN");
        let start = Instant::now();
        let result = matelist::solve(&b, max_plies, max_positions, None, &mut |line: &str| {
            println!("    {}  {}", line, fmt_ms(start.elapsed()));
        });
        let elapsed = start.elapsed();
        total_time += elapsed;
        let r = match result {
            Ok(r) => r,
            Err(e) => {
                println!("    skipped: {} ({})", e, fmt_ms(elapsed));
                continue;
            }
        };
        println!(
            "    {} positions ({} expanded), {} edges, {} plies, {} resolved, longest mate {} plies",
            group(r.positions as u64),
            group(r.expanded as u64),
            group(r.edges as u64),
            r.plies,
            group(r.resolved as u64),
            r.max_level
        );
        if r.mate_plies == 0 {
            println!("    FAIL: no mate found ({})", fmt_ms(elapsed));
            ok = false;
        } else if r.mate_plies != 2 * p.mate_in - 1 {
            println!("    FAIL: mate in {} plies, expected {} ({})", r.mate_plies, 2 * p.mate_in - 1, fmt_ms(elapsed));
            ok = false;
        } else {
            println!(
                "    ok: mate in {} in {}, proof DAG {} of {} positions ({:.1}%)  pv {}",
                r.mate_plies.div_ceil(2),
                fmt_ms(elapsed),
                group(r.proof_positions as u64),
                group(r.positions as u64),
                100.0 * r.proof_positions as f64 / r.positions as f64,
                pv_string(&r.pv)
            );
        }
    }
    print!("--- total: {}", fmt_ms(total_time));
    println!("{}\n", if ok { "  [all ok]" } else { "  [FAILURES]" });
}

pub fn pv_string(pv: &[Move]) -> String {
    pv.iter().map(|m| m.uci()).collect::<Vec<_>>().join(" ")
}

/// Compares the leaf classification (captures, en passant, castles, promotions,
/// checks, discovered and double checks, mates) with the chessprogramming.org
/// tables for every position that has them. This verifies `gives_check` /
/// `checkers_after` and `has_moves` against foreign data.
pub fn bitboard_perft_detailed(max_nodes: u64) {
    println!("=== bitboard / perft detail columns vs chessprogramming.org ===");
    let mut ok = true;
    let mut total_nodes = 0u64;
    let mut total_time = Duration::ZERO;
    for (i, p) in PERFT_POSITIONS.iter().enumerate() {
        if p.details.is_empty() {
            continue;
        }
        println!("[{}] {}\n    {}", i + 1, p.name, p.fen);
        println!("    {:>5} {:>16} {:>12} {:>8} {:>10} {:>10} {:>11} {:>9} {:>7} {:>9}", "depth", "nodes", "captures", "ep", "castles", "promos", "checks", "discov", "double", "mates");
        let mut b = Board::from_fen(p.fen).expect("valid FEN");
        for (depth, (&nodes, want)) in (1u32..).zip(p.nodes.iter().zip(p.details)) {
            if nodes > max_nodes {
                break;
            }
            let start = Instant::now();
            let d = bitboard::perft_detailed(&mut b, depth);
            let elapsed = start.elapsed();
            total_nodes += d.nodes;
            total_time += elapsed;

            let mut status = "ok".to_string();
            let mut fail = |name: &str, got: u64, exp: u64| {
                if exp != UNKNOWN && got != exp {
                    status = format!("FAIL {} got {} expected {}", name, got, exp);
                    ok = false;
                }
            };
            fail("nodes", d.nodes, nodes);
            fail("captures", d.detail.captures, want.captures);
            fail("ep", d.detail.en_passant, want.en_passant);
            fail("castles", d.detail.castles, want.castles);
            fail("promotions", d.detail.promotions, want.promotions);
            fail("checks", d.detail.checks, want.checks);
            fail("discovery", d.detail.discovery_checks, want.discovery_checks);
            fail("double", d.detail.double_checks, want.double_checks);
            fail("checkmates", d.detail.checkmates, want.checkmates);
            println!(
                "    {:>5} {:>16} {:>12} {:>8} {:>10} {:>10} {:>11} {:>9} {:>7} {:>9}  {} {}",
                depth,
                group(d.nodes),
                group(d.detail.captures),
                group(d.detail.en_passant),
                group(d.detail.castles),
                group(d.detail.promotions),
                group(d.detail.checks),
                group(d.detail.discovery_checks),
                group(d.detail.double_checks),
                group(d.detail.checkmates),
                status,
                fmt_ms(elapsed)
            );
        }
    }
    print!("--- total: {} nodes in {}", group(total_nodes), fmt_ms(total_time));
    println!("{}\n", if ok { "  [all ok]" } else { "  [FAILURES]" });
}

/// Walks all reference positions and checks at every node that `has_moves`,
/// `is_mate` and `is_stalemate` agree with `gen_moves` + `in_check`, and for
/// every move that `gives_check` agrees with playing the move and asking
/// `in_check`, and that `gen_checks` returns exactly the checking moves.
pub fn bitboard_mate_verify(max_nodes: u64) {
    fn walk(b: &mut Board, depth: u32, mismatches: &mut u64) {
        let mut buf = new_buffer();
        let mut checks = new_buffer();
        let n = b.gen_moves(&mut buf);
        let in_check = b.in_check();
        if b.has_moves() != (n > 0) || b.is_mate() != (n == 0 && in_check) || b.is_stalemate() != (n == 0 && !in_check) {
            *mismatches += 1;
        }
        let n_checks = b.gen_checks(&mut checks);
        let mut found = 0;
        let s = b.state();
        for &m in &buf[..n] {
            let gives = b.gives_check(m);
            b.do_move(m);
            if gives != b.in_check() {
                *mismatches += 1;
            }
            if gives {
                if found < n_checks && checks[found] == m {
                    found += 1;
                } else {
                    *mismatches += 1;
                }
            }
            if depth > 1 {
                walk(b, depth - 1, mismatches);
            }
            b.undo_move(m, s);
        }
        if found != n_checks {
            *mismatches += 1;
        }
    }

    let total = Cell::new(0u64);
    perft::run(
        "bitboard / has_moves, is_mate, gives_check, gen_checks == gen_moves + do_move + in_check at every node",
        |fen, depth| {
            let mut b = Board::from_fen(fen)?;
            let mut mm = 0u64;
            walk(&mut b, depth, &mut mm);
            total.set(total.get() + mm);
            if mm != 0 {
                return Err(format!("{} mismatches", mm));
            }
            Ok(bitboard::perft_recursive(&mut b, depth))
        },
        max_nodes,
    );
    println!("total mismatches: {}\n", total.get());
}
