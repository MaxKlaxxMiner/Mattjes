//! Milestone 5: endgame tables. Mirrors `mattjesGo/tests_egtb.go`.

use std::time::Instant;

use crate::bitboard::Board;
use crate::chess::{Piece, Pos, MATE_POSITIONS};
use crate::egtb::{self, Set};
use crate::tests_hash::group;

/// Checks the position indexing of every table: decoding an index and encoding
/// the squares again must give the same index, and every symmetric image of a
/// position (mirrors, transposition, color swap) must be located at the same
/// index. Four-piece tables are sampled with a stride.
pub fn egtb_index_roundtrip(stride: usize) {
    let set = Set::new();
    println!("egtb: {} materials, {} MB", set.tables.len(), set.total_size() >> 20);
    let start = Instant::now();
    for t in &set.tables {
        let step = if t.mat.pieces() > 3 { stride } else { 1 };
        let n = t.mat.pieces();
        let mut sq = vec![Pos::NONE; n];
        let mut pieces = vec![Piece::WHITE_KING, Piece::BLACK_KING];
        pieces.extend_from_slice(&t.slots);
        let (mut legal, mut checked, mut dead, mut images) = (0, 0, 0, 0);
        let symmetric = t.mat.white.len() == 1 && t.mat.black.len() == 1 && t.mat.white[0] == t.mat.black[0];
        let mut idx = 0;
        while idx < t.size {
            let white_move = t.decode(idx, &mut sq);
            let back = t.index(white_move, &sq);
            if back != idx as i64 {
                // dead index (two equal pieces in the other order, or the transposed twin): the same position, other digits
                let mut sq2 = vec![Pos::NONE; n];
                if t.decode(back as usize, &mut sq2) != white_move || !same_position(&sq, &sq2) {
                    panic!("{}: index {} decodes to {:?}, encodes to {} = {:?}", t.mat.name(), idx, sq, back, sq2);
                }
                dead += 1;
                idx += step;
                continue;
            }
            checked += 1;
            if !distinct(&sq) {
                idx += step;
                continue;
            }
            let b = Board::from_pieces(white_move, &sq, &pieces);
            if b.opponent_in_check() {
                idx += step;
                continue;
            }
            legal += 1;
            // every symmetric image must come back to this index
            let transforms = if t.mat.pawns() > 0 { 2 } else { 8 };
            for tf in 0..transforms {
                for flip in [false, true] {
                    let img = image_board(white_move, &sq, &pieces, tf, flip);
                    let located = set.locate(&img);
                    // symmetric material (KQKQ): the color-swapped image is another position of the same table
                    let same_index = !flip || !symmetric;
                    match located {
                        Some((t2, idx2)) if std::ptr::eq(t2, t) && (!same_index || idx2 == idx as i64) => images += 1,
                        _ => panic!("{}: index {} ({}), transform {} flip {} located as {:?}", t.mat.name(), idx, b.fen(), tf, flip, located.map(|(t2, i2)| (t2.mat.name(), i2))),
                    }
                }
            }
            idx += step;
        }
        println!("{:<6} {:>10} indices, {:>10} checked, {:>10} dead, {:>10} legal, {:>10} images ok", t.mat.name(), t.size, checked, dead, legal, images);
    }
    println!("roundtrip ok in {:.1} s", start.elapsed().as_secs_f64());
}

/// Both square lists hold the same kings and the same set of other squares
/// (equal pieces may be listed in either order), or the transposed set when
/// both kings are on the diagonal (the dead twin).
fn same_position(a: &[Pos], b: &[Pos]) -> bool {
    if a[0] != b[0] || a[1] != b[1] {
        return false;
    }
    let (mut sa, mut sb, mut st) = (0u64, 0u64, 0u64);
    for i in 2..a.len() {
        sa |= 1 << a[i].idx();
        sb |= 1 << b[i].idx();
        st |= 1 << Pos::from_xy(7 - b[i].y() as i32, 7 - b[i].x() as i32).idx(); // transposed
    }
    let on_diag = |p: Pos| p.x() == 7 - p.y();
    sa == sb || (sa == st && on_diag(a[0]) && on_diag(a[1]))
}

fn distinct(sq: &[Pos]) -> bool {
    let mut seen = 0u64;
    for &s in sq {
        if seen & (1 << s.idx()) != 0 {
            return false;
        }
        seen |= 1 << s.idx();
    }
    true
}

/// The position transformed by symmetry tf (1 mirror files, 2 mirror ranks, 4
/// transpose) and optionally with swapped colors (which also mirrors the ranks,
/// so pawns keep their direction).
fn image_board(white_move: bool, sq: &[Pos], pieces: &[Piece], tf: usize, flip: bool) -> Board {
    let mut sq2 = Vec::with_capacity(sq.len());
    let mut ps2 = Vec::with_capacity(pieces.len());
    for (i, &s) in sq.iter().enumerate() {
        let (mut f, mut r) = (s.x() as i32, 7 - s.y() as i32);
        if tf & 1 != 0 {
            f = 7 - f;
        }
        if tf & 2 != 0 {
            r = 7 - r;
        }
        if tf & 4 != 0 {
            std::mem::swap(&mut f, &mut r);
        }
        if flip {
            r = 7 - r;
        }
        sq2.push(Pos::from_xy(f, 7 - r));
        ps2.push(if flip { pieces[i].opponent() } else { pieces[i] });
    }
    Board::from_pieces(white_move != flip, &sq2, &ps2)
}

/// Computes the named tables (and their dependencies), compares the longest
/// mates with the literature and the test positions of `MATE_POSITIONS` with
/// the mate search results. `verbose` prints a line per level. The name "all"
/// generates every table.
pub fn egtb_generate(names: &[&str], workers: usize, verbose: bool, verify: bool) {
    let mut set = Set::new();
    let mut progress = |line: &str| {
        if verbose || !line.starts_with(' ') {
            println!("{}", line);
        }
    };
    let start = Instant::now();
    let names: Vec<String> = if names == ["all"] { set.tables.iter().map(|t| t.mat.name()).collect() } else { names.iter().map(|s| s.to_string()).collect() };
    let mut stats: Vec<(String, egtb::Stats)> = Vec::new();
    for name in &names {
        let ti = set.find(name).unwrap_or_else(|| panic!("unknown material {}", name));
        if set.tables[ti].is_generated() {
            continue; // already generated as a dependency
        }
        let st = set.generate(ti, workers, &mut progress);
        stats.push((name.clone(), st));
    }
    println!("\ngenerated in {:.1} s\n", start.elapsed().as_secs_f64());

    if verify {
        println!("forward verification (every position recomputed from its children):");
        let start = Instant::now();
        for ti in 0..set.tables.len() {
            if !set.tables[ti].is_generated() {
                continue;
            }
            let bad = set.verify(ti, workers, &|line| println!("{}", line));
            println!("  {:<5} {}", set.tables[ti].mat.name(), if bad > 0 { format!("{} MISMATCHES", bad) } else { "OK".to_string() });
        }
        println!("verified in {:.1} s\n", start.elapsed().as_secs_f64());
    }

    println!("longest mates (moves) against the literature:");
    for t in &set.tables {
        let name = t.mat.name();
        let Some((_, st)) = stats.iter().find(|(n, _)| *n == name) else { continue };
        match egtb::KNOWN_MAXIMA.iter().find(|(n, _)| *n == name) {
            Some(&(_, want)) => {
                let verdict = if st.longest_mate() == want { "OK".to_string() } else { format!("MISMATCH, expected {}", want) };
                println!("  {:<5} {:>3}  {}", name, st.longest_mate(), verdict);
            }
            None => println!("  {:<5} {:>3}  (not in the list)", name, st.longest_mate()),
        }
    }

    println!("\ntest positions:");
    for p in MATE_POSITIONS {
        let b = Board::from_fen(p.fen).expect("valid FEN");
        let Some(v) = set.lookup(&b) else { continue }; // more pieces or table not generated
        let want = 2 * p.mate_in - 1;
        let verdict = if v.is_win() && v.plies() == want { "OK".to_string() } else { format!("MISMATCH, expected win in {}", want) };
        println!("  {:<7} {:<12} {}", p.name, v.to_string(), verdict);
    }

    println!("\nchecksums against the recorded constants:");
    for t in &set.tables {
        if !t.is_generated() {
            continue;
        }
        let sum = t.raw_checksum();
        let verdict = match egtb::TABLE_CHECKSUMS.iter().find(|(n, _)| *n == t.mat.name()) {
            Some(&(_, want)) if want == sum => "OK".to_string(),
            Some(&(_, want)) => format!("MISMATCH, expected {:016x}", want),
            None => "(not recorded)".to_string(),
        };
        println!("  {:<5} {:016x}  {}", t.mat.name(), sum, verdict);
    }
}

/// The production path: the cache file next to the binary is loaded when
/// present and correct, otherwise all tables are generated, checked against the
/// checksum constants and written.
pub fn egtb_load_or_generate(workers: usize) -> Set {
    let set = Set::load_or_generate(&egtb::default_path(), workers, true, &mut |line| println!("{}", line));
    println!();
    set
}

/// Generates (or loads) tables beyond the four-piece base, one material per
/// name like "KQKBN", and reports what the docs table needs: the raw position
/// count before symmetry, the index space, generation time per level and in
/// total, the longest mate, the checksum and the file size. `verify`
/// recomputes every position from its children afterwards (one full forward
/// pass). Mirrors `egtbMeasure` in Go; Rust has no runtime memory statistics,
/// so the process memory is not printed here.
pub fn egtb_measure(names: &[&str], workers: usize, verify: bool) {
    let mut set = egtb_load_or_generate(workers);
    let mut progress = |line: &str| println!("{}", line);
    let log_path = egtb::default_cache_dir().join("measure.log");
    let log = |line: &str| {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(&log_path) {
            let _ = writeln!(f, "{}", line);
        }
    };
    for name in names {
        let m = egtb::Material::parse(name).unwrap_or_else(|e| panic!("{}", e));
        let ti = set.add_material(m.clone());
        let size = set.tables[ti].size;
        println!("=== {}: {} pieces, {} raw positions without symmetry, {} indices = {} MB ===", m.name(), m.pieces(), group(m.raw_positions()), group(size as u64), size >> 20);
        let start = Instant::now();
        let (ti, st, loaded) = set.load_or_generate_table(m.clone(), workers, &mut progress);
        let elapsed = start.elapsed();
        let t = &set.tables[ti];
        let sum = t.raw_checksum();
        let verdict = match egtb::recorded_checksum(&m.name()) {
            Some(want) if want == sum => "OK".to_string(),
            Some(want) => format!("MISMATCH, expected {:016x}", want),
            None => "(not recorded)".to_string(),
        };
        if loaded {
            println!("    loaded in {:.2} s, checksum {:016x} {}", elapsed.as_secs_f64(), sum, verdict);
        } else {
            println!(
                "    generated in {:.1} s with {} workers: {} legal, {} wins, {} losses, {} draws, longest mate {} plies = {} moves, {} levels, {} evaluations",
                st.duration.as_secs_f64(),
                workers,
                group(st.legal as u64),
                group(st.wins as u64),
                group(st.losses as u64),
                group(st.draws as u64),
                st.max_win,
                st.longest_mate(),
                st.levels,
                group(st.evaluations as u64)
            );
            if st.overflow {
                println!("    WARNING: distance range exceeded, at least {} positions beyond {} plies read as draws", st.beyond, egtb::MAX_PLIES);
            }
            if let Some((committed, working)) = egtb::peak_memory() {
                println!("    peak memory: {} committed, {} working set; table {} MB", egtb::format_bytes(committed), egtb::format_bytes(working), size >> 20);
            }
            if let Some(&(_, want)) = egtb::KNOWN_MAXIMA.iter().find(|(n, _)| *n == m.name()) {
                let verdict = if st.longest_mate() == want { "OK".to_string() } else { format!("MISMATCH, expected {}", want) };
                println!("    longest mate {} moves against the literature: {}", st.longest_mate(), verdict);
            }
            println!("    checksum {:016x} {}  (record as (\"{}\", 0x{:016x}),)", sum, verdict, m.name(), sum);
        }
        let path = set.table_path(t);
        if let Ok(info) = std::fs::metadata(&path) {
            println!("    file {}: {} MB", path.display(), info.len() >> 20);
        }
        // reference positions of this material: the table must give exactly 2N-1
        for p in MATE_POSITIONS {
            let b = Board::from_fen(p.fen).expect("valid FEN");
            match set.locate(&b) {
                Some((lt, _)) if std::ptr::eq(lt, t) => {}
                _ => continue,
            }
            let v = set.lookup(&b).unwrap_or(egtb::Value::DRAW);
            let want = 2 * p.mate_in - 1;
            let verdict = if v.is_win() && v.plies() == want { "OK".to_string() } else { format!("MISMATCH, expected win in {}", want) };
            println!("    test position {}: {}  {}", p.name, v, verdict);
        }
        let mut verified = "skipped".to_string();
        if verify {
            let start = Instant::now();
            let bad = set.verify(ti, workers, &|line| println!("{}", line));
            verified = format!("{} mismatches in {:.1} s", bad, start.elapsed().as_secs_f64());
            println!("    verify: {}", verified);
        }
        if loaded {
            log(&format!("{} pieces={} indices={} loaded checksum={:016x} {} verify={}", m.name(), m.pieces(), size, sum, verdict, verified));
        } else {
            log(&format!(
                "{} pieces={} indices={} legal={} wins={} losses={} draws={} longest_plies={} levels={} evaluations={} seconds={:.1} workers={} overflow={} beyond={} peak_commit_mb={} peak_ws_mb={} checksum={:016x} {} verify={}",
                m.name(),
                m.pieces(),
                size,
                st.legal,
                st.wins,
                st.losses,
                st.draws,
                st.max_win,
                st.levels,
                st.evaluations,
                st.duration.as_secs_f64(),
                workers,
                st.overflow,
                st.beyond,
                egtb::peak_memory().map_or(0, |p| p.0 >> 20),
                egtb::peak_memory().map_or(0, |p| p.1 >> 20),
                sum,
                verdict,
                verified
            ));
        }
        println!();
    }
}

/// Prints the names of all materials with `pieces` pieces in dependency order,
/// one per line and nothing else (for scripts).
pub fn egtb_list(pieces: usize) {
    for m in egtb::Material::all_with(pieces) {
        println!("{}", m.name());
    }
}
