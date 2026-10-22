//! Patricia's network, ported (module nnue): the reference lines and the
//! incremental check (mirrors `mattjesGo/tests_nnue.go`).

use std::time::Instant;

use crate::bitboard::Board;
use crate::chess::{new_buffer, PERFT_POSITIONS};
use crate::nnue::{self, Accumulator, Delta, Phase};
use crate::tests_hash::fmt_ms;

/// Prints, for every FEN of a file ("<name> | <fen>" or a bare FEN, '#'
/// comments skipped), the line the Patricia reference build prints for
/// "eval" (tests/patricia-eval.sh <file>), so that the outputs can be
/// diffed.
pub fn nnue_verify(fenfile: &str) {
    let text = match std::fs::read_to_string(fenfile) {
        Ok(t) => t,
        Err(e) => {
            println!("{}: {}", fenfile, e);
            return;
        }
    };
    let start = Instant::now();
    let mut n = 0;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fen = line.rsplit('|').next().unwrap_or(line).trim();
        let b = match Board::from_fen(fen) {
            Ok(b) => b,
            Err(e) => {
                println!("{} | error: {}", fen, e);
                continue;
            }
        };
        println!(
            "{} | eval mat {} phase {} mg {} eg {} sac {} eval {}",
            fen,
            nnue::material(&b),
            nnue::phase_of(&b) as u8,
            nnue::raw(&b, Phase::Middlegame),
            nnue::raw(&b, Phase::Endgame),
            nnue::raw(&b, Phase::Sacrifice),
            nnue::evaluate(&b)
        );
        n += 1;
    }
    eprintln!("--- nnue: {} positions evaluated in {}", n, fmt_ms(start.elapsed()));
}

/// Walks the move trees of the perft positions to the given depth and
/// compares the incrementally updated accumulator with a fresh one after
/// every move, for the sacrifice net.
pub fn nnue_incremental(depth: usize) {
    let net = nnue::load(Phase::Sacrifice);
    let start = Instant::now();
    let (mut nodes, mut mismatches) = (0usize, 0usize);
    fn walk(b: &Board, acc: &Accumulator, d: usize, net: &nnue::Net, nodes: &mut usize, mismatches: &mut usize) {
        if d == 0 {
            return;
        }
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        for &m in &buf[..n] {
            let delta = Delta::of(b, m);
            let mut c = *b;
            c.do_move(m);
            let mut next = Accumulator::default();
            next.update(net, acc, &delta);
            let mut fresh = Accumulator::default();
            fresh.refresh(net, &c);
            *nodes += 1;
            if next != fresh {
                *mismatches += 1;
                if *mismatches <= 5 {
                    println!("  MISMATCH after {} in {}", m.uci(), b.fen());
                }
            }
            walk(&c, &next, d - 1, net, nodes, mismatches);
        }
    }
    for p in PERFT_POSITIONS {
        let b = Board::from_fen(p.fen).expect("perft FEN");
        let mut acc = Accumulator::default();
        acc.refresh(net, &b);
        walk(&b, &acc, depth, net, &mut nodes, &mut mismatches);
    }
    let status = if mismatches > 0 { format!("{} MISMATCHES", mismatches) } else { "ok".to_string() };
    println!("--- nnue incremental: {} moves checked to depth {}, {}, {}", nodes, depth, status, fmt_ms(start.elapsed()));
}

/// Measures the evaluation throughput over the move trees of the perft
/// positions: a fresh accumulator per node (refresh and evaluate) and the
/// incremental path (update and evaluate), sacrifice net; the checksum of
/// the scores keeps the work honest.
pub fn nnue_bench(depth: usize) {
    let net = nnue::load(Phase::Sacrifice);
    println!("nnue bench: {}", if nnue::simd() { "AVX2" } else { "scalar" });
    fn walk(b: &Board, acc: &Accumulator, d: usize, net: &nnue::Net, incremental: bool, nodes: &mut usize, sum: &mut i64) {
        if d == 0 {
            return;
        }
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        for &m in &buf[..n] {
            let mut c = *b;
            let mut next = Accumulator::default();
            if incremental {
                let delta = Delta::of(b, m);
                c.do_move(m);
                next.update(net, acc, &delta);
            } else {
                c.do_move(m);
                next.refresh(net, &c);
            }
            *sum += next.evaluate(net, c.white_move) as i64;
            *nodes += 1;
            walk(&c, &next, d - 1, net, incremental, nodes, sum);
        }
    }
    for (incremental, name) in [(false, "refresh + evaluate"), (true, "update + evaluate ")] {
        let start = Instant::now();
        let (mut nodes, mut sum) = (0usize, 0i64);
        for p in PERFT_POSITIONS {
            let b = Board::from_fen(p.fen).expect("perft FEN");
            let mut acc = Accumulator::default();
            acc.refresh(net, &b);
            walk(&b, &acc, depth, net, incremental, &mut nodes, &mut sum);
        }
        let el = start.elapsed();
        println!("    {}: {} evaluations in {}, {:.0} ns each, {:.2} M/s, checksum {}", name, nodes, fmt_ms(el), el.as_nanos() as f64 / nodes as f64, nodes as f64 / el.as_secs_f64() / 1e6, sum);
    }
}
