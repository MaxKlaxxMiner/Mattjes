//! Milestone 2: hash key experiments. Mirrors `mattjesGo/tests_hash.go`.

use std::time::{Duration, Instant};

use crate::bitboard::{self, Board, ExactKey, Key};
use crate::chess::{new_buffer, START_FEN};
use crate::perft;

/// Walks all reference positions and compares the incremental Zobrist key with a
/// full recomputation at every node.
pub fn bitboard_hash_verify(max_nodes: u64) {
    println!("zobrist key words: {} ({} bit)", bitboard::KEY_WORDS, bitboard::KEY_WORDS * 64);
    let t = &*bitboard::TABLES;
    let mut mismatches = 0u64;

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

    let mismatch_cell = std::cell::Cell::new(0u64);
    perft::run(
        "bitboard / zobrist incremental == full recompute at every node",
        |fen, depth| {
            let mut b = Board::from_fen(fen)?;
            let mut mm = 0u64;
            walk(&mut b, depth - 1, &mut mm);
            mismatch_cell.set(mismatch_cell.get() + mm);
            if mm != 0 {
                return Err(format!("{} key mismatches", mm));
            }
            Ok(bitboard::perft_recursive(&mut b, depth))
        },
        max_nodes,
    );
    mismatches += mismatch_cell.get();
    let _ = t;
    println!("total key mismatches: {}\n", mismatches);
}

/// Measures what computing a key from scratch at every node costs, compared with
/// the plain perft (which already maintains the incremental key).
pub fn bitboard_perft_key_cost(max_nodes: u64) {
    fn run_variant(name: &str, max_nodes: u64, key: fn(&Board) -> u64) -> u64 {
        fn walk(b: &mut Board, depth: u32, key: fn(&Board) -> u64, sink: &mut u64) -> u64 {
            *sink ^= key(b);
            let mut buf = new_buffer();
            let n = b.gen_moves(&mut buf);
            if depth <= 1 {
                return n as u64;
            }
            let mut total = 0;
            let s = b.state();
            for &m in &buf[..n] {
                b.do_move(m);
                total += walk(b, depth - 1, key, sink);
                b.undo_move(m, s);
            }
            total
        }
        let sink = std::cell::Cell::new(0u64);
        perft::run(
            &format!("bitboard / perft + {} per node", name),
            |fen, depth| {
                let mut b = Board::from_fen(fen)?;
                let mut s = 0u64;
                let nodes = walk(&mut b, depth, key, &mut s);
                sink.set(sink.get() ^ s);
                Ok(nodes)
            },
            max_nodes,
        );
        sink.get()
    }
    let t = &*bitboard::TABLES;
    let _ = t;
    let mut sink = 0u64;
    sink ^= run_variant("crc64 recompute (yacboard style)", max_nodes, |b| b.crc64());
    sink ^= run_variant("zobrist full recompute", max_nodes, |b| bitboard::key_lo(&b.zobrist_full(&bitboard::TABLES)));
    sink ^= run_variant("exact 32-byte key + fold", max_nodes, |b| bitboard::exact_hash(&b.exact_key()));
    sink ^= run_variant("incremental key read (baseline)", max_nodes, |b| bitboard::key_lo(&b.key));
    println!("(sink {:x})\n", sink);
}

/// Breadth-first search keeping only distinct positions per ply (exact 32-byte key),
/// then counts how many distinct keys the shorter key types produce for the same set.
/// The difference is the number of collisions. Returns the last ply's unique positions.
pub fn bitboard_unique_positions(fen: &str, max_depth: u32, max_children: usize) -> Vec<ExactKey> {
    let root = Board::from_fen(fen).expect("valid FEN");
    println!("=== unique positions per ply ===\n    {}", fen);
    println!(
        "    {:>4} {:>14} {:>12} {:>8} {:>9} | collisions: {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}",
        "ply", "children", "unique", "time", "memory", "zob64", "zob128", "crc64", "zob32", "crc32lo", "crc32hi"
    );

    let mut level: Vec<ExactKey> = vec![root.exact_key()];
    let mut buf = new_buffer();
    for ply in 1..=max_depth {
        let start = Instant::now();
        let mut child_count = 0usize;
        for k in &level {
            child_count += Board::from_exact_key(k).gen_moves(&mut buf);
        }
        if child_count > max_children {
            println!("    {:>4} {:>14}   (limit {} children)", ply, group(child_count as u64), max_children);
            break;
        }
        let mut children: Vec<ExactKey> = Vec::with_capacity(child_count);
        for k in &level {
            let b = Board::from_exact_key(k);
            let n = b.gen_moves(&mut buf);
            for &m in &buf[..n] {
                let mut child = b;
                child.do_move(m);
                children.push(child.exact_key());
            }
        }
        children.sort_unstable();
        children.dedup();
        level = children;
        let unique = level.len();

        let mut z64: Vec<u64> = Vec::with_capacity(unique);
        let mut z128: Vec<Key> = Vec::with_capacity(unique);
        let mut crc: Vec<u64> = Vec::with_capacity(unique);
        for k in &level {
            let b = Board::from_exact_key(k);
            z128.push(b.key);
            z64.push(bitboard::key_lo(&b.key));
            crc.push(b.crc64());
        }
        fn collisions<T: Ord>(mut keys: Vec<T>) -> usize {
            let n = keys.len();
            keys.sort_unstable();
            keys.dedup();
            n - keys.len()
        }
        let c_zob32 = collisions(z64.iter().map(|k| k & 0xffff_ffff).collect());
        let c_crc32lo = collisions(crc.iter().map(|k| k & 0xffff_ffff).collect());
        let c_crc32hi = collisions(crc.iter().map(|k| k >> 32).collect());
        let c_zob128 = collisions(z128);
        let c_zob64 = collisions(z64);
        let c_crc64 = collisions(crc);

        println!(
            "    {:>4} {:>14} {:>12} {:>8} {:>9} | {:>19} {:>8} {:>8} {:>8} {:>8} {:>8}",
            ply,
            group(child_count as u64),
            group(unique as u64),
            fmt_ms(start.elapsed()),
            fmt_mb(unique * std::mem::size_of::<ExactKey>()),
            c_zob64,
            c_zob128,
            c_crc64,
            c_zob32,
            c_crc32lo,
            c_crc32hi
        );
        let n = unique as f64;
        println!(
            "         expected collisions for {} unique keys: 64 bit {:.2e}, 32 bit {:.1}",
            group(unique as u64),
            n * (n - 1.0) / 2.0 / 2f64.powi(64),
            n * (n - 1.0) / 2.0 / 2f64.powi(32)
        );
    }
    println!();
    level
}

/// Checks how evenly the low bits of each key type spread the given positions over
/// a table of 2^bits slots, compared with a uniform random hash.
pub fn bitboard_key_distribution(level: &[ExactKey], bits: u32) {
    let slots = 1usize << bits;
    let mask = (slots - 1) as u64;
    let n = level.len();
    let lambda = n as f64 / slots as f64;
    println!("=== index distribution: {} positions into 2^{} slots (load {:.2}) ===", group(n as u64), bits, lambda);
    println!("    {:<28} {:>10} {:>10} {:>8} {:>8}", "key -> index", "empty", "expected", "max", "z-score");

    type IndexFn = Box<dyn Fn(&Board) -> u64>;
    let variants: [(&str, IndexFn); 7] = [
        ("zobrist64 low bits", Box::new(move |b| bitboard::key_lo(&b.key) & mask)),
        ("zobrist64 high bits", Box::new(move |b| bitboard::key_lo(&b.key) >> (64 - bits))),
        ("crc64 low bits", Box::new(move |b| b.crc64() & mask)),
        ("crc64 high bits", Box::new(move |b| b.crc64() >> (64 - bits))),
        ("mix64(crc64) low bits", Box::new(move |b| bitboard::mix64(b.crc64()) & mask)),
        ("occupancy low bits (naive)", Box::new(move |b| (b.by_color[0] | b.by_color[1]) & mask)),
        ("exact key fold low bits", Box::new(move |b| bitboard::exact_hash(&b.exact_key()) & mask)),
    ];
    let mut counts = vec![0u32; slots];
    for (name, idx) in &variants {
        counts.fill(0);
        for k in level {
            let b = Board::from_exact_key(k);
            counts[idx(&b) as usize] += 1;
        }
        let (mut empty, mut max_load, mut chi2) = (0usize, 0u32, 0f64);
        for &c in &counts {
            if c == 0 {
                empty += 1;
            }
            max_load = max_load.max(c);
            let d = c as f64 - lambda;
            chi2 += d * d / lambda;
        }
        let df = (slots - 1) as f64;
        let z = (chi2 - df) / (2.0 * df).sqrt();
        println!("    {:<28} {:>10} {:>10.0} {:>8} {:>8.1}", name, empty, slots as f64 * (-lambda).exp(), max_load, z);
    }
    println!("    (z-score near 0 = indistinguishable from uniform random; large = clustered)\n");
}

pub fn start_fen() -> &'static str {
    START_FEN
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
