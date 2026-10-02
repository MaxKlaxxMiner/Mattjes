//! Generator-independent perft verification with speed and memory numbers.
//! Generators plug in via a closure that takes a FEN. Mirrors `mattjesGo/perft`.

mod alloc_stats;

use std::time::{Duration, Instant};

use crate::chess::PERFT_POSITIONS;

/// Verifies `f` against all reference positions, skipping depths whose expected
/// node count exceeds `max_nodes`. Returns false if any count was wrong.
pub fn run(title: &str, f: impl Fn(&str, u32) -> Result<u64, String>, max_nodes: u64) -> bool {
    run_prepared(title, || {}, f, max_nodes)
}

/// `run` with a hook that runs before every (position, depth) outside the measured
/// time, e.g. to clear a transposition table (a 1 GB memset would otherwise show
/// up as perft time).
pub fn run_prepared(title: &str, prepare: impl Fn(), f: impl Fn(&str, u32) -> Result<u64, String>, max_nodes: u64) -> bool {
    println!("=== {} ===", title);
    let mut ok = true;
    let mut total_nodes = 0u64;
    let mut total_time = Duration::ZERO;

    for (i, p) in PERFT_POSITIONS.iter().enumerate() {
        println!("[{}] {}\n    {}", i + 1, p.name, p.fen);
        for (depth, &expected) in (1u32..).zip(p.nodes) {
            if expected > max_nodes {
                break;
            }
            prepare();

            let alloc_before = alloc_stats::total_allocated();
            let start = Instant::now();
            let result = f(p.fen, depth);
            let elapsed = start.elapsed();
            let allocated = alloc_stats::total_allocated() - alloc_before;

            let nodes = match result {
                Ok(n) => n,
                Err(e) => {
                    println!("    depth {}: skipped ({})", depth, e);
                    break;
                }
            };
            total_nodes += nodes;
            total_time += elapsed;

            let status = if nodes == expected {
                "ok".to_string()
            } else {
                ok = false;
                format!("FAIL expected {}", group(expected))
            };
            println!(
                "    depth {}: {:>16} {:<28} {:>10} {:>8}  alloc {}",
                depth,
                group(nodes),
                status,
                fmt_duration(elapsed),
                mnps(nodes, elapsed),
                fmt_bytes(allocated)
            );
        }
    }

    print!("--- total: {} nodes in {} = {}", group(total_nodes), fmt_duration(total_time), mnps(total_nodes, total_time));
    println!("{}\n", if ok { "  [all ok]" } else { "  [FAILURES]" });
    ok
}

/// Formats an integer with thousands separators.
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

fn mnps(nodes: u64, d: Duration) -> String {
    if d < Duration::from_millis(1) {
        return String::new();
    }
    format!("{:.1} Mn/s", nodes as f64 / d.as_secs_f64() / 1e6)
}

fn fmt_duration(d: Duration) -> String {
    if d < Duration::from_millis(1) {
        format!("{} µs", d.as_micros())
    } else if d < Duration::from_secs(1) {
        format!("{:.1} ms", d.as_micros() as f64 / 1000.0)
    } else {
        format!("{:.2} s", d.as_secs_f64())
    }
}

fn fmt_bytes(n: u64) -> String {
    if n < 1 << 10 {
        format!("{} B", n)
    } else if n < 1 << 20 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else if n < 1 << 30 {
        format!("{:.1} MB", n as f64 / (1u64 << 20) as f64)
    } else {
        format!("{:.2} GB", n as f64 / (1u64 << 30) as f64)
    }
}
