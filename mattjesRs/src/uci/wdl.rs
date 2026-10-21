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
}

/// Turns the Hash size into the position budget of one graph (generous:
/// the author ignores memory for now).
pub const WDL_BYTES_PER_POSITION: usize = 64;

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
pub fn run_wdl<W: Write + Send + 'static>(root: &Board, multi_pv: usize, rep: &Arc<Reporter<W>>, known: &[RootMove], stop: &Arc<AtomicBool>, max_positions: usize) -> Core {
    let mut buf = new_buffer();
    let n = root.gen_moves(&mut buf);
    let mut entries: Vec<WdlEntry> = Vec::with_capacity(n);
    let mut open = Vec::new();
    for (i, &mv) in buf[..n].iter().enumerate() {
        let mut en = WdlEntry { mv, class: WdlClass::Unknown, pv: Vec::new(), positions: 0, method: "" };
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
    let mut opt = Options { epsilon: 8, max_positions, stop: Some(stop.clone()), progress: Some(progress), progress_every: 1 << 22, ..Default::default() };
    let mut decided = n - open.len();
    let mut wins = entries.iter().filter(|en| en.class == WdlClass::Win).count();
    rep.wdl_lines(decided, &entries, positions);
    // the two questions per move stay alive between the rounds: 0 "we
    // win", 1 "the opponent wins"; None until asked, dropped with the move
    let mut solvers: Vec<[Option<wdllist::Solver>; 2]> = (0..n).map(|_| [None, None]).collect();
    let mut budget = WDL_FIRST_BUDGET.min(max_positions);
    let mut round = 0;
    while !open.is_empty() && wins < multi_pv && !core.aborted {
        round += 1;
        opt.max_positions = budget;
        rep.note(&format!("wdl: round {}, {} positions per graph, {} moves open, {} positions so far", round, budget, open.len(), positions));
        let mut still_open = Vec::with_capacity(open.len());
        for &i in &open {
            if stop.load(Ordering::Relaxed) {
                core.aborted = true;
                break;
            }
            let mv = entries[i].mv;
            let mut c = *root;
            c.do_move(mv);
            *rep.current.lock().unwrap() = Some((mv, i + 1));
            let before = entries[i].class;
            // asks question q with this round's budget and books the memory
            // of the move's graphs
            let mut ask = |q: usize, attacks: bool, entries: &mut Vec<WdlEntry>, positions: &mut u64| {
                let r = solvers[i][q].get_or_insert_with(|| wdllist::Solver::new(&c, attacks, opt.clone())).run(budget);
                let total: usize = solvers[i].iter_mut().flatten().map(|s| s.positions()).sum();
                *positions += (total - entries[i].positions) as u64;
                entries[i].positions = total;
                r
            };
            // we win? (the root is a defender node of the child: the opponent
            // to move, we attack); open unless the first question is answered
            if matches!(entries[i].class, WdlClass::Unknown | WdlClass::NoLoss) {
                let r = ask(0, false, &mut entries, &mut positions);
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
                if r.aborted && stop.load(Ordering::Relaxed) {
                    core.aborted = true;
                    break;
                }
            }
            // the opponent wins? (the root is an attacker node of the child)
            if matches!(entries[i].class, WdlClass::Unknown | WdlClass::NoWin) {
                let r = ask(1, true, &mut entries, &mut positions);
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
                if r.aborted && stop.load(Ordering::Relaxed) {
                    core.aborted = true;
                    break;
                }
            }
            if entries[i].class.decided() {
                decided += 1;
                solvers[i] = [None, None];
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
        open = still_open;
        if budget >= max_positions {
            break; // the full budget left them open
        }
        budget = (budget * WDL_BUDGET_GROWTH).min(max_positions);
    }
    drop(solvers); // the open graphs go with the search
    core.nodes = positions;
    core.answered = !core.aborted;
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
