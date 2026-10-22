//! The pairs packer on the WDL symbol streams dumped by the Go testbench
//! (tests/wdl/<material>.wdl): sizes, a full decode check, random probe
//! timing and a checksum of the container (identical to Go's line).

use crate::pairs;
use std::time::Instant;

/// FNV-1a over the bytes (the checksum column shared with Go).
fn fnv64(data: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &b in data {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    h
}

/// A tiny xorshift for the random probes (no crate).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Packs every dumped stream (names: semicolon separated materials, "" = all),
/// verifies every position, times random probes and prints the sizes next
/// to the Syzygy WDL file of the material.
pub fn pairs_measure(dir: &str, names: &str, opt: pairs::Options, verify: bool) {
    let mut files: Vec<String> = std::fs::read_dir(dir)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.strip_suffix(".wdl").map(|s| s.to_string())
        })
        .filter(|n| names.is_empty() || format!(";{};", names).contains(&format!(";{};", n)))
        .collect();
    files.sort();
    println!(
        "=== pairs: blocks {} bytes, index 2^{} (0 = auto), {} pairs per pass, min count {}, {} symbols, symbol length {} ===",
        opt.block_bytes, opt.index_bits, opt.pairs_per_pass, opt.min_count, opt.max_symbols, opt.max_symbol_len
    );
    println!(
        "{:<6} {:>11} {:>9} {:>9} {:>6} | {:>9} {:>5} {:>7} {:>4} {:>3} {:>4} | {:>16} | {:>6} {:>6}",
        "table", "positions", "packed", "syzygy", "ratio", "tokens", "syms", "blocks", "bits", "idx", "pass", "checksum", "pack s", "probe"
    );
    let (mut sum_packed, mut sum_syzygy) = (0u64, 0u64);
    let mut sum_pack = 0.0f64;
    let mut ok = true;
    for name in &files {
        let values = std::fs::read(format!("{}/{}.wdl", dir, name)).expect("read stream");
        let start = Instant::now();
        let (packed, st) = pairs::pack_stats(&values, 3, opt);
        let pack_time = start.elapsed().as_secs_f64();
        let t = pairs::Table::open(&packed).expect("open");
        if verify {
            let workers = opt.workers.max(1);
            let chunk = values.len().div_ceil(workers);
            let bad = std::sync::atomic::AtomicUsize::new(0);
            std::thread::scope(|s| {
                for w in 0..workers {
                    let (lo, hi) = (w * chunk, values.len().min((w + 1) * chunk));
                    let (t, values, packed, bad) = (&t, &values, &packed, &bad);
                    s.spawn(move || {
                        let mut n = 0;
                        for (i, &v) in values.iter().enumerate().take(hi).skip(lo) {
                            if t.value(packed, i) != v {
                                n += 1;
                            }
                        }
                        bad.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
                    });
                }
            });
            let bad = bad.load(std::sync::atomic::Ordering::Relaxed);
            if bad > 0 {
                println!("  {}: {} of {} positions decode wrong", name, bad, values.len());
                ok = false;
            }
        }
        let mut rng = Rng(0x9E3779B97F4A7C15);
        let probes = 1usize << 20;
        let start = Instant::now();
        let mut sink = 0usize;
        for _ in 0..probes {
            sink += t.value(&packed, (rng.next() % values.len() as u64) as usize) as usize;
        }
        let probe = start.elapsed().as_nanos() as usize / probes;
        let syzygy = syzygy_file_size(name);
        let ratio = if syzygy > 0 { format!("{:.2}", st.bytes as f64 / syzygy as f64) } else { "-".to_string() };
        assert!(sink < usize::MAX);
        println!(
            "{:<6} {:>11} {:>9} {:>9} {:>6} | {:>9} {:>5} {:>7} {:>4} {:>3} {:>4} | {:016x} | {:>6.2} {:>4} ns",
            name, st.positions, st.bytes, syzygy, ratio, st.tokens, st.symbols, st.blocks, st.max_code_len, st.index_bits, st.passes, fnv64(&packed), pack_time, probe
        );
        sum_packed += st.bytes as u64;
        sum_syzygy += syzygy;
        sum_pack += pack_time;
    }
    let verdict = if ok { "all ok" } else { "FAILURES" };
    println!("{:<6} {:>11} {:>9} {:>9} {:>6.2} | packed in {:.1} s, {}\n", "total", "", sum_packed, sum_syzygy, sum_packed as f64 / sum_syzygy.max(1) as f64, sum_pack, verdict);
}

/// The size of the Syzygy WDL file of a material in the author's folder
/// ("KQKR" -> "KQvKR.rtbw"), 0 when missing.
pub fn syzygy_file_size(name: &str) -> u64 {
    let mut file = String::from(&name[..1]);
    file.push_str(&name[1..].replacen('K', "vK", 1));
    std::fs::metadata(format!("{}/{}.rtbw", crate::tests_syzygy::SYZYGY_DEFAULT_DIR, file)).map(|m| m.len()).unwrap_or(0)
}
