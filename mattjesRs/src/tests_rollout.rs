//! Rollouts with Patricia's network as the policy (module rollout), mirrors
//! `mattjesGo/tests_rollout.go`.

use std::path::Path;
use std::time::Instant;

use crate::bitboard::Board;
use crate::chess::{new_buffer, Outcome as ChessOutcome, WdlPosition, WDL_POSITIONS};
use crate::egtb::Set;
use crate::mateab::Tables;
use crate::rollout::{self, Options, Outcome, Reason};
use crate::tests_hash::fmt_ms;
use crate::tests_mate::pv_string;
use crate::tests_syzygy::SYZYGY_DEFAULT_DIR;

/// The oracle of the test bench options: t = the four-piece tables, s =
/// Syzygy WDL on top, else dead material only (the set and the Syzygy
/// tables are returned to outlive the oracle that borrows them).
fn rollout_oracle(options: &str) -> (Option<Set>, Option<crate::syzygy::Tables>) {
    let has = |o: char| options.contains(o);
    if !has('t') && !has('s') {
        return (None, None);
    }
    let set = crate::tests_egtb::egtb_load_or_generate(12);
    let syzygy = if has('s') {
        match crate::syzygy::Tables::open(Path::new(SYZYGY_DEFAULT_DIR)) {
            Ok(t) => Some(t),
            Err(e) => {
                println!("syzygy: {}", e);
                None
            }
        }
    } else {
        None
    };
    (Some(set), syzygy)
}

fn selected(names: &str, p: &WdlPosition) -> bool {
    match names {
        "" => true,
        "wins" => p.result != ChessOutcome::Draw,
        "draws" => p.result == ChessOutcome::Draw,
        _ => names.split(';').any(|n| n.trim() == p.name),
    }
}

/// Plays every root move of the selected WDL test positions out and
/// compares the outcomes with the recorded win/draw/loss counts: the
/// quality of the rollout as a guess. Options: t/s the oracle, v the line
/// of every root move.
pub fn rollout_positions(names: &str, options: &str, max_plies: usize) {
    let has = |o: char| options.contains(o);
    let (set, syzygy) = rollout_oracle(options);
    let oracle = set.as_ref().map(|s| Tables { set: s, wdl: syzygy.as_ref() });
    let opt = Options { max_plies, oracle };
    let oracle_name = if has('s') {
        "four-piece tables and Syzygy WDL"
    } else if has('t') {
        "four-piece tables"
    } else {
        "dead material only"
    };
    println!("=== rollouts, Patricia's net as the policy of both sides, {}, at most {} plies ===", oracle_name, max_plies);
    let (mut total_moves, mut total_exact) = (0, 0);
    let mut total_time = std::time::Duration::ZERO;
    for (i, p) in WDL_POSITIONS.iter().enumerate() {
        if !selected(names, p) {
            continue;
        }
        let b = Board::from_fen(p.fen).expect("test FEN");
        let start = Instant::now();
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        let mut count = [0usize; 4]; // by Outcome for the root's side
        let (mut table_hits, mut plies, mut evals) = (0, 0, 0);
        for &m in &buf[..n] {
            let mut c = b;
            c.do_move(m);
            let r = rollout::play(&c, opt);
            // the outcome is for the opponent (to move after m): invert it
            let o = match r.outcome {
                Outcome::Win => Outcome::Loss,
                Outcome::Loss => Outcome::Win,
                o => o,
            };
            count[o as usize] += 1;
            if r.reason == Reason::Table && r.line.is_empty() {
                table_hits += 1;
            }
            plies += r.line.len();
            evals += r.evals;
            if has('v') {
                println!("      {:<6} {:<7} {:<10} {:>3} plies  {}", m.uci(), o, r.reason, r.line.len(), pv_string(&r.line));
            }
        }
        let elapsed = start.elapsed();
        total_time += elapsed;
        let exact = count[Outcome::Win as usize].min(p.wins as usize) + count[Outcome::Draw as usize].min(p.draws as usize) + count[Outcome::Loss as usize].min(p.losses as usize);
        total_moves += n;
        total_exact += exact;
        println!(
            "[{}] {:<34} guessed {:>2}/{:>2}/{:>2} ({} unknown) expected {:>2}/{:>2}/{:>2}, at most {:>2} of {:>2} right, {:>2} table hits at once, {:>5.1} plies avg, {:>7} evals, {}",
            i + 1,
            p.name,
            count[Outcome::Win as usize],
            count[Outcome::Draw as usize],
            count[Outcome::Loss as usize],
            count[Outcome::Unknown as usize],
            p.wins,
            p.draws,
            p.losses,
            exact,
            n,
            table_hits,
            plies as f64 / n.max(1) as f64,
            evals,
            fmt_ms(elapsed)
        );
    }
    println!("--- rollouts: at most {} of {} root moves guessed right, {}\n", total_exact, total_moves, fmt_ms(total_time));
}

/// Plays one FEN out and prints the line.
pub fn rollout_line(fen: &str, options: &str, max_plies: usize) {
    let b = match Board::from_fen(fen) {
        Ok(b) => b,
        Err(e) => {
            println!("{}", e);
            return;
        }
    };
    let (set, syzygy) = rollout_oracle(options);
    let oracle = set.as_ref().map(|s| Tables { set: s, wdl: syzygy.as_ref() });
    let start = Instant::now();
    let r = rollout::play(&b, Options { max_plies, oracle });
    println!("{}\n  {} by {} after {} plies, {} evals, {}\n  {}", fen, r.outcome, r.reason, r.line.len(), r.evals, fmt_ms(start.elapsed()), pv_string(&r.line));
}
