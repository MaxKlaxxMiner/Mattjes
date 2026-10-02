//! Milestone 2: hash key regression tests. The comparison experiments (CRC64,
//! truncated keys, index distribution) are documented in docs/m2-hash-keys.md.

use std::time::{Duration, Instant};

use crate::bitboard::{self, packed_fixed_record, Board, Codec, Key, PackedFixed};
use crate::chess::{new_buffer, START_FEN};
use crate::perft;

/// Walks all reference positions and compares the incremental Zobrist key with a
/// full recomputation at every node, forwards and backwards.
pub fn bitboard_hash_verify(max_nodes: u64) {
    fn walk(b: &mut Board, depth: u32, mismatches: &mut u64) {
        let t = &*bitboard::TABLES;
        if b.key != b.zobrist_full(t) {
            *mismatches += 1;
        }
        if depth == 0 {
            return;
        }
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        let s = b.state();
        for &m in &buf[..n] {
            b.do_move(m);
            walk(b, depth - 1, mismatches);
            b.undo_move(m, s);
            if b.key != b.zobrist_full(t) {
                *mismatches += 1;
            }
        }
    }

    let total = std::cell::Cell::new(0u64);
    perft::run(
        "bitboard / zobrist incremental == full recompute at every node",
        |fen, depth| {
            let mut b = Board::from_fen(fen)?;
            let mut mm = 0u64;
            walk(&mut b, depth - 1, &mut mm);
            total.set(total.get() + mm);
            if mm != 0 {
                return Err(format!("{} key mismatches", mm));
            }
            Ok(bitboard::perft_recursive(&mut b, depth))
        },
        max_nodes,
    );
    println!("total key mismatches: {}\n", total.get());
}

/// Distinct positions after n plies from the start position
/// (OEIS A083276, en passant only counted when a legal capture exists).
const UNIQUE_REFERENCE: [usize; 7] = [20, 400, 5362, 72078, 822518, 9417681, 96400068];

type Record = [u8; PackedFixed::MAX_BYTES];

/// The exact identity of a position: its PackedFixed record with the move counters zeroed.
fn position_record(b: &Board) -> Record {
    let mut c = *b;
    c.halfmove_clock = 0;
    c.move_number = 0;
    packed_fixed_record(&c)
}

/// Breadth-first search keeping only distinct positions per ply (exact record),
/// compares the counts with OEIS A083276 for the start position and reports
/// Zobrist collisions among the distinct positions. This is the prototype of the
/// list-based search with deduplication.
pub fn bitboard_unique_positions(fen: &str, max_depth: u32, max_children: usize) {
    let root = Board::from_fen(fen).expect("valid FEN");
    let is_start = fen == START_FEN;
    println!("=== unique positions per ply ===\n    {}", fen);
    println!("    {:>4} {:>14} {:>12} {:>10} {:>8} {:>9} {:>10}", "ply", "children", "unique", "reference", "time", "memory", "zobrist");

    let mut ok = true;
    let mut level: Vec<Record> = vec![position_record(&root)];
    let mut buf = new_buffer();
    for ply in 1..=max_depth as usize {
        let start = Instant::now();
        let mut child_count = 0usize;
        for k in &level {
            child_count += PackedFixed::decode(k).0.gen_moves(&mut buf);
        }
        if child_count > max_children {
            println!("    {:>4} {:>14}   (limit {} children)", ply, group(child_count as u64), max_children);
            break;
        }
        let mut children: Vec<Record> = Vec::with_capacity(child_count);
        for k in &level {
            let b = PackedFixed::decode(k).0;
            let n = b.gen_moves(&mut buf);
            for &m in &buf[..n] {
                let mut child = b;
                child.do_move(m);
                children.push(position_record(&child));
            }
        }
        children.sort_unstable();
        children.dedup();
        level = children;
        let unique = level.len();

        let mut keys: Vec<Key> = level.iter().map(|k| PackedFixed::decode(k).0.key).collect();
        keys.sort_unstable();
        keys.dedup();
        let collisions = unique - keys.len();

        let mut reference = "-".to_string();
        if is_start && ply - 1 < UNIQUE_REFERENCE.len() {
            reference = group(UNIQUE_REFERENCE[ply - 1] as u64);
            if UNIQUE_REFERENCE[ply - 1] != unique {
                reference += " FAIL";
                ok = false;
            }
        }
        println!(
            "    {:>4} {:>14} {:>12} {:>10} {:>8} {:>9} {:>10}",
            ply,
            group(child_count as u64),
            group(unique as u64),
            reference,
            fmt_ms(start.elapsed()),
            fmt_mb(unique * PackedFixed::MAX_BYTES),
            collisions
        );
    }
    println!("    {}\n", if ok { "[all ok]" } else { "[FAILURES]" });
}

fn group(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn fmt_ms(d: Duration) -> String {
    if d < Duration::from_secs(1) {
        format!("{:.0} ms", d.as_micros() as f64 / 1000.0)
    } else {
        format!("{:.2} s", d.as_secs_f64())
    }
}

fn fmt_mb(bytes: usize) -> String {
    if bytes < 1 << 20 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1u64 << 20) as f64)
    }
}
