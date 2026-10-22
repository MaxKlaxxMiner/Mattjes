//! Milestone 7, step 2: the WDL search on the explicit graph. Mirrors
//! `mattjesGo/tests_wdllist.go`, the output is line-identical except for
//! times.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::bitboard::Board;
use crate::chess::{Outcome, WdlPosition, WDL_POSITIONS};
use crate::tests_hash::{fmt_ms, group};
use crate::tests_mate::pv_string;
use crate::wdllist::{self, Options};

/// Runs the WDL test positions with the explicit graph: first "the side to
/// move wins", then "the opponent wins"; both disproven = draw. `names` as
/// in wdlpn_solve. Options: B = brute force, p = the proof-number descent
/// alone, e/E/x/X = epsilon 1/8, 1/2, 1, 2, w = stored numbers almost
/// unclipped, l = numbers on the log scale, C = the certificate builder
/// without the perpetual-check chase, P = the certificate builder alone,
/// t = the four-piece tables as oracle, s = Syzygy WDL on top, v = progress
/// lines.
pub fn wdllist_solve(names: &str, options: &str) {
    let has = |o: char| options.contains(o);
    let selected = |p: &WdlPosition| -> bool {
        match names {
            "" => true,
            "wins" => p.result != Outcome::Draw,
            "draws" => p.result == Outcome::Draw,
            _ => names.split(';').any(|n| n.trim() == p.name),
        }
    };
    // t = the four-piece tables as oracle: every capture into them is a
    // terminal, which the draw certificates of five-piece positions need;
    // s = the Syzygy WDL tables on top (the author's folder)
    let set = if has('t') || has('s') { Some(crate::tests_egtb::egtb_load_or_generate(12)) } else { None };
    let syzygy = if has('s') {
        match crate::syzygy::Tables::open(std::path::Path::new(crate::tests_syzygy::SYZYGY_DEFAULT_DIR)) {
            Ok(tb) => Some(tb),
            Err(e) => {
                println!("syzygy: {}", e);
                None
            }
        }
    } else {
        None
    };
    let mut opt = Options { brute_force: has('B'), no_race: has('p'), no_chase: has('C'), no_descent: has('P'), max_positions: 16 << 20, oracle: set.as_ref().map(|s| crate::mateab::Tables { set: s, wdl: syzygy.as_ref() }), ..Default::default() };
    for (letter, eps) in [('e', 1), ('E', 4), ('x', 8), ('X', 16)] {
        if has(letter) {
            opt.epsilon = eps;
        }
    }
    if has('v') {
        opt.progress = Some(Arc::new(|line: &str| println!("        {}", line)));
    }
    let mut cap_name = format!("linear, stored numbers clipped at {}", wdllist::DEFAULT_NUMBER_CAP);
    if has('w') {
        opt.number_cap = wdllist::WIDE_NUMBER_CAP;
        cap_name = "linear, wide".to_string();
    }
    if has('l') {
        opt.log_numbers = true;
        cap_name = "log scale".to_string();
    }
    let mode = if opt.brute_force {
        "brute force (reachable graph)".to_string()
    } else if opt.no_race {
        format!("proof-number descent alone, epsilon {}/8, numbers {}", opt.epsilon, cap_name)
    } else if opt.no_descent {
        "certificate builder alone".to_string()
    } else {
        format!("proof-number descent racing the certificate builder, epsilon {}/8, numbers {}", opt.epsilon, cap_name)
    };
    let mode = if opt.no_chase && !opt.no_race { format!("{}, no chase", mode) } else { mode };
    let mode = if has('s') {
        format!("{}, four-piece tables and Syzygy WDL as oracle", mode)
    } else if has('t') {
        format!("{}, four-piece tables as oracle", mode)
    } else {
        mode
    };
    println!("=== wdllist, {} ===", mode);
    let mut ok = true;
    let (mut total_positions, mut total_expansions) = (0usize, 0usize);
    let mut total_time = Duration::ZERO;
    for (i, p) in WDL_POSITIONS.iter().enumerate() {
        if !selected(p) {
            continue;
        }
        println!("[{}] {}  {}  expected {} ({}/{}/{})", i + 1, p.name, p.fen, p.result, p.wins, p.draws, p.losses);
        let b = Board::from_fen(p.fen).expect("valid FEN");
        let start = Instant::now();
        let mut got: Option<Outcome> = Some(Outcome::Draw);
        let mut pv = Vec::new();
        let (mut positions, mut expansions, mut bytes) = (0usize, 0usize, 0usize);
        for (q, attacks) in [true, false].into_iter().enumerate() {
            let question = if attacks { "side to move wins" } else { "opponent wins" };
            let q_start = Instant::now();
            let r = wdllist::solve(&b, attacks, opt.clone());
            positions += r.positions;
            expansions += r.expansions;
            bytes += r.bytes;
            let verdict = if r.stuck {
                "STUCK (bug)".to_string()
            } else if r.aborted {
                "aborted".to_string()
            } else {
                match r.outcome {
                    wdllist::Outcome::Proven => format!("proven (win in {} plies)", r.win_plies),
                    wdllist::Outcome::Disproven => format!("disproven ({} safe)", r.safe),
                    wdllist::Outcome::Undecided => "undecided".to_string(),
                }
            };
            let visits = if r.method == "certificate" { format!("{} chase nodes", group(r.chase_nodes)) } else { format!("{} visits", group(r.visits)) };
            println!(
                "    q{} {:<18} {:<28} by {:<11} {:>10} positions, {:>10} expansions, {:>22}, {} won, {} fixed points, {} resets, {} exhausts, {}",
                q + 1,
                format!("{}:", question),
                verdict,
                r.method,
                group(r.positions as u64),
                group(r.expansions as u64),
                visits,
                group(r.won as u64),
                r.safe_checks,
                r.resets,
                r.exhausts,
                fmt_ms(q_start.elapsed())
            );
            if r.outcome == wdllist::Outcome::Proven {
                got = Some(if attacks { Outcome::Win } else { Outcome::Loss });
                pv = r.pv;
                break;
            }
            if r.outcome != wdllist::Outcome::Disproven {
                got = None;
                break;
            }
        }
        let elapsed = start.elapsed();
        total_positions += positions;
        total_expansions += expansions;
        total_time += elapsed;
        let verdict = if got == Some(p.result) {
            "ok"
        } else {
            ok = false;
            "FAIL"
        };
        let result = got.map_or("undecided".to_string(), |o| o.to_string());
        println!("    result {} ({}): {} positions, {} expansions in {}, graphs {} MB ({} bytes per position)", result, verdict, group(positions as u64), group(expansions as u64), fmt_ms(elapsed), bytes >> 20, bytes / positions.max(1));
        if !pv.is_empty() {
            println!("    line ({} plies): {}", pv.len(), pv_string(&pv));
        }
    }
    print!("--- total: {} positions, {} expansions in {}", group(total_positions as u64), group(total_expansions as u64), fmt_ms(total_time));
    println!("{}", if ok { "  [all ok]" } else { "  [FAILURES]" });
    if let Some((committed, working_set)) = crate::egtb::peak_memory() {
        // the bytes per graph position (docs/m7-wdl-search.md): one
        // position with the descent alone, so the graph is the process
        println!("    peak memory: {} MB committed, {} MB working set", committed >> 20, working_set >> 20);
    }
    println!();
}
