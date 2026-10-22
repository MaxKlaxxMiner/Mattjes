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
    let mut set: Set = crate::tests_egtb::egtb_load_or_generate(12);
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
            let Some(v) = tb.probe_wdl(&b) else {
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
