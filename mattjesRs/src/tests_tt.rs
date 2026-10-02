//! Milestone 3: transposition tables (measurements in docs/m3-transposition-table.md).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use crate::bitboard::{self, Board, Codec, PackedFixed};
use crate::chess::{new_buffer, PERFT_POSITIONS, START_FEN};
use crate::perft;
use crate::tests_hash::{fmt_mb, fmt_ms, group, position_record, Record, UNIQUE_REFERENCE};
use crate::tt::{self, Key, TransTable};
use crate::ttstore::{Store, MAX_LOAD_PERCENT};

/// XORed into both key words, so (position, remaining depth) is simply another
/// 128-bit key: same position, different depth, different entry.
const PERFT_DEPTH_SALT: [Key; 32] = {
    let mut s = [[0u64; 2]; 32];
    let mut x = 0x9E3779B97F4A7C15u64;
    let mut i = 0;
    while i < 32 {
        let mut w = 0;
        while w < 2 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            s[i][w] = x;
            w += 1;
        }
        i += 1;
    }
    s
};

/// `perft_recursive` with a transposition table: a subtree whose (position, depth)
/// was already counted is taken from the table. Leaves are still bulk-counted at
/// depth 1, so entries start at depth 2. The value is the node count; subtrees
/// larger than the value width (24 bits at 16 Mi slots) are not stored, which
/// costs almost nothing because there are so few of them.
pub fn perft_tt<T: TransTable>(b: &mut Board, depth: u32, t: &mut T) -> u64 {
    let mut buf = new_buffer();
    let n = b.gen_moves(&mut buf);
    if depth <= 1 {
        return n as u64;
    }
    let salt = PERFT_DEPTH_SALT[depth as usize];
    let k: Key = [b.key[0] ^ salt[0], b.key[1] ^ salt[1]];
    if let Some(count) = t.probe(k) {
        return count;
    }
    let mut total = 0u64;
    let s = b.state();
    for &m in &buf[..n] {
        b.do_move(m);
        total += perft_tt(b, depth - 1, t);
        b.undo_move(m, s);
    }
    if total <= t.max_value() {
        t.store(k, total);
    }
    total
}

/// Runs the reference perft suite with a transposition table of the given size and
/// layout. The table is cleared before every (position, depth) outside the
/// measured time, so the speedup comes only from transpositions inside one run.
/// Prints hit rate, replacements and the near-miss statistics for the key-size
/// question.
pub fn bitboard_perft_tt(max_nodes: u64, size_mb: usize, bucketed: bool) {
    if bucketed {
        run_perft_tt("4-way bucket", tt::Buckets::new(size_mb), max_nodes, size_mb);
    } else {
        run_perft_tt("direct-mapped", tt::Table::new(size_mb), max_nodes, size_mb);
    }
}

fn run_perft_tt<T: TransTable>(name: &str, t: T, max_nodes: u64, size_mb: usize) {
    bitboard::init_duration(); // build the lazy tables before the first timed run
    let slots = t.slots();
    let value_bits = t.value_bits();
    let table = RefCell::new(t);
    perft::run_prepared(
        &format!("bitboard / perft with {} TT, {} MB = {} entries, {} value bits", name, size_mb, group(slots as u64), value_bits),
        || table.borrow_mut().clear(),
        |fen, depth| {
            let mut b = Board::from_fen(fen)?;
            Ok(perft_tt(&mut b, depth, &mut *table.borrow_mut()))
        },
        max_nodes,
    );
    let t = table.borrow();
    print_tt_stats(t.stats(), t.used(), slots);
}

fn print_tt_stats(s: &tt::Stats, used: usize, slots: usize) {
    let hit_rate = if s.probes > 0 { 100.0 * s.hits as f64 / s.probes as f64 } else { 0.0 };
    println!(
        "    probes {}, hits {} ({:.1}%), stores {}, replaced {}, fill after last run {:.1}%",
        group(s.probes),
        group(s.hits),
        hit_rate,
        group(s.stores),
        group(s.replaced),
        100.0 * used as f64 / slots as f64
    );
    println!(
        "    foreign entries seen {}, false hits a table with only 16 / 32 / 48 check bits would have had: {} / {} / {}\n",
        group(s.foreign),
        group(s.near_miss[0]),
        group(s.near_miss[1]),
        group(s.near_miss[2])
    );
}

/// Shared by Go and Rust so a table saved by one can be loaded by the other.
fn tt_file_path() -> PathBuf {
    std::env::temp_dir().join("mattjes-perft.tt")
}

/// Measures the persistence cycle: cold perft run, save, load into a fresh table,
/// warm run (which should only touch the root), and the rejection of a file with a
/// wrong fingerprint. The file is kept for `bitboard_perft_tt_load` (and for the
/// other language).
pub fn bitboard_perft_tt_persist(fen: &str, depth: u32, size_mb: usize) {
    let path = tt_file_path();
    let fp = bitboard::zobrist_fingerprint();
    println!("=== perft TT persistence ===\n    {}\n    depth {}, {} MB direct-mapped table, zobrist fingerprint {:016x}", fen, depth, size_mb, fp);
    let mut b = Board::from_fen(fen).expect("valid FEN");

    let mut t = tt::Table::new(size_mb);
    let start = Instant::now();
    let cold = perft_tt(&mut b, depth, &mut t);
    println!("    cold run:  {} nodes in {}, {} stores, fill {:.1}%", group(cold), fmt_ms(start.elapsed()), group(t.stats.stores), 100.0 * t.used() as f64 / t.slots() as f64);

    let start = Instant::now();
    t.save(&path, fp).expect("save");
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    println!("    save:      {} in {} -> {}", fmt_mb(size as usize), fmt_ms(start.elapsed()), path.display());

    let start = Instant::now();
    let mut t2 = tt::Table::load(&path, fp).expect("load");
    println!("    load:      {} in {}", fmt_mb(t2.bytes()), fmt_ms(start.elapsed()));

    let start = Instant::now();
    let warm = perft_tt(&mut b, depth, &mut t2);
    println!("    warm run:  {} nodes in {}, {} probes, {} hits", group(warm), fmt_ms(start.elapsed()), group(t2.stats.probes), group(t2.stats.hits));

    match tt::Table::load(&path, fp ^ 1) {
        Err(e) => println!("    wrong fingerprint rejected: {}", e),
        Ok(_) => println!("    FAIL: wrong fingerprint accepted"),
    }
    println!("    {}\n", if cold == warm { "[all ok]" } else { "[FAILURES]" });
}

/// Loads the file left by `bitboard_perft_tt_persist` (from either language) and
/// runs the warm perft; the count must match the reference suite.
pub fn bitboard_perft_tt_load(fen: &str, depth: u32) {
    let path = tt_file_path();
    println!("=== perft TT load ===\n    {} depth {} from {}", fen, depth, path.display());
    let start = Instant::now();
    let mut t = match tt::Table::load(&path, bitboard::zobrist_fingerprint()) {
        Ok(t) => t,
        Err(e) => {
            println!("    load failed: {}\n", e);
            return;
        }
    };
    println!("    load:      {} in {}", fmt_mb(t.bytes()), fmt_ms(start.elapsed()));
    let mut b = Board::from_fen(fen).expect("valid FEN");
    let start = Instant::now();
    let warm = perft_tt(&mut b, depth, &mut t);
    println!("    warm run:  {} nodes in {}, {} probes, {} hits", group(warm), fmt_ms(start.elapsed()), group(t.stats.probes), group(t.stats.hits));
    let expected = PERFT_POSITIONS.iter().find(|p| p.fen == fen).and_then(|p| p.nodes.get(depth as usize - 1)).copied();
    match expected {
        None => println!("    (no reference value)"),
        Some(e) if e == warm => println!("    [all ok]"),
        Some(e) => println!("    FAIL expected {}", group(e)),
    }
    println!();
}

/// `bitboard_unique_positions` with a `ttstore` set instead of sort+dedup: a child
/// is kept only if its 128-bit key is new. Only the distinct positions are ever
/// stored, not all children. The counts must still match OEIS A083276 (a 128-bit
/// collision would lose a position). The set has a fixed size per ply, estimated
/// from the child count; if it runs full the ply is redone with twice the slots.
pub fn bitboard_unique_positions_hashed(fen: &str, max_depth: u32, max_children: usize) {
    let root = Board::from_fen(fen).expect("valid FEN");
    let is_start = fen == START_FEN;
    println!("=== unique positions per ply, ttstore set ===\n    {}", fen);
    println!("    {:>4} {:>14} {:>12} {:>10} {:>8} {:>9} {:>9}", "ply", "children", "unique", "reference", "time", "records", "set");

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
        // The store has a fixed size, so the ply is sized up front: distinct positions
        // are about half of the children from ply 3 on. If the estimate is too small
        // (plies 1 and 2 have no transpositions at all) the ply is redone with twice
        // the slots.
        let mut slots = 16;
        while slots * MAX_LOAD_PERCENT / 100 < child_count / 2 {
            slots *= 2;
        }
        let (set, next) = loop {
            let mut set = Store::with_slots(slots);
            let mut next: Vec<Record> = Vec::with_capacity(child_count / 2);
            let mut full = false;
            'expand: for k in &level {
                let b = PackedFixed::decode(k).0;
                let n = b.gen_moves(&mut buf);
                for &m in &buf[..n] {
                    let mut child = b;
                    child.do_move(m);
                    let (is_new, ok) = set.insert(child.key);
                    if !ok {
                        full = true;
                        break 'expand;
                    }
                    if is_new {
                        next.push(position_record(&child));
                    }
                }
            }
            if !full {
                break (set, next);
            }
            println!("    {:>4} {:>14}   (set with {} slots full, retrying with twice the size)", ply, group(child_count as u64), group(slots as u64));
            slots *= 2;
        };
        level = next;
        let unique = level.len();

        let mut reference = "-".to_string();
        if is_start && ply - 1 < UNIQUE_REFERENCE.len() {
            reference = group(UNIQUE_REFERENCE[ply - 1] as u64);
            if UNIQUE_REFERENCE[ply - 1] != unique {
                reference += " FAIL";
                ok = false;
            }
        }
        println!(
            "    {:>4} {:>14} {:>12} {:>10} {:>8} {:>9} {:>9}",
            ply,
            group(child_count as u64),
            group(unique as u64),
            reference,
            fmt_ms(start.elapsed()),
            fmt_mb(unique * PackedFixed::MAX_BYTES),
            fmt_mb(set.bytes())
        );
    }
    println!("    {}\n", if ok { "[all ok]" } else { "[FAILURES]" });
}

/// Checks `ttstore` against a `HashMap` and its save/load.
pub fn bitboard_store_roundtrip() {
    fn walk(b: &mut Board, depth: u32, s: &mut Store, r: &mut HashMap<Key, u64>, fails: &mut u32) {
        if !s.put(b.key, depth as u64).1 {
            *fails += 1;
        }
        r.insert(b.key, depth as u64);
        if depth == 0 {
            return;
        }
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        let st = b.state();
        for &m in &buf[..n] {
            b.do_move(m);
            walk(b, depth - 1, s, r, fails);
            b.undo_move(m, st);
        }
    }

    println!("=== ttstore roundtrip ===");
    let path = std::env::temp_dir().join("mattjes-store.tts");
    let fp = bitboard::zobrist_fingerprint();
    let mut root = Board::from_fen(START_FEN).expect("valid FEN");
    let mut s = Store::new(4); // 262,144 slots, accepts 196,608 keys
    let mut r = HashMap::new();
    let mut fails = 0;
    walk(&mut root, 4, &mut s, &mut r, &mut fails);

    if s.len() != r.len() {
        fails += 1;
    }
    for (&k, &v) in &r {
        if s.get(k) != Some(v) {
            fails += 1;
        }
    }
    s.save(&path, fp).expect("save");
    let s2 = Store::load(&path, fp).expect("load");
    for (&k, &v) in &r {
        if s2.get(k) != Some(v) {
            fails += 1;
        }
    }
    let _ = std::fs::remove_file(&path);
    println!("    {} keys, {} slots, {}, {} value bits, {} failures", group(s.len() as u64), group(s.slots() as u64), fmt_mb(s.bytes()), s.value_bits(), fails);
    println!("    {}\n", if fails == 0 { "[all ok]" } else { "[FAILURES]" });
}
