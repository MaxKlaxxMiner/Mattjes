//! Milestone 7: the Syzygy WDL reader against the own tables and the
//! recorded test positions (mirrors `mattjesGo/tests_syzygy.go`).

use std::path::Path;
use std::time::Instant;

use crate::bitboard::Board;
use crate::chess::{new_buffer, Outcome, Piece, Pos, WDL_POSITIONS};
use crate::egtb::{Material, Set};
use crate::syzygy::{self, Tables};

/// The author's Syzygy folder (all three- to six-piece tables).
pub const SYZYGY_DEFAULT_DIR: &str = "C:/Users/MaxKlaxx/Desktop/spiele/chess/table";

/// A small deterministic generator (xorshift), no crate needed.
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
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Compares Syzygy WDL probes with the engine's own tables on random legal
/// positions of the named materials (semicolon separated; three- and
/// four-piece ones from the base, larger ones from the cache files) and
/// reports mismatches and the probe rate.
pub fn syzygy_verify(dir: &str, names: &str, samples: usize) {
    let start = Instant::now();
    let tb = match Tables::open(Path::new(dir)) {
        Ok(t) => t,
        Err(e) => {
            println!("syzygy: {}", e);
            return;
        }
    };
    println!("syzygy: {} WDL tables in {}, largest {} pieces, scanned in {:?}", tb.count(), dir, tb.largest(), start.elapsed());
    let set: Set = crate::tests_egtb::egtb_load_or_generate(12);
    syzygy_verify_tables(&tb, set, names, samples);
}

/// Writes our own WDL tables of the named base materials ("" = all) into
/// dir ("" = the cache folder's wdl subfolder) and prints the sizes (the
/// lines of the Go testbench; the files are byte-identical).
pub fn syzygy_own_write(names: &str, workers: usize, opt: crate::pairs::Options, bo: syzygy::write::BuildOptions, dir: &str) {
    let set = crate::tests_egtb::egtb_load_or_generate(workers);
    let start = Instant::now();
    let mut total = 0u64;
    let folder = if dir.is_empty() { set.wdl_path() } else { std::path::PathBuf::from(dir) };
    for t in set.base() {
        if !names.is_empty() && !format!(";{};", names).contains(&format!(";{};", t.mat.name())) {
            continue;
        }
        let t_start = Instant::now();
        let path = folder.join(format!("{}-{}.wdl", t.mat.pieces(), t.mat.name()));
        let b = syzygy::write::write_wdl(&set, t, &path, opt, bo.clone(), workers, None).expect("write wdl");
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        println!("{:<6} {:>4} candidates, {:>8} bytes in {} streams, file {:>8} bytes ({:.1} s)", t.mat.name(), b.candidates, b.bytes(), b.streams.len(), size, t_start.elapsed().as_secs_f64());
        total += size;
    }
    println!("written {} bytes to {} in {:.1} s\n", total, folder.display(), start.elapsed().as_secs_f64());
}

/// Builds the own WDL or DTM streams of the named materials in memory (beyond
/// the base generated exactly first) and prints size, candidates and time per
/// material, the Syzygy file as the yardstick; port of Go's `syzygyBuild`.
/// mode: "wdl", or the quantised distances as "dtm" (signed, draws stored),
/// "dtmd" (draws free), "dtmu" (bucket only, draws free: what `write_dtm`
/// writes). verbose prints the builder's lines with the seconds since the
/// build started, which separates the sampling from the finalists.
pub fn syzygy_build(names: &str, workers: usize, opt: crate::pairs::Options, bo: syzygy::write::BuildOptions, mode: &str, verbose: bool) {
    let mut set = crate::tests_egtb::egtb_load_or_generate(workers);
    println!("=== own {} tables: blocks {} bytes, {} pairs per pass, min count {}, {} symbols, symbol length {}, dict sample {}, sample target {}, finalists {} ===", mode, opt.block_bytes, opt.pairs_per_pass, opt.min_count, opt.max_symbols, opt.max_symbol_len, opt.dict_sample, bo.sample_target, bo.finalists);
    let start = Instant::now();
    let mut canonical = Vec::new();
    for name in names.split(';') {
        if let Ok(m) = crate::egtb::Material::parse(name.trim()) {
            canonical.push(m.name());
        }
    }
    let mut indices: Vec<usize> = (0..set.base().len()).collect();
    for name in &canonical {
        let m = crate::egtb::Material::parse(name).expect("parsed above");
        if m.pieces() > 4 {
            let ti = set.add_material(m.clone());
            let t_start = Instant::now();
            set.generate(ti, workers, &mut |line: &str| {
                if !line.starts_with(' ') {
                    println!("{}", line);
                }
            });
            println!("{}: generated in {:.1} s", m.name(), t_start.elapsed().as_secs_f64());
            indices.push(ti);
        }
    }
    let (mut sum_ours, mut sum_syzygy) = (0u64, 0u64);
    for ti in indices {
        let t = &set.tables[ti];
        if !names.is_empty() && !canonical.iter().any(|n| *n == t.mat.name()) {
            continue;
        }
        let t_start = Instant::now();
        let src = match mode {
            "dtm" => syzygy::write::Source::new_dtm(&set, t, crate::egtb::FILE_STEP, true, false, workers),
            "dtmd" => syzygy::write::Source::new_dtm(&set, t, crate::egtb::FILE_STEP, true, true, workers),
            "dtmu" | "dtmw" => syzygy::write::Source::new_dtm(&set, t, crate::egtb::FILE_STEP, false, true, workers),
            _ => syzygy::write::Source::new(&set, t, workers),
        };
        let size = if mode == "wdl" { crate::tests_pairs::syzygy_file_size(&t.mat.name()) } else { 0 };
        let mut tbo = bo.clone();
        let mut t_start = t_start;
        if mode == "dtmw" {
            // the DTM with the orders the WDL chose (no sampling): the WDL is built first
            let w_start = Instant::now();
            let progress = |line: &str| println!("    {:6.1} s  {}", w_start.elapsed().as_secs_f64(), line);
            let wdl = syzygy::write::build(&syzygy::write::Source::new(&set, t, workers), opt, bo.clone(), workers, if verbose { Some(&progress) } else { None });
            let orders = wdl.orders();
            println!("{:<6} WDL {:>8} bytes in {:.1} s, orders {}", t.mat.name(), wdl.bytes(), w_start.elapsed().as_secs_f64(), orders.iter().map(|side| side.iter().map(|o| o.to_string()).collect::<Vec<_>>().join(",")).collect::<Vec<_>>().join(" | "));
            tbo.orders = orders;
            t_start = Instant::now();
        }
        let b_start = Instant::now();
        let progress = |line: &str| println!("    {:6.1} s  {}", b_start.elapsed().as_secs_f64(), line);
        let b = syzygy::write::build(&src, opt, tbo, workers, if verbose { Some(&progress) } else { None });
        println!(
            "{:<6} {:>9} legal, {:>4.1}% capture-decided, {:>4} candidates: ours {:>8}, syzygy {:>8}, ratio {:.2} ({:.1} s, source {:.1} s)",
            t.mat.name(),
            src.legal,
            100.0 * src.decided as f64 / src.legal.max(1) as f64,
            b.candidates,
            b.bytes(),
            size,
            b.bytes() as f64 / size.max(1) as f64,
            t_start.elapsed().as_secs_f64(),
            (b_start - t_start).as_secs_f64()
        );
        for (i, s) in b.streams.iter().enumerate() {
            println!("       stream {}: order {:<12} {:>9} positions, {:>8} bytes: {}", i, s.order, s.size, s.packed.len(), s.stats);
        }
        sum_ours += b.bytes() as u64;
        sum_syzygy += size;
    }
    println!("total: ours {}, syzygy {}, ratio {:.2} ({:.1} s)\n", sum_ours, sum_syzygy, sum_ours as f64 / sum_syzygy.max(1) as f64, start.elapsed().as_secs_f64());
}

/// Writes the WDL and DTM files of a material (generated exactly in memory
/// beyond the base) into dir and probes random positions through the oracle
/// (an empty own set, so the files answer; the full base resolves captures)
/// against the exact values: the verdict must match and the exact distance
/// lie in the bounds.
pub fn syzygy_dtm_check(name: &str, dir: &str, samples: usize, workers: usize) {
    use crate::mateab::{Verdict, UNBOUNDED};
    let m = Material::parse(name).expect("material");
    let mut gen = crate::tests_egtb::egtb_load_or_generate(workers);
    let ti = if m.pieces() <= 4 {
        gen.find(&m.name()).expect("base material")
    } else {
        let ti = gen.add_material(m.clone());
        let st = gen.generate(ti, workers, &mut |line: &str| {
            if !line.starts_with(' ') {
                println!("{}", line);
            }
        });
        println!("{}: generated in {:.1} s", m.name(), st.duration.as_secs_f64());
        ti
    };
    let (opt, bo) = (crate::pairs::Options { workers, ..Default::default() }, syzygy::write::BuildOptions::default());
    let dir = std::path::Path::new(dir);
    let start = Instant::now();
    let wdl_path = dir.join(format!("{}-{}.wdl", m.pieces(), m.name()));
    let wb = syzygy::write::write_wdl(&gen, &gen.tables[ti], &wdl_path, opt, bo.clone(), workers, None).expect("write wdl");
    println!("{}: WDL {} bytes in {:.1} s", m.name(), wb.bytes(), start.elapsed().as_secs_f64());
    let start = Instant::now();
    let dtm_path = dir.join(format!("{}-{}.dtm", m.pieces(), m.name()));
    let db = syzygy::write::write_dtm(&gen, &gen.tables[ti], &dtm_path, opt, bo.clone(), workers, None).expect("write dtm");
    println!("{}: DTM {} bytes in {:.1} s (step {}, {} symbols)", m.name(), db.bytes(), start.elapsed().as_secs_f64(), db.step, db.alphabet);
    let mut tb = Tables::new();
    assert!(tb.add_own(dir).unwrap_or(0) > 0, "no WDL file registered");
    assert!(tb.add_dtm(dir).unwrap_or(0) > 0, "no DTM file registered");
    let empty = Set::new();
    let oracle = crate::mateab::Tables { set: &gen, wdl: Some(&tb) }; // the base resolves captures
    let probe_oracle = crate::mateab::Tables { set: &empty, wdl: Some(&tb) };
    let _ = oracle;
    let mut pieces = vec![Piece::WHITE_KING, Piece::BLACK_KING];
    pieces.extend(m.white.iter().map(|p| Piece(Piece::WHITE.0 | p.0)));
    pieces.extend(m.black.iter().map(|p| Piece(Piece::BLACK.0 | p.0)));
    let mut squares = vec![Pos(0); pieces.len()];
    let mut rng = Rng(0x2545F4914F6CDD1D);
    let (mut checked, mut bad, mut bounded) = (0usize, 0usize, 0usize);
    let t0 = Instant::now();
    while checked < samples {
        let mut used = 0u64;
        for (i, p) in pieces.iter().enumerate() {
            loop {
                let sq = rng.below(64) as usize;
                if used & (1 << sq) != 0 || ((p.0 & Piece::TYPE_MASK.0) == Piece::PAWN.0 && !(8..56).contains(&sq)) {
                    continue;
                }
                used |= 1 << sq;
                squares[i] = Pos(sq as i8);
                break;
            }
        }
        let b = Board::from_pieces(rng.below(2) == 0, &squares, &pieces);
        if b.opponent_in_check() {
            continue;
        }
        let Some(exact) = gen.lookup(&b) else { continue };
        // the probe set is empty, but the capture resolution needs the base: the
        // oracle's set is only consulted for its own tables, which the empty set lacks
        let (verdict, lo, hi) = probe_with_base(&probe_oracle, &gen, &b);
        checked += 1;
        let want = if exact.is_win() {
            Verdict::Win
        } else if exact.is_loss() {
            Verdict::Loss
        } else {
            Verdict::Draw
        };
        let plies = exact.plies();
        if verdict != want || (want != Verdict::Draw && (plies < lo || plies > hi)) {
            bad += 1;
            if bad <= 5 {
                println!("  MISMATCH {}: exact {:?}, oracle {:?} [{}, {}]", b.fen(), exact, verdict, lo, hi);
            }
        }
        if want != Verdict::Draw && hi < UNBOUNDED {
            bounded += 1;
        }
    }
    let el = t0.elapsed();
    let verdict = if bad > 0 { format!("{} MISMATCHES", bad) } else { "ok".to_string() };
    println!("{}: {} positions checked, {}, {} wins and losses with DTM bounds, {:.0} probes/s\n", m.name(), checked, verdict, bounded, checked as f64 / el.as_secs_f64());
}

/// The oracle probe with the captures resolved by the full base (the
/// oracle's own set stays empty so that the files answer).
fn probe_with_base(oracle: &crate::mateab::Tables, base: &Set, b: &Board) -> (crate::mateab::Verdict, u32, u32) {
    use crate::mateab::{Oracle, Verdict, UNBOUNDED};
    let Some(wdl) = oracle.wdl else { return oracle.probe(b) };
    match wdl.probe_wdl_with(b, Some(base)) {
        Some(v) if v >= syzygy::CURSED_WIN => {
            let (lo, hi) = dtm_bounds_of(wdl, b, true);
            (Verdict::Win, lo, hi)
        }
        Some(v) if v <= syzygy::BLESSED_LOSS => {
            let (lo, hi) = dtm_bounds_of(wdl, b, false);
            (Verdict::Loss, lo, hi)
        }
        Some(_) => (Verdict::Draw, 0, 0),
        None => (Verdict::Unknown, 0, UNBOUNDED),
    }
}

fn dtm_bounds_of(wdl: &Tables, b: &Board, win: bool) -> (u32, u32) {
    let Some((bucket, step)) = wdl.probe_dtm(b) else { return (1, crate::mateab::UNBOUNDED) };
    let v = crate::egtb::Value::of_moves(win, (bucket as u32 * step).min(127));
    v.bounds(step)
}

/// Round-trips random indices of the candidate orders of the base materials
/// ("" = all) through the Syzygy index decoder and the encoder.
pub fn syzygy_decode_check(names: &str, max_orders: usize, samples: usize) {
    let start = Instant::now();
    let mut total_bad = 0;
    for m in Material::all() {
        if !names.is_empty() && !format!(";{};", names).contains(&format!(";{};", m.name())) {
            continue;
        }
        let (checked, bad) = syzygy::decode::check_decoder(&m, max_orders, samples, 0x9E3779B97F4A7C15, &|line| println!("{}", line));
        let verdict = if bad > 0 { format!("{} MISMATCHES", bad) } else { "ok".to_string() };
        println!("{:<6} {:>8} indices round-tripped, {}", m.name(), checked, verdict);
        total_bad += bad;
    }
    if total_bad == 0 {
        println!("--- decoder: every index round-trips, {:?}\n", start.elapsed());
    } else {
        println!("--- decoder: {} MISMATCHES\n", total_bad);
    }
}

/// Probes our own WDL tables in the cache folder through the Syzygy prober
/// against the base tables ("" = every material with a file).
pub fn syzygy_own_verify(names: &str, samples: usize) {
    let set = crate::tests_egtb::egtb_load_or_generate(12);
    let mut tb = Tables::new();
    let n = match tb.add_own(&set.wdl_path()) {
        Ok(n) => n,
        Err(e) => {
            println!("own wdl: {}", e);
            return;
        }
    };
    println!("own wdl: {} tables in {}", n, set.wdl_path().display());
    let names = if names.is_empty() {
        set.base().iter().filter(|t| set.wdl_file(&t.mat).exists()).map(|t| t.mat.name()).collect::<Vec<_>>().join(";")
    } else {
        names.to_string()
    };
    syzygy_verify_tables(&tb, set, &names, samples);
}

/// Compares the WDL tables with the own tables of the named materials on
/// random positions (the base answers captures into materials without a file).
fn syzygy_verify_tables(tb: &Tables, mut set: Set, names: &str, samples: usize) {
    let start = Instant::now();
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let mut total_mismatch = 0;
    for name in names.split(';') {
        let name = name.trim();
        let m = match Material::parse(name) {
            Ok(m) => m,
            Err(e) => {
                println!("{}: {}", name, e);
                continue;
            }
        };
        if m.pieces() > 4 && set.find(&m.name()).is_none() {
            let ti = set.add_material(m.clone());
            let path = set.table_path(&set.tables[ti]);
            if let Err(e) = set.load_table(ti, &path) {
                println!("{}: own table not loaded ({}), skipped", name, e);
                continue;
            }
        }
        let mut pieces = vec![Piece::WHITE_KING, Piece::BLACK_KING];
        pieces.extend(m.white.iter().map(|p| Piece(Piece::WHITE.0 | p.0)));
        pieces.extend(m.black.iter().map(|p| Piece(Piece::BLACK.0 | p.0)));
        let mut squares = vec![Pos(0); pieces.len()];
        let (mut mismatch, mut checked, mut no_table) = (0usize, 0usize, 0usize);
        let probes_before = tb.probes.load(std::sync::atomic::Ordering::Relaxed);
        let t0 = Instant::now();
        while checked < samples {
            let mut used = 0u64;
            for (i, p) in pieces.iter().enumerate() {
                loop {
                    let sq = rng.below(64) as usize;
                    if used & (1 << sq) != 0 {
                        continue;
                    }
                    if (p.0 & Piece::TYPE_MASK.0) == Piece::PAWN.0 && !(8..56).contains(&sq) {
                        continue;
                    }
                    used |= 1 << sq;
                    squares[i] = Pos(sq as i8);
                    break;
                }
            }
            let white_move = rng.below(2) == 0;
            let b = Board::from_pieces(white_move, &squares, &pieces);
            if b.opponent_in_check() {
                continue;
            }
            let Some(own) = set.lookup(&b) else { continue };
            let Some(v) = tb.probe_wdl_with(&b, Some(&set)) else {
                no_table += 1;
                if no_table > 10 {
                    break;
                }
                continue;
            };
            checked += 1;
            let own_class = if own.is_win() { 1 } else if own.is_loss() { -1 } else { 0 };
            let syz_class = if v >= syzygy::CURSED_WIN { 1 } else if v <= syzygy::BLESSED_LOSS { -1 } else { 0 };
            if own_class != syz_class {
                mismatch += 1;
                if mismatch <= 5 {
                    println!("  MISMATCH {}: own {:?} ({}), syzygy {}", b.fen(), own, own_class, v);
                }
            }
        }
        let probes = tb.probes.load(std::sync::atomic::Ordering::Relaxed) - probes_before;
        let el = t0.elapsed();
        let status = if no_table > 10 {
            "no Syzygy table".to_string()
        } else if mismatch > 0 {
            format!("{} MISMATCHES", mismatch)
        } else {
            "ok".to_string()
        };
        println!("{:<8} {:>7} positions checked, {}, {} probes (with captures) in {:?}, {:.0} probes/s", name, checked, status, probes, el, probes as f64 / el.as_secs_f64());
        total_mismatch += mismatch;
    }
    if total_mismatch == 0 {
        println!("--- syzygy: all materials agree with the own tables, {:?}", start.elapsed());
    } else {
        println!("--- syzygy: {} MISMATCHES", total_mismatch);
    }
}

/// Prints the WDL probe of each FEN (semicolon separated) and the oracle's
/// verdict with the four-piece base on top.
pub fn syzygy_probe(dir: &str, fens: &str) {
    let tb = match Tables::open(Path::new(dir)) {
        Ok(t) => t,
        Err(e) => {
            println!("syzygy: {}", e);
            return;
        }
    };
    let set = crate::tests_egtb::egtb_load_or_generate(12);
    let oracle = crate::mateab::Tables { set: &set, wdl: Some(&tb) };
    for fen in fens.split(';') {
        let fen = fen.trim();
        let b = match Board::from_fen(fen) {
            Ok(b) => b,
            Err(e) => {
                println!("{}: {:?}", fen, e);
                continue;
            }
        };
        let probe = tb.probe_wdl(&b);
        let (verdict, lo, hi) = crate::mateab::Oracle::probe(&oracle, &b);
        println!("{:<50} has {:<5} probe {:?} oracle {:?} {}..{}", fen, tb.has(&b), probe, verdict, lo, hi);
    }
}

/// Probes every root move of the WDL test positions and compares the
/// win/draw/loss counts with the recorded ones.
pub fn syzygy_positions(dir: &str) {
    let tb = match Tables::open(Path::new(dir)) {
        Ok(t) => t,
        Err(e) => {
            println!("syzygy: {}", e);
            return;
        }
    };
    let mut fails = 0;
    for p in WDL_POSITIONS {
        let b = Board::from_fen(p.fen).expect("test FEN");
        if !tb.has(&b) {
            println!("{:<32} no table", p.name);
            continue;
        }
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        let (mut wins, mut draws, mut losses) = (0, 0, 0);
        for &m in &buf[..n] {
            let mut c = b;
            c.do_move(m);
            match tb.probe_wdl(&c) {
                Some(v) if v <= syzygy::BLESSED_LOSS => wins += 1,
                Some(v) if v >= syzygy::CURSED_WIN => losses += 1,
                Some(_) => draws += 1,
                None => {
                    println!("{:<32} move {}: no table", p.name, m.uci());
                    break;
                }
            }
        }
        let ok = wins == p.wins && draws == p.draws && losses == p.losses;
        let status = if ok { "ok".to_string() } else { format!("MISMATCH, expected {}/{}/{}", p.wins, p.draws, p.losses) };
        if !ok {
            fails += 1;
        }
        let _ = Outcome::Draw;
        println!("{:<32} {:>2}/{:>2}/{:>2} {}", p.name, wins, draws, losses, status);
    }
    if fails == 0 {
        println!("--- syzygy: every test position agrees with the recorded move counts");
    } else {
        println!("--- syzygy: {} MISMATCHES", fails);
    }
}
