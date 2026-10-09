//! Stage c of the UCI milestone, port of `mattjesGo/uci/search.go`: positions
//! the tables do not settle go to one of the mate searches of milestone 4,
//! running in its own thread with the tables as oracle. The main loop keeps
//! reading commands; "stop" (or the time budget) sets a flag the search
//! polls, the thread unwinds and reports the last completed depth. The
//! thread owns the `Set` and the transposition table while it runs and hands
//! both back with the result; loading tables stays with the main loop.

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{compact, Engine, Event, PROGRESS_INTERVAL};
use crate::bitboard::Board;
use crate::chess::{Move, RootMove};
use crate::egtb::{self, Set};
use crate::mateab::{self, Tables};
use crate::matelist;
use crate::matepn;
use crate::tt::{self, TransTable};

/// The Search option's default: df-pn proves mates with the fewest nodes
/// (docs/m4-mate-search.md).
pub const DEFAULT_ALGO: &str = "matepn";
/// The Hash option's default (the transposition table of mateab and matepn,
/// the position budget of matelist); the minimum is what mateab's 21 value
/// bits need from a direct-mapped table (2^21 slots).
pub const DEFAULT_HASH_MB: usize = 256;
pub const MIN_HASH_MB: usize = 32;
/// Below this mateab uses the direct-mapped table, because buckets have two
/// value bits less than a direct table of the same size.
const BUCKETS_FROM_MB: usize = 128;
/// Bounds the iterative deepening: mate in 64 at most (matepn's limit;
/// mateab's is one more).
pub const MAX_SEARCH_PLIES: usize = matepn::MAX_PLIES;
/// Turns the Hash size into matelist's position budget (record, store slot,
/// edges and counters, measured 110 to 130).
const MATELIST_BYTES_PER_POSITION: usize = 128;
/// How often (in nodes) a search offers a progress line; the reporter then
/// throttles by time. The searchers' own default (2^22 and 2^26) is minutes
/// apart at 200,000 nodes per second.
const PROGRESS_EVERY: u64 = 1 << 16;

pub fn is_algo(s: &str) -> bool {
    matches!(s.to_ascii_lowercase().as_str(), "none" | "mateab" | "matepn" | "matelist")
}

/// What "go" asks for: infinite, a depth bound in plies (mate N = 2N-1 plies,
/// depth N rounded up to odd) and a time budget (movetime, or a twentieth of
/// the remaining time plus half the increment).
pub struct GoParams {
    pub infinite: bool,
    pub max_plies: usize,
    pub budget: Option<Duration>,
}

/// The transposition table of the search: one at a time, the layout follows
/// the algorithm.
pub enum SearchTable {
    /// matepn: proofs must not be evicted by larger numbers.
    Direct(tt::Table),
    /// mateab: buckets keep mates over refutations.
    Buckets(tt::Buckets),
}

impl SearchTable {
    pub fn clear(&mut self) {
        match self {
            SearchTable::Direct(t) => t.clear(),
            SearchTable::Buckets(t) => t.clear(),
        }
    }
}

/// What the search thread hands back: the set and table it borrowed, the
/// mate it found (0 = none), the deepest completed depth and the counters.
/// The main loop prints the final line, because extending the line through
/// the tables may load tables, which only the main loop does.
pub struct SearchEnd {
    pub set: Set,
    pub table: Option<SearchTable>,
    pub mate_plies: usize,
    /// Best first, `root[0]` is the headline.
    pub root: Vec<RootMove>,
    pub plies: usize,
    pub nodes: u64,
    pub elapsed: Duration,
    pub aborted: bool,
    pub note: String,
}

/// The algorithm-independent part of a search result.
#[derive(Default)]
struct Core {
    mate_plies: usize,
    root: Vec<RootMove>,
    plies: usize,
    nodes: u64,
    note: String,
}

/// Prints the per-depth and progress lines of a running search; shared by
/// the searcher's progress callback and the per-depth report, hence the
/// interior mutability.
struct Reporter<W: Write + Send + 'static> {
    out: Arc<Mutex<W>>,
    start: Instant,
    last: Mutex<Instant>,
    multi_pv: usize,
}

/// Renders a root move's score: a proven mate by its length, or by the depth
/// as upper bound when the proof tree lost the length; cp 0 ("no mate within
/// the depth") otherwise.
fn root_score(rm: &RootMove, depth: usize) -> String {
    if rm.proven {
        let n = if rm.mate_plies == 0 { depth } else { rm.mate_plies as usize };
        format!("mate {}", n.div_ceil(2))
    } else {
        "cp 0".to_string()
    }
}

fn root_pv(rm: &RootMove) -> String {
    if rm.pv.is_empty() {
        rm.mv.uci()
    } else {
        pv_string(&rm.pv)
    }
}

impl<W: Write + Send + 'static> Reporter<W> {
    fn counters(&self, nodes: u64) -> String {
        let ms = self.start.elapsed().as_millis() as u64;
        format!("nodes {} nps {} time {}", nodes, nps(nodes, ms), ms)
    }

    /// Reports a completed depth without a mate: the first MultiPV root
    /// moves best first, each as "no mate within depth" (score cp 0) with
    /// the move as its line, so the GUI's list stays complete.
    fn depth(&self, depth: usize, nodes: u64, root: &[RootMove]) {
        *self.last.lock().unwrap() = Instant::now();
        let counters = self.counters(nodes);
        let mut o = self.out.lock().unwrap();
        if root.is_empty() {
            let _ = writeln!(o, "info depth {} score cp 0 {}", depth, counters);
        }
        for (i, rm) in root.iter().enumerate().take(self.multi_pv) {
            let _ = writeln!(o, "info depth {} multipv {} score {} {} pv {}", depth, i + 1, root_score(rm, depth), counters, root_pv(rm));
        }
        let _ = o.flush();
    }

    /// Reports the node count inside a depth and the root move under
    /// examination, at most every few seconds.
    fn progress(&self, depth: usize, nodes: u64, current: Option<Move>) {
        {
            let mut last = self.last.lock().unwrap();
            if last.elapsed() < PROGRESS_INTERVAL {
                return;
            }
            *last = Instant::now();
        }
        let cur = current.map(|m| format!(" currmove {}", m.uci())).unwrap_or_default();
        let mut o = self.out.lock().unwrap();
        let _ = writeln!(o, "info depth {}{} {}", depth, cur, self.counters(nodes));
        let _ = o.flush();
    }

    /// Reports a status line of a phase-based search, at most every few seconds.
    fn text(&self, line: &str) {
        {
            let mut last = self.last.lock().unwrap();
            if last.elapsed() < PROGRESS_INTERVAL {
                return;
            }
            *last = Instant::now();
        }
        let mut o = self.out.lock().unwrap();
        let _ = writeln!(o, "info string {}", compact(line));
        let _ = o.flush();
    }
}

fn nps(nodes: u64, ms: u64) -> u64 {
    nodes * 1000 / ms.max(1)
}

fn pv_string(pv: &[Move]) -> String {
    pv.iter().map(|m| m.uci()).collect::<Vec<_>>().join(" ")
}

impl<W: Write + Send + 'static> Engine<W> {
    pub(super) fn parse_go(&self, words: &[&str]) -> GoParams {
        let mut p = GoParams { infinite: false, max_plies: MAX_SEARCH_PLIES, budget: None };
        let num = |i: usize| words.get(i + 1).and_then(|w| w.parse::<i64>().ok()).unwrap_or(0);
        let white = self.board.white_move;
        let (mut time_left, mut inc, mut movetime) = (0i64, 0i64, 0i64); // milliseconds
        for (i, &w) in words.iter().enumerate() {
            match w {
                "infinite" => p.infinite = true,
                "movetime" => movetime = num(i),
                "wtime" if white => time_left = num(i),
                "btime" if !white => time_left = num(i),
                "winc" if white => inc = num(i),
                "binc" if !white => inc = num(i),
                "mate" => {
                    let n = num(i);
                    if n > 0 {
                        p.max_plies = MAX_SEARCH_PLIES.min((2 * n - 1) as usize);
                    }
                }
                "depth" => {
                    let n = num(i);
                    if n > 0 {
                        p.max_plies = MAX_SEARCH_PLIES.min((n | 1) as usize);
                    }
                }
                _ => {}
            }
        }
        if p.infinite {
        } else if movetime > 0 {
            p.budget = Some(Duration::from_millis(movetime as u64));
        } else if time_left > 0 {
            p.budget = Some(Duration::from_millis((time_left / 20 + inc / 2).max(50) as u64));
        }
        p
    }

    /// Runs the configured algorithm on the current position in a thread; the
    /// result arrives through the search channel, announced by an event.
    pub(super) fn start_search(&mut self, p: GoParams) {
        let stop = Arc::new(AtomicBool::new(false));
        self.stop = Some(stop.clone());
        self.searching = true;
        if let Some(budget) = p.budget {
            let s = stop.clone();
            std::thread::spawn(move || {
                std::thread::sleep(budget);
                s.store(true, Ordering::Relaxed);
            });
        }
        let (root, algo, max_plies, multi_pv) = (self.board, self.algo.clone(), p.max_plies, self.multi_pv);
        let table = if algo == "matelist" { None } else { self.search_table(&algo) };
        let max_positions = (self.hash_mb << 20) / MATELIST_BYTES_PER_POSITION;
        let set = self.set.take().expect("tables are loaded before searching");
        let (out, tx, search_tx) = (self.out.clone(), self.tx.clone(), self.search_tx.clone());
        std::thread::spawn(move || {
            let rep = Arc::new(Reporter { out, start: Instant::now(), last: Mutex::new(Instant::now()), multi_pv });
            let (core, table) = {
                let oracle = Tables { set: &set };
                match (algo.as_str(), table) {
                    ("mateab", Some(SearchTable::Buckets(t))) => {
                        let (c, t) = run_mateab(&root, max_plies, oracle, t, &stop, &rep);
                        (c, Some(SearchTable::Buckets(t)))
                    }
                    ("mateab", Some(SearchTable::Direct(t))) => {
                        let (c, t) = run_mateab(&root, max_plies, oracle, t, &stop, &rep);
                        (c, Some(SearchTable::Direct(t)))
                    }
                    ("matepn", Some(SearchTable::Direct(t))) => {
                        let (c, t) = run_matepn(&root, max_plies, oracle, t, &stop, &rep);
                        (c, Some(SearchTable::Direct(t)))
                    }
                    ("matelist", table) => (run_matelist(&root, max_plies, max_positions, &stop, &rep), table),
                    (_, table) => (Core::default(), table),
                }
            };
            let end = SearchEnd {
                set,
                table,
                mate_plies: core.mate_plies,
                root: core.root,
                plies: core.plies,
                nodes: core.nodes,
                elapsed: rep.start.elapsed(),
                aborted: stop.load(Ordering::Relaxed),
                note: core.note,
            };
            let _ = search_tx.send(end);
            let _ = tx.send(Event::Searched);
        });
    }

    /// Ends a running search and handles its result (which sends the best
    /// move unless a "go infinite" holds it back).
    pub(super) fn stop_search(&mut self) {
        if !self.searching {
            return;
        }
        if let Some(s) = &self.stop {
            s.store(true, Ordering::Relaxed);
        }
        match self.search_rx.recv() {
            Ok(r) => self.finish_search(r),
            Err(_) => self.searching = false,
        }
    }

    /// Takes the set and table back, prints the result of a search and sends
    /// the best move (held back during a "go infinite"). A found mate is
    /// reported with its line, continued through the tables where the search
    /// ended in a table position.
    pub(super) fn finish_search(&mut self, r: SearchEnd) {
        self.searching = false;
        self.stop = None;
        let mut set = r.set;
        set.cache_dir = self.cache_dir.clone();
        self.set = Some(set);
        if r.table.is_some() {
            self.tables = r.table;
        }
        if self.save_pending {
            self.save_pending = false;
            self.save_tables();
        }
        let ms = r.elapsed.as_millis() as u64;
        if r.mate_plies > 0 && !r.root.is_empty() {
            // the first MultiPV root moves: proven ones with their lines continued
            // through the tables, the rest as cp 0 (no mate found behind them)
            let counters = format!("nodes {} nps {} time {}", r.nodes, nps(r.nodes, ms), ms);
            for (i, rm) in r.root.iter().enumerate().take(self.multi_pv) {
                let mut pv = rm.pv.clone();
                if rm.proven {
                    if pv.is_empty() {
                        pv.push(rm.mv);
                    }
                    pv = self.extend_pv(&pv);
                }
                let shown = RootMove { mv: rm.mv, pv, ..Default::default() };
                self.send(&format!("info depth {} multipv {} score {} {} pv {}", r.mate_plies, i + 1, root_score(rm, r.mate_plies), counters, root_pv(&shown)));
            }
            self.pending_best = r.root[0].mv.uci();
        } else if r.nodes > 0 || r.note.is_empty() {
            let what = if r.aborted { "stopped, no mate" } else { "no mate" };
            self.send(&format!("info string search: {} within {} plies, {} nodes in {:.1} s", what, r.plies, egtb::group(r.nodes as usize), ms as f64 / 1000.0));
        }
        if !r.note.is_empty() {
            self.send(&format!("info string search: {}", r.note));
        }
        if !self.infinite {
            let best = self.pending_best.clone();
            self.send(&format!("bestmove {}", best));
        }
    }

    /// Plays the search's line and, where it ends in a position the tables
    /// know, appends the optimal play to mate.
    fn extend_pv(&mut self, pv: &[Move]) -> Vec<Move> {
        let mut b = self.board;
        for &m in pv {
            b.do_move(m);
        }
        self.load_material(&b);
        let mut out = pv.to_vec();
        if let Some(v) = self.eval(&b, 1) {
            if v.is_win() || v.is_loss() {
                let rest = self.line(b, v);
                out.extend(rest);
            }
        }
        out
    }

    /// Takes the transposition table for the algorithm out of the engine,
    /// allocating it with the current Hash size when the size or the layout
    /// changed (one table at a time).
    fn search_table(&mut self, algo: &str) -> Option<SearchTable> {
        let want_buckets = algo == "mateab" && self.hash_mb >= BUCKETS_FROM_MB;
        let fits = match &self.tables {
            Some(SearchTable::Buckets(_)) => want_buckets,
            Some(SearchTable::Direct(_)) => !want_buckets,
            None => false,
        };
        if !fits || self.table_mb != self.hash_mb {
            self.tables = None; // free the old one before allocating the new one
            self.tables = Some(if want_buckets { SearchTable::Buckets(tt::Buckets::new(self.hash_mb)) } else { SearchTable::Direct(tt::Table::new(self.hash_mb)) });
            self.table_mb = self.hash_mb;
        }
        self.tables.take()
    }
}

/// Depth-first with iterative deepening (mateab); the bucket table keeps
/// proven mates over refutations (a direct table below BUCKETS_FROM_MB).
fn run_mateab<W: Write + Send + 'static, T: TransTable + 'static>(root: &Board, max_plies: usize, oracle: Tables<'_>, table: T, stop: &Arc<AtomicBool>, rep: &Arc<Reporter<W>>) -> (Core, T) {
    let mut s = mateab::Searcher::new(oracle, Some(table));
    s.stop = Some(stop.clone());
    s.progress_every = PROGRESS_EVERY;
    let (base, depth) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicUsize::new(1)));
    let (r2, b2, d2) = (rep.clone(), base.clone(), depth.clone());
    s.progress = Some(Box::new(move |nodes, current, _| r2.progress(d2.load(Ordering::Relaxed), b2.load(Ordering::Relaxed) + nodes, current)));
    let r = s.solve(root, max_plies as u32, |plies, r| {
        base.store(r.nodes, Ordering::Relaxed);
        depth.store(plies as usize + 2, Ordering::Relaxed);
        if r.mate_plies == 0 {
            rep.depth(plies as usize, r.nodes, &r.root);
        }
    });
    let core = Core { mate_plies: r.mate_plies as usize, root: r.root, plies: r.plies as usize, nodes: r.nodes, note: String::new() };
    (core, s.into_table().expect("the table was given"))
}

/// df-pn with iterative deepening (matepn, the measured best settings:
/// mobility, epsilon 1/2, final entries), direct-mapped table.
fn run_matepn<W: Write + Send + 'static, T: TransTable + 'static>(root: &Board, max_plies: usize, oracle: Tables<'_>, table: T, stop: &Arc<AtomicBool>, rep: &Arc<Reporter<W>>) -> (Core, T) {
    let bits = table.value_bits();
    let mut s = matepn::Searcher::new(oracle, table, matepn::Saturating { bits }, true);
    s.epsilon = 4;
    s.final_entries = true;
    s.stop = Some(stop.clone());
    s.progress_every = PROGRESS_EVERY;
    let (base, depth) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicUsize::new(1)));
    let (r2, b2, d2) = (rep.clone(), base.clone(), depth.clone());
    s.progress = Some(Box::new(move |nodes, current, _| r2.progress(d2.load(Ordering::Relaxed), b2.load(Ordering::Relaxed) + nodes, current)));
    let r = s.solve_shortest(root, max_plies, |plies, r| {
        base.store(r.nodes, Ordering::Relaxed);
        depth.store(plies + 2, Ordering::Relaxed);
        if !r.proven {
            rep.depth(plies, r.nodes, &r.root);
        }
    });
    let mut core = Core { root: r.root, plies: r.plies, nodes: r.nodes, ..Default::default() };
    if r.proven {
        core.mate_plies = r.mate_plies;
        if core.mate_plies == 0 {
            // proven within the depth, but the proof tree lost entries: the
            // depth is an upper bound of the mate length
            core.mate_plies = r.plies;
            core.note = "proof tree incomplete (table entries replaced), the mate is at most that long".to_string();
        }
    }
    (core, s.into_table())
}

/// Breadth-first enumeration plus retrograde analysis (matelist); the Hash
/// size bounds the positions, a stop ends the enumeration at the next ply
/// and resolves what was reached.
fn run_matelist<W: Write + Send + 'static>(root: &Board, max_plies: usize, max_positions: usize, stop: &Arc<AtomicBool>, rep: &Arc<Reporter<W>>) -> Core {
    match matelist::solve(root, max_plies as u32, max_positions, Some(stop), &mut |line| rep.text(&format!("matelist: {}", line))) {
        Ok(r) => Core { mate_plies: r.mate_plies as usize, root: r.root, plies: r.plies as usize, nodes: r.positions as u64, note: String::new() },
        Err(e) => Core { note: format!("{} (raise Hash)", e), ..Default::default() },
    }
}
