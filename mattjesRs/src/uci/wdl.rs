//! The WDL search (Search fastwdl, milestone 7): every root move is asked
//! two questions on the explicit graph (wdllist, the proof-number descent
//! racing the certificate builder): "do we win after it" and "does the
//! opponent win after it"; both refuted is a guaranteed draw. Mirrors
//! `mattjesGo/uci/wdl.go`. Scores (the author's choice): 9999 win, -9999
//! loss, 0 draw, 1 unknown, 10 "cannot lose, might win", -10 "cannot win,
//! might lose".

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::search::{nps, pv_string, Core, Reporter};
use super::{RootMove, PROGRESS_INTERVAL};
use crate::bitboard::Board;
use crate::chess::{new_buffer, Move};
use crate::mateab::Tables;
use crate::rollout;
use crate::wdllist::{self, Options, Outcome};

/// A root move's state, best first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum WdlClass {
    Win,
    NoLoss,
    Draw,
    Unknown,
    NoWin,
    Loss,
}

impl WdlClass {
    pub fn score(self) -> &'static str {
        match self {
            WdlClass::Win => "cp 9999",
            WdlClass::NoLoss => "cp 10",
            WdlClass::Draw => "cp 0",
            WdlClass::Unknown => "cp 1",
            WdlClass::NoWin => "cp -10",
            WdlClass::Loss => "cp -9999",
        }
    }

    pub fn decided(self) -> bool {
        matches!(self, WdlClass::Win | WdlClass::Draw | WdlClass::Loss)
    }
}

/// One root move's result.
#[derive(Clone)]
pub struct WdlEntry {
    pub mv: Move,
    pub class: WdlClass,
    /// The winning line (ours or the opponent's), the move first.
    pub pv: Vec<Move>,
    /// Positions of the move's graphs (both questions, both racers).
    pub positions: usize,
    pub method: &'static str,
    /// The rollout's guess for us (WdlRollouts), Unknown without.
    pub guess: rollout::Outcome,
}

/// Orders the open moves by the rollout's guess: wins first (the first
/// proven win ends a MultiPV 1 search), then unknown, draws, losses.
fn guess_rank(o: rollout::Outcome) -> u8 {
    match o {
        rollout::Outcome::Win => 0,
        rollout::Outcome::Unknown => 1,
        rollout::Outcome::Draw => 2,
        rollout::Outcome::Loss => 3,
    }
}

/// The measured memory of a graph position (record, key, store slot,
/// edges with their headers and growth, counters: 240 to 255 bytes on
/// KQ-KP with 4 million positions, 2026-10-22).
pub const WDL_BYTES_PER_POSITION: usize = 256;

/// The number of graphs the search holds (two questions, each racing the
/// descent against the certificate builder). Hash is the hard cap over all
/// of them: Hash·2^20 / (WDL_GRAPHS·WDL_BYTES_PER_POSITION) positions per
/// graph, so Hash 16384 allows 16 Mi positions per graph.
pub const WDL_GRAPHS: usize = 4;

/// The piece value sum of a side without the king (queen 9, rook 5,
/// bishop and knight 3, pawn 1), to ask the weaker side's question first:
/// refuting the win of a lone knight is a small certificate, proving the
/// win of two rooks against it is deep, and the cheap answer ("we cannot
/// lose", cp 10) should not wait behind the expensive one.
fn wdl_material(b: &Board, white: bool) -> u32 {
    let side = if white { 0 } else { 1 };
    const VALUES: [u32; 6] = [0, 9, 5, 3, 3, 1]; // kind order K, Q, R, B, N, P
    VALUES.iter().zip(b.pieces[side].iter()).map(|(v, bb)| v * bb.count_ones()).sum()
}

/// The optional table extension of a line (the main loop has it, the
/// search thread does not).
pub type Extend<'a> = Option<&'a mut dyn FnMut(&[Move]) -> Vec<Move>>;

/// The position budget of the first round (per question).
pub const WDL_FIRST_BUDGET: usize = 1 << 16;

/// Multiplies the budget from round to round; the rounds before the last
/// one cost a third of it in total.
pub const WDL_BUDGET_GROWTH: usize = 4;

/// Drives the WDL search over the root moves in rounds: known table values
/// are taken as they are, every other move gets the two questions with a
/// position budget that grows by WDL_BUDGET_GROWTH per round until it
/// reaches the Hash budget, so that no single move eats the whole time
/// before the others are even looked at (a graph is built anew per
/// question, the restarts cost a third). The list is reported after every
/// decision; the search ends when every move is decided, when the top
/// multi_pv moves are proven wins (nothing can rank above them), when the
/// full budget left moves open, or on stop.
#[allow(clippy::too_many_arguments)]
pub fn run_wdl<W: Write + Send + 'static>(root: &Board, multi_pv: usize, rep: &Arc<Reporter<W>>, known: &[RootMove], stop: &Arc<AtomicBool>, max_positions: usize, oracle: Tables<'_>, net_init: bool, rollouts: bool, threads: usize) -> Core {
    let mut buf = new_buffer();
    let n = root.gen_moves(&mut buf);
    let mut entries: Vec<WdlEntry> = Vec::with_capacity(n);
    let mut open = Vec::new();
    for (i, &mv) in buf[..n].iter().enumerate() {
        let mut en = WdlEntry { mv, class: WdlClass::Unknown, pv: Vec::new(), positions: 0, method: "", guess: rollout::Outcome::Unknown };
        let mut settled = false;
        for k in known.iter().filter(|k| k.mv == mv) {
            settled = true;
            en.method = "table";
            if k.value.is_win() {
                en.class = WdlClass::Win;
                en.pv = k.pv.clone();
            } else if k.value.is_loss() {
                en.class = WdlClass::Loss;
                en.pv = k.pv.clone();
            } else {
                en.class = WdlClass::Draw;
            }
        }
        if !settled {
            open.push(i);
        }
        entries.push(en);
    }
    let mut core = Core::default();
    let mut positions = 0u64;
    let last = Mutex::new(Instant::now());
    let rep_progress = rep.clone();
    let progress: wdllist::Progress = Arc::new(move |line: &str| {
        let mut l = last.lock().unwrap();
        if l.elapsed() < PROGRESS_INTERVAL {
            return;
        }
        *l = Instant::now();
        rep_progress.text(&format!("wdl: {}", line));
    });
    // level 1 of the net initialisation measured best over the wins (docs/nnue-patricia.md)
    let opt = Options { epsilon: 8, max_positions, stop: Some(stop.clone()), progress: Some(progress), progress_every: 1 << 22, oracle: Some(oracle), net_init: if net_init { 1 } else { 0 }, ..Default::default() };
    // two shared graph pairs for all root moves: 0 "we win after the
    // move" (we attack), 1 "the opponent wins after it"
    let mut solvers = [wdllist::Solver::new(root, true, opt.clone()), wdllist::Solver::new(root, false, opt.clone())];
    let mut counted = 0usize; // positions of all graphs booked so far
    let mut decided = n - open.len();
    let mut wins = entries.iter().filter(|en| en.class == WdlClass::Win).count();
    rep.wdl_lines(decided, &entries, positions);
    // the weaker side's question first (wdl_material): 0 = we win, 1 = the
    // opponent wins
    let order: [usize; 2] = if wdl_material(root, !root.white_move) < wdl_material(root, root.white_move) { [1, 0] } else { [0, 1] };
    if rollouts && !open.is_empty() {
        // a rollout per open move as the guess (module rollout, Patricia's
        // net as both sides' policy down to the tables): guessed wins are
        // asked first, and every move is asked its guessed question first
        let start = Instant::now();
        let mut guesses = [0usize; 4];
        let ropt = rollout::Options { max_plies: 0, oracle: Some(oracle) };
        for &i in &open {
            let mut c = *root;
            c.do_move(entries[i].mv);
            let r = rollout::play(&c, ropt);
            let g = match r.outcome {
                rollout::Outcome::Win => rollout::Outcome::Loss, // for the opponent, to move after the move
                rollout::Outcome::Loss => rollout::Outcome::Win,
                o => o,
            };
            entries[i].guess = g;
            guesses[g as usize] += 1;
        }
        open.sort_by_key(|&i| guess_rank(entries[i].guess));
        rep.note(&format!("wdl: rollouts guess {} win, {} draw, {} loss, {} unknown of {} open moves in {:?}", guesses[rollout::Outcome::Win as usize], guesses[rollout::Outcome::Draw as usize], guesses[rollout::Outcome::Loss as usize], guesses[rollout::Outcome::Unknown as usize], open.len(), start.elapsed()));
    }
    let mut budget = WDL_FIRST_BUDGET.min(max_positions);
    let mut round = 0;
    while !open.is_empty() && wins < multi_pv && !core.aborted {
        round += 1;
        rep.note(&format!("wdl: round {}, {} new positions per graph and question, {} moves open, {} positions so far", round, budget, open.len(), positions));
        let mut still_open = Vec::with_capacity(open.len());
        let round_start = counted;
        for &i in &open {
            if stop.load(Ordering::Relaxed) {
                core.aborted = true;
                break;
            }
            let mv = entries[i].mv;
            *rep.current.lock().unwrap() = Some((mv, i + 1));
            let before = entries[i].class;
            // asks question q for the move with this round's increment and
            // books what the shared graphs grew by
            let ask = |q: usize, solvers: &mut [wdllist::Solver<'_>], entries: &mut Vec<WdlEntry>, positions: &mut u64, counted: &mut usize| {
                let r = solvers[q].run_move(mv, budget);
                let total = solvers[0].positions() + solvers[1].positions();
                *positions += (total - *counted) as u64;
                entries[i].positions += total - *counted;
                *counted = total;
                r
            };
            // the guessed question first: a guessed win asks "we win", a
            // guessed loss "the opponent wins", the rest the material order
            let move_order: [usize; 2] = match entries[i].guess {
                rollout::Outcome::Win => [0, 1],
                rollout::Outcome::Loss => [1, 0],
                _ => order,
            };
            let needed = |class: WdlClass, q: usize| q == 0 && matches!(class, WdlClass::Unknown | WdlClass::NoLoss) || q == 1 && matches!(class, WdlClass::Unknown | WdlClass::NoWin);
            // with two threads both questions of an open move run at once
            // (separate graphs); the results are applied in the move's
            // order, the second only while it is still needed
            let mut results: [Option<wdllist::Result>; 2] = [None, None];
            if threads >= 2 && entries[i].class == WdlClass::Unknown {
                let (s0, s1) = solvers.split_at_mut(1);
                let (r0, r1) = std::thread::scope(|s| {
                    let h0 = s.spawn(|| s0[0].run_move(mv, budget));
                    let h1 = s.spawn(|| s1[0].run_move(mv, budget));
                    (h0.join().expect("question 0"), h1.join().expect("question 1"))
                });
                let total = solvers[0].positions() + solvers[1].positions();
                positions += (total - counted) as u64;
                entries[i].positions += total - counted;
                counted = total;
                results = [Some(r0), Some(r1)];
            }
            for &q in &move_order {
                if !needed(entries[i].class, q) {
                    continue;
                }
                let r = match results[q].take() {
                    Some(r) => r,
                    None => ask(q, &mut solvers, &mut entries, &mut positions, &mut counted),
                };
                if q == 0 {
                    // we win? (the root is a defender node of the child: the
                    // opponent to move, we attack)
                    match r.outcome {
                        Outcome::Proven => {
                            entries[i].class = WdlClass::Win;
                            entries[i].pv = std::iter::once(mv).chain(r.pv.iter().copied()).collect();
                            entries[i].method = r.method;
                            wins += 1;
                        }
                        Outcome::Disproven => {
                            entries[i].class = if entries[i].class == WdlClass::NoLoss { WdlClass::Draw } else { WdlClass::NoWin };
                            entries[i].method = r.method;
                        }
                        Outcome::Undecided => {}
                    }
                } else {
                    // the opponent wins? (the root is an attacker node of the child)
                    match r.outcome {
                        Outcome::Proven => {
                            entries[i].class = WdlClass::Loss;
                            entries[i].pv = std::iter::once(mv).chain(r.pv.iter().copied()).collect();
                            entries[i].method = r.method;
                        }
                        Outcome::Disproven => {
                            entries[i].class = if entries[i].class == WdlClass::NoWin { WdlClass::Draw } else { WdlClass::NoLoss };
                            entries[i].method = r.method;
                        }
                        Outcome::Undecided => {}
                    }
                }
                if r.aborted && stop.load(Ordering::Relaxed) {
                    core.aborted = true;
                    break;
                }
            }
            if core.aborted {
                break;
            }
            if entries[i].class.decided() {
                decided += 1;
            } else {
                still_open.push(i);
            }
            if entries[i].class != before {
                rep.wdl_lines(decided, &entries, positions);
            }
            if wins >= multi_pv {
                break;
            }
        }
        if core.aborted {
            break;
        }
        let progressed = counted > round_start || still_open.len() < open.len();
        open = still_open;
        if !progressed {
            break; // the graphs are full and nothing was decided: the rest stays open
        }
        budget = (budget * WDL_BUDGET_GROWTH).min(max_positions);
    }
    drop(solvers); // the graphs go with the search
    core.nodes = positions;
    // answered only when every move is decided: full graphs end the search
    // with open moves, and a background generation must go on for them
    core.answered = decided == n;
    core.wdl = Some(order_wdl(&entries));
    core
}

/// Sorts best first: wins, no-loss, draws, unknown, no-win, losses; within
/// a class by the positions it cost (the hardest first).
pub fn order_wdl(entries: &[WdlEntry]) -> Vec<WdlEntry> {
    let mut out = entries.to_vec();
    out.sort_by(|a, b| a.class.cmp(&b.class).then(b.positions.cmp(&a.positions)));
    out
}

/// The MultiPV lines of the WDL search: one depth per block (the number of
/// decided moves), the score of the class, the line as given (extended
/// through the tables by the caller when it may touch them).
pub fn wdl_block(multi_pv: usize, ms: u64, decided: usize, list: &[WdlEntry], positions: u64, mut extend: Extend<'_>) -> Vec<String> {
    let mut lines = Vec::new();
    for (i, en) in list.iter().enumerate().take(multi_pv) {
        let mut pv = en.pv.clone();
        if pv.is_empty() {
            pv.push(en.mv);
        }
        if let Some(ext) = extend.as_mut() {
            if en.class == WdlClass::Win || en.class == WdlClass::Loss {
                pv = ext(&pv);
            }
        }
        lines.push(format!("info depth {} multipv {} score {} nodes {} nps {} time {} pv {}", decided.max(1), i + 1, en.class.score(), positions, nps(positions, ms), ms, pv_string(&pv)));
    }
    lines
}

/// The closing info string of a WDL search.
pub fn wdl_summary(entries: &[WdlEntry]) -> String {
    let mut counts = [0usize; 6];
    for en in entries {
        counts[en.class as usize] += 1;
    }
    format!("wdl: {} win, {} cannot lose, {} draw, {} unknown, {} cannot win, {} loss", counts[0], counts[1], counts[2], counts[3], counts[4], counts[5])
}

impl<W: Write + Send + 'static> Reporter<W> {
    /// Prints the MultiPV block of the WDL search (no table extension: the
    /// search thread must not touch the tables).
    pub fn wdl_lines(&self, decided: usize, entries: &[WdlEntry], positions: u64) {
        let list = order_wdl(entries);
        let ms = self.start.elapsed().as_millis() as u64;
        let lines = wdl_block(self.multi_pv, ms, decided, &list, positions, None);
        let mut o = self.out.lock().unwrap();
        for l in lines {
            let _ = writeln!(o, "{}", l);
        }
        let _ = o.flush();
    }
}
