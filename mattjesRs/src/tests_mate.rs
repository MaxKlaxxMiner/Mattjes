//! Milestone 4, step 1: generator extensions for the mate search.

use std::cell::Cell;
use std::time::{Duration, Instant};

use crate::bitboard::{self, Board};
use crate::chess::{new_buffer, PERFT_POSITIONS, UNKNOWN};
use crate::perft;
use crate::tests_hash::{fmt_ms, group};

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
