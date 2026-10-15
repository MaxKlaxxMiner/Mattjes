//! Milestone 4, step 4: df-pn. Mirrors `mattjesGo/tests_pn.go`.

use std::time::{Duration, Instant};

use crate::bitboard::Board;
use crate::chess::{MatePosition, MATE_POSITIONS};
use crate::egtb;
use crate::mateab::{self, Oracle};
use crate::matepn::{self, Codec};
use crate::tests_egtb;
use crate::tests_hash::{fmt_ms, group};
use crate::tests_mate::pv_string;
use crate::tt::{self, TransTable};

/// Runs the reference positions up to `max_mate_in` moves with df-pn: a
/// direct-mapped table of `size_mb`, pn/dn packed by codec ("sat" or "float"),
/// options as letters: m = mobility initialisation, l = lazy mobility (1/1
/// leaves, the move count when a node is entered; overrides m), e/E/x/X =
/// 1+epsilon thresholds with epsilon 1/8, 1/2, 1, 2, f = depth-free final entries,
/// t = endgame tables as oracle (cached tables of the positions' own materials
/// included; beyond the base they hold buckets of `FILE_STEP` moves, the search
/// resolves the exact distance from the bounds), i = iterative deepening (the
/// shortest mate; otherwise a single search at the known depth 2*mate_in-1,
/// the yardstick run).
pub fn matepn_solve(max_mate_in: u32, size_mb: usize, codec: &str, options: &str) {
    run_matepn(MATE_POSITIONS, &|p| p.mate_in <= max_mate_in, size_mb, codec, options);
}

pub fn matepn_solve_named(name: &str, size_mb: usize, codec: &str, options: &str) {
    run_matepn(MATE_POSITIONS, &|p| p.name == name, size_mb, codec, options);
}

/// Runs a single position that is not in `MATE_POSITIONS` (the regression list
/// must not change), such as the five-piece KRR-KN of test-positions.md with
/// its cached table.
pub fn matepn_solve_fen(name: &'static str, fen: &'static str, mate_in: u32, size_mb: usize, codec: &str, options: &str) {
    let positions = [MatePosition { name, fen, mate_in, proof_positions: 0, note: "" }];
    run_matepn(&positions, &|_| true, size_mb, codec, options);
}

fn run_matepn(positions: &[MatePosition], selected: &dyn Fn(&MatePosition) -> bool, size_mb: usize, codec: &str, options: &str) {
    let table = tt::Table::new(size_mb);
    let bits = table.value_bits();
    let tables = options.contains('t');
    let mut set = if tables { Some(tests_egtb::egtb_load_or_generate(12)) } else { None };
    if let Some(set) = set.as_mut() {
        tests_egtb::egtb_load_for_positions(set, positions, selected);
    }
    match (codec, &set) {
        ("sat", None) => run(table, matepn::Saturating { bits }, mateab::Material, None, positions, selected, options),
        ("float", None) => run(table, matepn::Float { bits }, mateab::Material, None, positions, selected, options),
        ("sat", Some(set)) => run(table, matepn::Saturating { bits }, mateab::Tables { set }, Some(set), positions, selected, options),
        ("float", Some(set)) => run(table, matepn::Float { bits }, mateab::Tables { set }, Some(set), positions, selected, options),
        _ => panic!("codec sat or float"),
    }
}

#[allow(clippy::too_many_arguments)]
fn run<T: TransTable + 'static, C: Codec, O: Oracle + Copy>(table: T, codec: C, oracle: O, set: Option<&egtb::Set>, positions: &[MatePosition], selected: &dyn Fn(&MatePosition) -> bool, options: &str) {
    let has = |o: char| options.contains(o);
    let (lazy, final_entries, tables, iterative, mated) = (has('l'), has('f'), has('t'), has('i'), has('M'));
    let mobility = has('m') && !lazy;
    let mobility_name = if lazy {
        "lazy"
    } else if mobility {
        "on"
    } else {
        "off"
    };
    let mut epsilon = 0;
    for (letter, eps) in [('e', 1), ('E', 4), ('x', 8), ('X', 16)] {
        if has(letter) {
            epsilon = eps;
        }
    }
    println!(
        "=== matepn / df-pn, {}, direct-mapped TT {} MB ({} value bits), codec {}, mobility {}, epsilon {}/8, final {}, mated {}, {} ===",
        if iterative { "iterative deepening" } else { "single depth 2N-1" },
        (table.slots() * tt::ENTRY_BYTES) >> 20,
        table.value_bits(),
        codec.name(),
        mobility_name,
        epsilon,
        final_entries,
        mated,
        if tables { "endgame tables" } else { "material oracle" }
    );
    let mut ok = true;
    let mut total_nodes = 0u64;
    let mut total_time = Duration::ZERO;
    let mut table = Some(table);
    for (i, p) in positions.iter().enumerate() {
        if !selected(p) {
            continue;
        }
        println!("[{}] {}  {}  mate in {}", i + 1, p.name, p.fen, p.mate_in);
        let b = Board::from_fen(p.fen).expect("valid FEN");
        let mut t = table.take().unwrap();
        t.clear();
        t.reset_stats();
        let mut s = matepn::Searcher::new(oracle, t, codec, mobility);
        s.lazy_mobility = lazy;
        s.epsilon = epsilon;
        s.final_entries = final_entries;
        s.mated = mated;
        let start = Instant::now();
        let depth_start = std::rc::Rc::new(std::cell::Cell::new(start));
        let ds = depth_start.clone();
        s.progress = Some(Box::new(move |nodes, t: &T| {
            let st = t.stats();
            println!("              ... {} nodes, {}, tt {} stores, {} replaced", group(nodes), fmt_ms(ds.get().elapsed()), group(st.stores), group(st.replaced));
        }));
        let want = (2 * p.mate_in - 1) as usize;
        let r = if iterative {
            let mut last = 0u64;
            s.solve_shortest(&b, want, |plies, r| {
                let verdict = if r.proven { format!("mate in {}", plies.div_ceil(2)) } else { "no mate".to_string() };
                println!("    depth {:>2}: {:<10} {:>14} nodes {:>9}", plies, verdict, group(r.nodes - last), fmt_ms(depth_start.get().elapsed()));
                last = r.nodes;
                depth_start.set(Instant::now());
            })
        } else {
            s.solve(&b, want)
        };
        let elapsed = start.elapsed();
        total_nodes += r.nodes;
        total_time += elapsed;
        let t = s.table();
        let st = t.stats();
        if r.proven {
            println!(
                "    proven: {} nodes in {}, {} leaves, {} tt hits, {} final hits, {} stores, {} replaced, fill {:.1}%",
                group(r.nodes),
                fmt_ms(elapsed),
                group(r.leaves),
                group(r.tt_hits),
                group(r.final_hits),
                group(st.stores),
                group(st.replaced),
                100.0 * t.used() as f64 / t.slots() as f64
            );
            if let Some(set) = set {
                // the packed tables beyond the base (docs/m5-endgame-tables.md, "Blockformat")
                let (hits, misses, distinct, blocks) = set.cache_stats();
                if hits + misses > 0 {
                    println!("    block cache {} MB: {} hits, {} misses, {} of {} blocks touched", egtb::CACHE_BYTES >> 20, group(hits), group(misses), group(distinct), group(blocks));
                    set.reset_cache_stats();
                }
            }
            if r.mate_plies > 0 {
                let mut verdict = "the shortest".to_string();
                if r.mate_plies != want as i32 {
                    verdict = format!("shortest is {}", want);
                    if iterative {
                        verdict = format!("FAIL: {}", verdict);
                        ok = false;
                    }
                }
                println!("    proof tree: mate in {} plies ({})  pv {}", r.mate_plies, verdict, pv_string(&r.pv));
            } else {
                println!("    proof tree: incomplete in the table (entries overwritten)");
            }
            if p.proof_positions > 0 {
                println!("    yardstick: proof DAG {} positions, nodes / proof = {:.1}", group(p.proof_positions as u64), r.nodes as f64 / p.proof_positions as f64);
            }
        } else if r.disproven {
            println!("    FAIL: disproven after {} nodes in {}", group(r.nodes), fmt_ms(elapsed));
            ok = false;
        } else {
            println!("    FAIL: neither proven nor disproven after {} nodes", group(r.nodes));
            ok = false;
        }
        table = Some(s.into_table());
    }
    print!("--- total: {} nodes in {}", group(total_nodes), fmt_ms(total_time));
    println!("{}\n", if ok { "  [all ok]" } else { "  [FAILURES]" });
}
