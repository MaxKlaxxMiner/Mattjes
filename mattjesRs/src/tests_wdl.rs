//! Milestone 7: WDL search. Mirrors `mattjesGo/tests_wdl.go`, the output is
//! line-identical except for times.

use std::time::{Duration, Instant};

use crate::bitboard::Board;
use crate::chess::{Outcome, WdlPosition, WDL_POSITIONS};
use crate::mateab::{self, Oracle};
use crate::tests_egtb;
use crate::tests_hash::{fmt_ms, group};
use crate::tests_mate::pv_string;
use crate::tt::{self, TransTable};
use crate::wdlpn::{self, Codec};

/// Runs the WDL test positions: first "the side to move wins" (root =
/// attacker), if that is disproven "the opponent wins" (root = defender);
/// both disproven = draw. Tables are off unless asked for.
///
/// `names` selects positions by name (semicolon separated, "" = all, "wins"
/// and "draws" the two groups). Options as in matepn: m = mobility
/// initialisation, e/E/x/X = epsilon 1/8, 1/2, 1, 2, b = bucket table with
/// priority classes, T = the four-piece base as oracle (otherwise only dead
/// material).
///
/// The search runs on a thread with a large stack: every recursion level
/// holds the children with their dependency sets (about 18 KB), and the ply
/// cap is 255.
pub fn wdlpn_solve(names: &str, size_mb: usize, codec: &str, options: &str) {
    let (names, codec, options) = (names.to_string(), codec.to_string(), options.to_string());
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || run_wdlpn(&names, size_mb, &codec, &options))
        .expect("search thread")
        .join()
        .expect("search thread panicked");
}

fn run_wdlpn(names: &str, size_mb: usize, codec: &str, options: &str) {
    let selected = |p: &WdlPosition| -> bool {
        match names {
            "" => true,
            "wins" => p.result != Outcome::Draw,
            "draws" => p.result == Outcome::Draw,
            _ => names.split(';').any(|n| n.trim() == p.name),
        }
    };
    let set = if options.contains('T') { Some(tests_egtb::egtb_load_or_generate(12)) } else { None };
    if options.contains('b') {
        let mut table = tt::Buckets::new(size_mb);
        table.set_priority_bits(wdlpn::PRIORITY_BITS);
        run_with_codec(table, size_mb, codec, set.as_ref(), &selected, options);
    } else {
        run_with_codec(tt::Table::new(size_mb), size_mb, codec, set.as_ref(), &selected, options);
    }
}

fn run_with_codec<T: TransTable + 'static>(table: T, size_mb: usize, codec: &str, set: Option<&crate::egtb::Set>, selected: &dyn Fn(&WdlPosition) -> bool, options: &str) {
    let bits = wdlpn::codec_bits(&table);
    match (codec, set) {
        ("sat", None) => run(table, size_mb, wdlpn::Saturating { bits }, mateab::Material, selected, options),
        ("float", None) => run(table, size_mb, wdlpn::Float { bits }, mateab::Material, selected, options),
        ("sat", Some(set)) => run(table, size_mb, wdlpn::Saturating { bits }, mateab::Tables { set, wdl: None }, selected, options),
        ("float", Some(set)) => run(table, size_mb, wdlpn::Float { bits }, mateab::Tables { set, wdl: None }, selected, options),
        _ => panic!("codec sat or float"),
    }
}

fn run<T: TransTable + 'static, C: Codec, O: Oracle + Copy>(table: T, size_mb: usize, codec: C, oracle: O, selected: &dyn Fn(&WdlPosition) -> bool, options: &str) {
    let has = |o: char| options.contains(o);
    let mobility = has('m');
    let mut epsilon = 0;
    for (letter, eps) in [('e', 1), ('E', 4), ('x', 8), ('X', 16)] {
        if has(letter) {
            epsilon = eps;
        }
    }
    println!(
        "=== wdlpn, {} {} MB ({} value bits, {} for the codec), codec {}, mobility {}, epsilon {}/8, {} ===",
        if has('b') { "bucket TT with priority replacement" } else { "direct-mapped TT" },
        size_mb,
        table.value_bits(),
        wdlpn::codec_bits(&table),
        codec.name(),
        mobility,
        epsilon,
        if has('T') { "four-piece tables as oracle" } else { "dead material only" }
    );
    let mut ok = true;
    let mut total_nodes = 0u64;
    let mut total_stores = 0u64;
    let mut total_time = Duration::ZERO;
    let mut table = Some(table);
    for (i, p) in WDL_POSITIONS.iter().enumerate() {
        if !selected(p) {
            continue;
        }
        println!("[{}] {}  {}  expected {} ({}/{}/{})", i + 1, p.name, p.fen, p.result, p.wins, p.draws, p.losses);
        let b = Board::from_fen(p.fen).expect("valid FEN");
        let mut t = table.take().unwrap();
        t.clear();
        t.reset_stats();
        let mut s = wdlpn::Searcher::new(oracle, t, codec, mobility);
        s.epsilon = epsilon;
        let start = Instant::now();
        let q_start = std::rc::Rc::new(std::cell::Cell::new(start));
        let qs = q_start.clone();
        s.progress = Some(Box::new(move |nodes, t: &T, c| {
            let st = t.stats();
            println!(
                "              ... {} nodes, {}, {} leaves, {} hits, {}/{} cond hits/misses ({} kept), {} capped, tt {} stores, {} replaced, {} used",
                group(nodes),
                fmt_ms(qs.get().elapsed()),
                group(c.leaves),
                group(c.tt_hits),
                group(c.cond_hits),
                group(c.cond_miss),
                group(c.cond_size),
                group(c.capped),
                group(st.stores),
                group(st.replaced),
                group(t.used() as u64)
            );
        }));
        let mut nodes = 0u64;
        let mut got: Option<Outcome> = Some(Outcome::Draw); // None = undecided
        let mut pv = Vec::new();
        for (q, attacks) in [true, false].into_iter().enumerate() {
            let question = if attacks { "side to move wins" } else { "opponent wins" };
            q_start.set(Instant::now());
            let r = s.solve(&b, attacks);
            nodes += r.nodes;
            let verdict = if r.aborted {
                "aborted".to_string()
            } else if r.proven {
                "proven".to_string()
            } else if r.disproven {
                "disproven".to_string()
            } else {
                format!("undecided (pn {}, dn {})", r.pn, r.dn)
            };
            println!(
                "    q{} {:<18} {:<10} {:>12} nodes, {} leaves, {} tt hits, {}/{} conditional hits/misses ({} kept), {} capped, {}",
                q + 1,
                format!("{}:", question),
                verdict,
                group(r.nodes),
                group(r.leaves),
                group(r.tt_hits),
                group(r.cond_hits),
                group(r.cond_miss),
                group(r.cond_size as u64),
                group(r.capped),
                fmt_ms(q_start.get().elapsed())
            );
            if r.proven {
                got = Some(if attacks { Outcome::Win } else { Outcome::Loss });
                pv = r.pv;
                break;
            }
            if !r.disproven {
                got = None;
                break;
            }
        }
        let elapsed = start.elapsed();
        let t = s.table();
        let st = t.stats();
        total_nodes += nodes;
        total_stores += st.stores;
        total_time += elapsed;
        let verdict = if got == Some(p.result) {
            "ok"
        } else {
            ok = false;
            "FAIL"
        };
        let result = match got {
            Some(o) => o.to_string(),
            None => "undecided".to_string(),
        };
        println!(
            "    result {} ({}): {} nodes in {}, {} stores, {} replaced, {} dropped, {} entries used ({:.1}%)",
            result,
            verdict,
            group(nodes),
            fmt_ms(elapsed),
            group(st.stores),
            group(st.replaced),
            group(st.dropped),
            group(t.used() as u64),
            100.0 * t.used() as f64 / t.slots() as f64
        );
        if !pv.is_empty() {
            println!("    line ({} plies): {}", pv.len(), pv_string(&pv));
        }
        table = Some(s.into_table());
    }
    print!("--- total: {} nodes, {} stores in {}", group(total_nodes), group(total_stores), fmt_ms(total_time));
    println!("{}\n", if ok { "  [all ok]" } else { "  [FAILURES]" });
}
