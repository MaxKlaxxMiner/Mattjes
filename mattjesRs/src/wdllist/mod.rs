//! The WDL search on an explicit graph (milestone 7, step 2,
//! docs/m7-wdl-search.md). Direct port of `mattjesGo/wdllist`, see there
//! for the full explanation: positions and edges are stored, wins are
//! propagated backwards with counters as they appear, a draw is the
//! greatest fixed point "the defender always has a move that keeps the
//! attacker away from a win", and two searchers race per question: the
//! proof-number descent (good at wins) and the certificate builder (good at
//! draws with a small safe set).

mod certificate;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::bitboard::{packed_fixed_record, Board, Codec, PackedFixed};
use crate::chess::{new_buffer, Move, MAX_MOVES};
use crate::mateab::{Material, Oracle, Verdict};
use crate::ttstore::{Store, MAX_LOAD_PERCENT};

/// Outcome of one question.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Outcome {
    #[default]
    Undecided,
    /// The attacker forces a win.
    Proven,
    /// The defender holds at least a draw (the root is safe).
    Disproven,
}

/// Result of one question.
#[derive(Clone, Default)]
pub struct Result {
    pub outcome: Outcome,
    /// Which search decided: "pn", "certificate", "brute force".
    pub method: &'static str,
    pub pv: Vec<Move>,
    /// Length of the attacker's win along the graph levels (0 if not proven).
    pub win_plies: usize,
    /// Unique positions in the graph.
    pub positions: usize,
    /// Positions whose children were added.
    pub expansions: usize,
    /// Child edges.
    pub edges: usize,
    pub visits: u64,
    /// Heuristic disproofs the fixed point did not confirm.
    pub resets: usize,
    /// Fixed-point runs.
    pub safe_checks: usize,
    /// Widening rounds of the certificate builder.
    pub rounds: usize,
    /// Positions in the safe set reachable from the root (draw certificate) when disproven.
    pub safe: usize,
    /// Positions won for the attacker.
    pub won: usize,
    /// Subgraphs expanded by brute force because the numbers had saturated.
    pub exhausts: usize,
    pub aborted: bool,
    /// Aborted because the descent made no progress (a bug, never a result).
    pub stuck: bool,
}

/// The progress callback: shared by the two racing searchers.
pub type Progress = Arc<dyn Fn(&str) + Send + Sync>;

/// Options of the search.
#[derive(Clone, Default)]
pub struct Options {
    /// Expands the whole reachable graph, no guidance.
    pub brute_force: bool,
    /// Runs the proof-number descent alone (no certificate builder).
    pub no_race: bool,
    /// Widens the child's threshold (in eighths, as in matepn).
    pub epsilon: u32,
    /// Clips the stored node numbers (0 = DEFAULT_NUMBER_CAP).
    pub number_cap: u32,
    /// Keeps proof and disproof numbers on a logarithmic scale (see the Go
    /// package): sums never saturate, discrimination suffers.
    pub log_numbers: bool,
    /// Stops the search with `aborted` beyond this many positions.
    pub max_positions: usize,
    /// Polled every STOP_NODES visits and every 1024 expansions.
    pub stop: Option<Arc<AtomicBool>>,
    /// The race's own stop flag for one searcher (set by `solve`).
    pub race_stop: Option<Arc<AtomicBool>>,
    /// Receives a line every `progress_every` expansions and after every fixed-point run.
    pub progress: Option<Progress>,
    pub progress_every: usize,
}

/// The infinite proof or disproof number.
pub const INF: u32 = 1 << 30;

/// The polling interval of the stop flag (a power of two).
pub const STOP_NODES: u64 = 1 << 12;

/// Caps the descent (a node beyond it returns its thresholds, i.e. stays
/// unknown); the graph has no other depth bound.
pub const MAX_DEPTH: usize = 1024;

/// Clips the numbers a node stores, exactly like the saturating codec of
/// wdlpn; sums inside a node and thresholds are not clipped.
pub const DEFAULT_NUMBER_CAP: u32 = 1022;

/// Stores the numbers almost unclipped.
pub const WIDE_NUMBER_CAP: u32 = INF / 2;

/// The stack of the search threads: the descent recurses up to MAX_DEPTH
/// with about 4 KB per level.
const THREAD_STACK: usize = 16 << 20;

/// The store starts this small and grows fourfold whenever it is full (the
/// keys are kept per node for the rehash): a question that ends after a
/// few thousand positions must not pay for zeroing a 512 MB store.
const INITIAL_SLOTS: usize = 1 << 16;

const ST_UNKNOWN: u8 = 0;
const ST_WIN: u8 = 1; // the side to move wins
const ST_LOSS: u8 = 2; // the side to move loses
const ST_DRAW_T: u8 = 3; // terminal draw (stalemate, dead material)
const ST_SAFE: u8 = 4; // the attacker cannot win here (fixed point)

const FL_EXPANDED: u8 = 1; // children added
const FL_ON_PATH: u8 = 2; // on the current descent path
const FL_UNSAFE: u8 = 4; // scratch flag of the fixed point
const FL_REGION: u8 = 8; // scratch flag of exhaust and the closure

type Record = [u8; PackedFixed::MAX_BYTES];

/// The explicit position graph of one question.
pub struct Graph {
    attacker_white: bool,
    log: bool,
    store: Store,
    max_slots: usize,
    keys: Vec<crate::tt::Key>,
    recs: Vec<Record>,
    children: Vec<Vec<u32>>,
    parents: Vec<Vec<u32>>,
    status: Vec<u8>,
    white: Vec<bool>,
    level: Vec<u16>,
    nmoves: Vec<u8>,
    remaining: Vec<u8>,
    pn: Vec<u32>,
    dn: Vec<u32>,
    flags: Vec<u8>,
    expansions: usize,
    edges: usize,
    visits: u64,
    last_check: usize,
    safe_checks: usize,
    /// Per placement of the defender's pieces, the positions in the graph
    /// with that placement (certificate builder only, see the policy).
    defender_configs: Option<HashMap<u64, u32>>,
    /// The placement of the defender's pieces in the root (bitboards per
    /// kind), the reference of the stay-put policy.
    root_defender: [u64; 6],
    /// Each unsafe node's distance to the leak (certificate builder only).
    unsafe_level: Option<Vec<u16>>,
}

impl Graph {
    fn new(root: &Board, side_to_move_attacks: bool, log: bool, max_positions: usize, certificate: bool) -> Graph {
        let mut max_slots = INITIAL_SLOTS;
        while max_slots * MAX_LOAD_PERCENT / 100 < max_positions {
            max_slots *= 2;
        }
        let attacker_white = root.white_move == side_to_move_attacks;
        let mut g = Graph {
            attacker_white,
            log,
            store: Store::with_slots(INITIAL_SLOTS.min(max_slots)),
            max_slots,
            keys: Vec::new(),
            recs: Vec::new(),
            children: Vec::new(),
            parents: Vec::new(),
            status: Vec::new(),
            white: Vec::new(),
            level: Vec::new(),
            nmoves: Vec::new(),
            remaining: Vec::new(),
            pn: Vec::new(),
            dn: Vec::new(),
            flags: Vec::new(),
            expansions: 0,
            edges: 0,
            visits: 0,
            last_check: 0,
            safe_checks: 0,
            defender_configs: if certificate { Some(HashMap::new()) } else { None },
            root_defender: root.pieces[if attacker_white { 1 } else { 0 }],
            unsafe_level: if certificate { Some(Vec::new()) } else { None },
        };
        g.store.put(root.key, 0);
        g.add(root);
        g
    }

    fn defender_side(&self) -> usize {
        if self.attacker_white {
            1
        } else {
            0
        }
    }

    /// Hashes the placement of the defender's pieces.
    fn defender_config(&self, b: &Board) -> u64 {
        let mut h: u64 = 0x9E3779B97F4A7C15;
        for &bb in &b.pieces[self.defender_side()] {
            h = (h ^ bb).wrapping_mul(0xBF58476D1CE4E5B9);
            h ^= h >> 29;
        }
        h
    }

    /// The node index of a position.
    pub fn index(&self, b: &Board) -> Option<u32> {
        self.store.get(b.key).map(|v| v as u32)
    }

    /// The node index of a key, inserting it as the next node when it is
    /// new (the caller then calls `add`); grows the store when it is full.
    /// `ok` is false once the position budget is exhausted.
    fn get_or_put(&mut self, key: crate::tt::Key) -> (u32, bool, bool) {
        let next = self.recs.len() as u64;
        let (idx, is_new, ok) = self.store.get_or_put(key, next);
        if ok {
            return (idx as u32, is_new, true);
        }
        if self.store.slots() >= self.max_slots {
            return (0, false, false);
        }
        let mut grown = Store::with_slots((self.store.slots() * 4).min(self.max_slots));
        for (i, k) in self.keys.iter().enumerate() {
            grown.put(*k, i as u64);
        }
        self.store = grown;
        let (idx, is_new, ok) = self.store.get_or_put(key, next);
        (idx as u32, is_new, ok)
    }

    /// The node is a win for the attacker (attacker to move and winning, or
    /// defender to move and losing).
    pub fn attacker_won(&self, i: u32) -> bool {
        match self.status[i as usize] {
            ST_WIN => self.attacker_at(i),
            ST_LOSS => !self.attacker_at(i),
            _ => false,
        }
    }

    /// The node is decided against the attacker (draw or the attacker gets mated).
    fn defender_holds(&self, i: u32) -> bool {
        match self.status[i as usize] {
            ST_DRAW_T | ST_SAFE => true,
            ST_WIN => !self.attacker_at(i),
            ST_LOSS => self.attacker_at(i),
            _ => false,
        }
    }

    #[inline(always)]
    fn attacker_at(&self, i: u32) -> bool {
        self.white[i as usize] == self.attacker_white
    }

    /// Creates a node: terminal check (dead material, mate, stalemate) and
    /// the leaf numbers from its mobility.
    fn add(&mut self, b: &Board) -> u32 {
        let i = self.recs.len() as u32;
        self.recs.push(packed_fixed_record(b));
        self.keys.push(b.key);
        self.children.push(Vec::new());
        self.parents.push(Vec::new());
        self.white.push(b.white_move);
        self.level.push(0);
        self.flags.push(0);
        let attacker = b.white_move == self.attacker_white;
        let mut status = ST_UNKNOWN;
        let mut n = 0;
        if Material.probe(b).0 == Verdict::Draw {
            status = ST_DRAW_T;
        } else {
            let mut buf = new_buffer();
            n = b.gen_moves(&mut buf);
            if n == 0 {
                status = if b.in_check() { ST_LOSS } else { ST_DRAW_T };
            }
        }
        self.status.push(status);
        self.nmoves.push(n as u8);
        self.remaining.push(n as u8);
        if let Some(configs) = self.defender_configs.as_mut() {
            let h = {
                let mut h: u64 = 0x9E3779B97F4A7C15;
                let side = if self.attacker_white { 1 } else { 0 };
                for &bb in &b.pieces[side] {
                    h = (h ^ bb).wrapping_mul(0xBF58476D1CE4E5B9);
                    h ^= h >> 29;
                }
                h
            };
            *configs.entry(h).or_insert(0) += 1;
        }
        let (mut pn, mut dn) = (1u32, 1u32);
        if status == ST_LOSS && !attacker {
            pn = 0;
            dn = INF;
        } else if status != ST_UNKNOWN {
            pn = INF;
            dn = 0;
        } else if attacker {
            dn = self.leaf_number(n);
        } else {
            pn = self.leaf_number(n);
        }
        self.pn.push(pn);
        self.dn.push(dn);
        i
    }

    /// The mobility of a fresh leaf as a number: n moves, or on the log
    /// scale 1 + log2(n) rounded down.
    fn leaf_number(&self, n: usize) -> u32 {
        if self.log {
            32 - (n as u32).leading_zeros()
        } else {
            n as u32
        }
    }

    /// Adds two numbers: linear with saturation, or on the log scale.
    fn sum2(&self, a: u32, b: u32) -> u32 {
        if self.log {
            lsum(a, b)
        } else {
            sat_add(a, b)
        }
    }

    fn board(&self, i: u32) -> Board {
        PackedFixed::decode(&self.recs[i as usize]).0
    }

    /// Numbers of a child as seen from a node during the descent.
    fn child_numbers(&self, c: u32) -> (u32, u32) {
        if self.attacker_won(c) {
            (0, INF)
        } else if self.defender_holds(c) || self.flags[c as usize] & FL_ON_PATH != 0 {
            (INF, 0) // decided for the defender, or a repetition of the path
        } else {
            (self.pn[c as usize], self.dn[c as usize])
        }
    }

    /// Pushes a decided node to its parents: a win for the side to move
    /// takes one legal move from each parent (all gone = the parent loses),
    /// a loss makes every open parent win.
    fn propagate(&mut self, i: u32) {
        let mut queue = vec![i];
        while let Some(c) = queue.pop() {
            match self.status[c as usize] {
                ST_WIN => {
                    for pi in 0..self.parents[c as usize].len() {
                        let p = self.parents[c as usize][pi] as usize;
                        if self.status[p] != ST_UNKNOWN {
                            continue;
                        }
                        self.remaining[p] -= 1;
                        if self.remaining[p] == 0 {
                            self.status[p] = ST_LOSS;
                            queue.push(p as u32);
                        }
                    }
                }
                ST_LOSS => {
                    for pi in 0..self.parents[c as usize].len() {
                        let p = self.parents[c as usize][pi] as usize;
                        if self.status[p] == ST_UNKNOWN {
                            self.status[p] = ST_WIN;
                            queue.push(p as u32);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Gives every open node its leaf numbers again.
    fn reset_numbers(&mut self) {
        for i in 0..self.status.len() {
            if self.status[i] != ST_UNKNOWN {
                continue;
            }
            let n = self.leaf_number(self.nmoves[i] as usize);
            if self.attacker_at(i as u32) {
                self.pn[i] = 1;
                self.dn[i] = n;
            } else {
                self.pn[i] = n;
                self.dn[i] = 1;
            }
        }
    }

    /// Runs the fixed point: unsafe is the least fixed point from the won
    /// nodes and the unexpanded open nodes (an attacker node is unsafe if
    /// any child is, a defender node if all children are); every open node
    /// that is not unsafe becomes safe for good.
    #[allow(clippy::needless_range_loop)]
    fn safe_set(&mut self) {
        self.safe_checks += 1;
        self.last_check = self.expansions;
        let n = self.recs.len();
        let mut safe_children = vec![0u16; n];
        if let Some(l) = self.unsafe_level.as_mut() {
            l.clear();
            l.resize(n, 0);
        }
        let mut queue: Vec<u32> = Vec::new();
        for i in 0..n {
            self.flags[i] &= !FL_UNSAFE;
            let unsafe_ = if self.status[i] != ST_UNKNOWN {
                self.attacker_won(i as u32)
            } else if self.flags[i] & FL_EXPANDED == 0 {
                true
            } else {
                safe_children[i] = self.children[i].len() as u16;
                false
            };
            if unsafe_ {
                self.flags[i] |= FL_UNSAFE;
                queue.push(i as u32);
            }
        }
        // breadth-first, so that unsafe_level is the distance to the leak
        let mut head = 0;
        while head < queue.len() {
            let c = queue[head] as usize;
            head += 1;
            for pi in 0..self.parents[c].len() {
                let p = self.parents[c][pi] as usize;
                if self.flags[p] & FL_UNSAFE != 0 || self.status[p] != ST_UNKNOWN {
                    continue;
                }
                if !self.attacker_at(p as u32) {
                    safe_children[p] -= 1;
                    if safe_children[p] != 0 {
                        continue;
                    }
                }
                self.flags[p] |= FL_UNSAFE;
                if let Some(l) = self.unsafe_level.as_mut() {
                    l[p] = l[c] + 1;
                }
                queue.push(p as u32);
            }
        }
        for i in 0..n {
            if self.status[i] == ST_UNKNOWN && self.flags[i] & FL_UNSAFE == 0 {
                self.status[i] = ST_SAFE;
            }
        }
    }

    /// Counts the positions reachable from the root through
    /// defender-holding children only, the attacker with all moves, the
    /// defender with one (the draw certificate).
    fn reachable_safe(&self) -> usize {
        let mut seen = vec![false; self.recs.len()];
        let mut stack = vec![0u32];
        seen[0] = true;
        let mut count = 0;
        while let Some(i) = stack.pop() {
            count += 1;
            let attacker = self.attacker_at(i);
            for &c in &self.children[i as usize] {
                if !self.defender_holds(c) {
                    continue;
                }
                if !seen[c as usize] {
                    seen[c as usize] = true;
                    stack.push(c);
                }
                if !attacker {
                    break;
                }
            }
        }
        count
    }

    /// Recomputes win and loss levels from the mates (for the line).
    fn levels(&mut self) {
        let n = self.recs.len();
        let mut remaining = vec![0u8; n];
        let mut done = vec![false; n];
        let mut queue: Vec<u32> = Vec::new();
        for i in 0..n {
            remaining[i] = self.nmoves[i];
            self.level[i] = 0;
            if self.status[i] == ST_LOSS && self.nmoves[i] == 0 {
                done[i] = true;
                queue.push(i as u32);
            }
        }
        let mut lvl = 0u16;
        while !queue.is_empty() {
            let mut next = Vec::new();
            for &c in &queue {
                for &p in &self.parents[c as usize] {
                    let p = p as usize;
                    if done[p] {
                        continue;
                    }
                    if lvl.is_multiple_of(2) {
                        done[p] = true;
                        self.level[p] = lvl + 1;
                        next.push(p as u32);
                    } else {
                        remaining[p] -= 1;
                        if remaining[p] == 0 {
                            done[p] = true;
                            self.level[p] = lvl + 1;
                            next.push(p as u32);
                        }
                    }
                }
            }
            queue = next;
            lvl += 1;
        }
    }

    /// Follows the levels from the root: the attacker takes the child that
    /// loses fastest, the defender the child that wins slowest.
    fn line(&self, root: u32) -> Vec<Move> {
        let mut pv = Vec::new();
        let mut i = root;
        while pv.len() < 500 {
            let b = self.board(i);
            let mut buf = new_buffer();
            let n = b.gen_moves(&mut buf);
            if n == 0 {
                break;
            }
            let attacker_wins = self.attacker_won(i);
            let mut best: Option<(Move, u32, u16)> = None;
            for &m in &buf[..n] {
                let mut c = b;
                c.do_move(m);
                let Some(ci) = self.index(&c) else { continue };
                if !self.attacker_won(ci) {
                    continue;
                }
                let lv = self.level[ci as usize];
                let take = if self.attacker_at(i) == attacker_wins && self.attacker_at(i) {
                    best.is_none_or(|(_, _, bl)| lv < bl) // the attacker moves: shortest
                } else {
                    best.is_none_or(|(_, _, bl)| lv > bl)
                };
                if take {
                    best = Some((m, ci, lv));
                }
            }
            let Some((m, ci, _)) = best else { break };
            pv.push(m);
            i = ci;
        }
        pv
    }

    fn count_won(&self) -> usize {
        (0..self.recs.len() as u32).filter(|&i| self.attacker_won(i)).count()
    }

    fn fill_result(&mut self, r: &mut Result) {
        if self.attacker_won(0) {
            r.outcome = Outcome::Proven;
        } else if self.status[0] == ST_SAFE {
            r.outcome = Outcome::Disproven;
            r.safe = self.reachable_safe();
        }
        r.won = self.count_won();
        r.positions = self.recs.len();
        r.expansions = self.expansions;
        r.edges = self.edges;
        r.visits = self.visits;
        r.safe_checks = self.safe_checks;
        if r.outcome == Outcome::Proven {
            self.levels();
            r.win_plies = self.level[0] as usize;
            r.pv = self.line(0);
        }
    }
}

/// Answers one question. With `side_to_move_attacks` the root is an
/// attacker node ("the side to move forces a win"), otherwise a defender
/// node. Unless told otherwise it races the proof-number descent against
/// the certificate builder on two threads; the first decision wins and
/// stops the other.
pub fn solve(root: &Board, side_to_move_attacks: bool, opt: Options) -> Result {
    if opt.brute_force || opt.no_race {
        let brute = opt.brute_force;
        let mut r = on_big_stack(move || solve_pn(root, side_to_move_attacks, opt), root);
        r.method = if brute { "brute force" } else { "pn" };
        return r;
    }
    let stop_pn = Arc::new(AtomicBool::new(false));
    let stop_cert = Arc::new(AtomicBool::new(false));
    let mut cert_opt = opt.clone();
    cert_opt.race_stop = Some(stop_cert.clone());
    let mut pn_opt = opt;
    pn_opt.race_stop = Some(stop_pn.clone());
    let root_copy = *root;
    let (rp, rc) = std::thread::scope(|s| {
        let cert_stop_pn = stop_pn.clone();
        let cert = std::thread::Builder::new()
            .stack_size(THREAD_STACK)
            .spawn_scoped(s, move || {
                let mut r = solve_certificate(&root_copy, side_to_move_attacks, cert_opt);
                r.method = "certificate";
                cert_stop_pn.store(true, Ordering::Relaxed);
                r
            })
            .expect("certificate thread");
        let pn = std::thread::Builder::new()
            .stack_size(THREAD_STACK)
            .spawn_scoped(s, move || {
                let mut r = solve_pn(&root_copy, side_to_move_attacks, pn_opt);
                r.method = "pn";
                r
            })
            .expect("pn thread");
        let rp = pn.join().expect("pn thread panicked");
        stop_cert.store(true, Ordering::Relaxed);
        let rc = cert.join().expect("certificate thread panicked");
        (rp, rc)
    });
    if rp.outcome != Outcome::Undecided {
        return rp;
    }
    if rc.outcome != Outcome::Undecided {
        return rc;
    }
    let mut r = rp;
    r.aborted = r.aborted || rc.aborted;
    r.stuck = r.stuck || rc.stuck;
    r
}

/// Runs a search on a thread with a large stack (the descent recurses).
fn on_big_stack(f: impl FnOnce() -> Result + Send, _root: &Board) -> Result {
    std::thread::scope(|s| std::thread::Builder::new().stack_size(THREAD_STACK).spawn_scoped(s, f).expect("search thread").join().expect("search thread panicked"))
}

/// Answers one question with the proof-number descent.
fn solve_pn(root: &Board, side_to_move_attacks: bool, opt: Options) -> Result {
    let max_positions = if opt.max_positions == 0 { 1 << 26 } else { opt.max_positions };
    let g = Graph::new(root, side_to_move_attacks, opt.log_numbers, max_positions, false);
    let mut s = Searcher::new(g, opt);
    let mut r = Result::default();
    if s.opt.brute_force {
        let mut i = 0;
        while i < s.g.recs.len() && !s.aborted {
            if s.g.status[i] == ST_UNKNOWN && s.g.flags[i] & FL_EXPANDED == 0 {
                s.expand(i as u32);
            }
            i += 1;
        }
        if !s.aborted {
            s.g.safe_set();
        }
    } else {
        while !s.aborted {
            let (pn, dn) = s.mid(0, INF, INF, 0);
            if s.g.attacker_won(0) || s.g.status[0] == ST_SAFE {
                break;
            }
            if dn == 0 || pn == 0 {
                // the heuristic decided, the fixed point has to confirm it
                s.g.safe_set();
                if s.g.status[0] == ST_SAFE || s.g.attacker_won(0) {
                    break;
                }
                r.resets += 1;
                s.g.reset_numbers();
            }
        }
    }
    r.aborted = s.aborted;
    r.stuck = s.stuck;
    r.exhausts = s.exhausts;
    s.g.fill_result(&mut r);
    let line = format!("done: {} positions, {} expansions, {} edges, {} visits, {} won, {} fixed-point runs, {} resets, {}", s.g.recs.len(), s.g.expansions, s.g.edges, s.g.visits, r.won, s.g.safe_checks, r.resets, fmt_dur(s.start.elapsed()));
    s.report(&line);
    r
}

/// The search state around a graph.
struct Searcher {
    g: Graph,
    opt: Options,
    start: Instant,
    cap_n: u32,
    aborted: bool,
    stuck: bool,
    exhausts: usize,
}

impl Searcher {
    fn new(g: Graph, mut opt: Options) -> Searcher {
        if opt.progress_every == 0 {
            opt.progress_every = 1 << 20;
        }
        let cap_n = if opt.number_cap == 0 { DEFAULT_NUMBER_CAP } else { opt.number_cap };
        Searcher { g, opt, start: Instant::now(), cap_n, aborted: false, stuck: false, exhausts: 0 }
    }

    fn report(&self, line: &str) {
        if let Some(p) = &self.opt.progress {
            p(line);
        }
    }

    fn stop_requested(&self) -> bool {
        self.opt.stop.as_ref().is_some_and(|s| s.load(Ordering::Relaxed)) || self.opt.race_stop.as_ref().is_some_and(|s| s.load(Ordering::Relaxed))
    }

    /// Adds all children of a node and propagates wins.
    fn expand(&mut self, i: u32) {
        if self.g.expansions & 1023 == 0 && self.stop_requested() {
            self.aborted = true;
            return;
        }
        let g = &mut self.g;
        let b = g.board(i);
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        g.flags[i as usize] |= FL_EXPANDED;
        g.expansions += 1;
        g.children[i as usize] = Vec::with_capacity(n);
        for &m in &buf[..n] {
            let mut c = b;
            c.do_move(m);
            let (ci, is_new, ok) = g.get_or_put(c.key);
            if !ok {
                self.aborted = true;
                return;
            }
            if is_new {
                g.add(&c);
            }
            g.children[i as usize].push(ci);
            g.parents[ci as usize].push(i);
            g.edges += 1;
        }
        // wins through the new edges: a losing child wins the node, a
        // winning child is one legal move less for the node
        for ci in 0..g.children[i as usize].len() {
            let c = g.children[i as usize][ci] as usize;
            if g.status[c] == ST_LOSS && g.status[i as usize] == ST_UNKNOWN {
                g.status[i as usize] = ST_WIN;
            } else if g.status[c] == ST_WIN {
                g.remaining[i as usize] -= 1;
            }
        }
        if g.status[i as usize] == ST_UNKNOWN && g.remaining[i as usize] == 0 {
            g.status[i as usize] = ST_LOSS;
        }
        if g.status[i as usize] != ST_UNKNOWN {
            g.propagate(i);
        }
        if g.expansions.is_multiple_of(self.opt.progress_every) {
            let line = format!("{} positions, {} expansions, {} edges, {} visits, {}", g.recs.len(), g.expansions, g.edges, g.visits, fmt_dur(self.start.elapsed()));
            self.report(&line);
        }
        // the fixed point now and then: safe nodes are decided for good
        if !self.opt.brute_force && self.g.expansions - self.g.last_check >= 4096.max(self.g.recs.len() / 4) {
            self.g.safe_set();
            let line = format!("fixed point at {} expansions: {} positions, {}", self.g.expansions, self.g.recs.len(), fmt_dur(self.start.elapsed()));
            self.report(&line);
        }
    }

    /// Expands every undecided node reachable from c (brute force on the
    /// subgraph, each position once).
    fn exhaust(&mut self, c: u32) {
        let mut stack = vec![c];
        self.g.flags[c as usize] |= FL_REGION;
        let mut marked = vec![c];
        while let Some(i) = stack.pop() {
            if self.aborted {
                break;
            }
            if self.g.status[i as usize] != ST_UNKNOWN {
                continue;
            }
            if self.g.flags[i as usize] & FL_EXPANDED == 0 {
                self.expand(i);
                if self.aborted {
                    break;
                }
            }
            for ci in 0..self.g.children[i as usize].len() {
                let ch = self.g.children[i as usize][ci];
                if self.g.flags[ch as usize] & FL_REGION == 0 && self.g.status[ch as usize] == ST_UNKNOWN {
                    self.g.flags[ch as usize] |= FL_REGION;
                    marked.push(ch);
                    stack.push(ch);
                }
            }
        }
        for i in marked {
            self.g.flags[i as usize] &= !FL_REGION;
        }
    }

    /// Nagai's multiple iterative deepening on the graph: searches below
    /// node i until its proof number reaches th_pn or its disproof number
    /// reaches th_dn, and returns the numbers at that point.
    fn mid(&mut self, i: u32, th_pn: u32, th_dn: u32, depth: usize) -> (u32, u32) {
        self.g.visits += 1;
        if self.g.visits & (STOP_NODES - 1) == 0 && self.stop_requested() {
            self.aborted = true;
        }
        if self.g.visits & ((1 << 22) - 1) == 0 {
            let line = format!("{} visits at depth {}: {} positions, {} expansions, node {} pn {} dn {} thresholds {}/{}, {}", self.g.visits, depth, self.g.recs.len(), self.g.expansions, i, self.g.pn[i as usize], self.g.dn[i as usize], th_pn, th_dn, fmt_dur(self.start.elapsed()));
            self.report(&line);
        }
        if self.aborted || depth >= MAX_DEPTH {
            return (th_pn, th_dn);
        }
        if self.g.attacker_won(i) {
            return (0, INF);
        }
        if self.g.defender_holds(i) {
            return (INF, 0);
        }
        if self.g.flags[i as usize] & FL_EXPANDED == 0 {
            self.expand(i);
            if self.aborted {
                return (th_pn, th_dn);
            }
            if self.g.attacker_won(i) {
                return (0, INF);
            }
            if self.g.defender_holds(i) {
                return (INF, 0);
            }
        }
        let attacker = self.g.attacker_at(i);
        self.g.flags[i as usize] |= FL_ON_PATH;
        let children = self.g.children[i as usize].clone();
        // what a child returned from its descent overrides its stored
        // numbers for the rest of this loop
        let mut local = [(0u32, 0u32, false); MAX_MOVES];
        loop {
            let (mut best, mut second) = (INF, INF);
            let mut best_other = 0u32;
            let mut sum = 0u32;
            let mut bi = usize::MAX;
            for (ci, &c) in children.iter().enumerate() {
                let (cpn, cdn) = child_numbers_local(&self.g, &local, ci, c);
                let (k, other) = if attacker { (cpn, cdn) } else { (cdn, cpn) };
                sum = self.g.sum2(sum, other);
                if k < best || (k == best && bi != usize::MAX && other < best_other) {
                    second = best;
                    best = k;
                    bi = ci;
                    best_other = other;
                } else if k < second {
                    second = k;
                }
            }
            let (pn, dn) = if attacker { (best, sum) } else { (sum, best) };
            self.g.pn[i as usize] = clip(pn, self.cap_n);
            self.g.dn[i as usize] = clip(dn, self.cap_n);
            if pn >= th_pn || dn >= th_dn || bi == usize::MAX {
                self.g.flags[i as usize] &= !FL_ON_PATH;
                return (pn, dn);
            }
            let c = children[bi];
            if self.g.flags[c as usize] & FL_ON_PATH != 0 || self.g.attacker_won(c) || self.g.defender_holds(c) {
                // the best child is decided or a repetition: nothing to
                // search below on this path, so the parent must move on
                self.g.flags[i as usize] &= !FL_ON_PATH;
                return (pn.max(th_pn), dn.max(th_dn));
            }
            let (cpn, cdn) = child_numbers_local(&self.g, &local, bi, c);
            let (c_th_pn, c_th_dn);
            if self.g.log {
                let mut bound = sat_add(second, 1);
                if self.opt.epsilon >= 8 && second < INF {
                    bound = sat_add(second, 2);
                }
                // lsum is not associative: the threshold for the child's
                // other number is found with the very sum the node computes
                let th = if attacker { th_dn } else { th_pn };
                let sum_with = |v: u32| -> u32 {
                    let mut sum = 0u32;
                    for (ci, &c2) in children.iter().enumerate() {
                        let o = if ci != bi {
                            let (p2, d2) = child_numbers_local(&self.g, &local, ci, c2);
                            if attacker {
                                d2
                            } else {
                                p2
                            }
                        } else {
                            v
                        };
                        sum = lsum(sum, o);
                    }
                    sum
                };
                let other = lthreshold(th, sum_with);
                if attacker {
                    c_th_pn = th_pn.min(bound);
                    c_th_dn = other;
                } else {
                    c_th_dn = th_dn.min(bound);
                    c_th_pn = other;
                }
            } else {
                let mut bound = sat_add(second, 1);
                if self.opt.epsilon > 0 && second < INF {
                    bound = sat_add(second, (((second as u64) * self.opt.epsilon as u64) / 8).max(1) as u32);
                }
                if attacker {
                    c_th_pn = th_pn.min(bound);
                    c_th_dn = sat_sub(th_dn, sat_sub(dn, cdn));
                } else {
                    c_th_dn = th_dn.min(bound);
                    c_th_pn = sat_sub(th_pn, sat_sub(pn, cpn));
                }
            }
            let (rpn, rdn) = self.mid(c, c_th_pn, c_th_dn, depth + 1);
            if local[bi].2 && local[bi].0 == rpn && local[bi].1 == rdn && !self.aborted {
                // the child came back twice with the same numbers under the
                // same thresholds: the numbers have saturated and the
                // heuristic is blind here; brute force below the child
                self.exhausts += 1;
                let line = format!("saturated at node {} (depth {}, pn {} dn {}, thresholds {}/{}): child {} returned {}/{} twice under {}/{}, expanding its subgraph", i, depth, pn, dn, th_pn, th_dn, c, rpn, rdn, c_th_pn, c_th_dn);
                self.report(&line);
                self.exhaust(c);
                if !self.aborted {
                    self.g.safe_set();
                }
                if self.g.attacker_won(i) {
                    self.g.flags[i as usize] &= !FL_ON_PATH;
                    return (0, INF);
                }
                if self.g.defender_holds(i) {
                    self.g.flags[i as usize] &= !FL_ON_PATH;
                    return (INF, 0);
                }
                if !self.g.attacker_won(c) && !self.g.defender_holds(c) {
                    // cannot happen: a fully expanded subgraph is decided by the fixed point
                    self.aborted = true;
                    self.stuck = true;
                    self.g.flags[i as usize] &= !FL_ON_PATH;
                    return (pn, dn);
                }
                local[bi].2 = false;
                continue;
            }
            local[bi] = (rpn, rdn, true);
            if self.aborted {
                self.g.flags[i as usize] &= !FL_ON_PATH;
                return (pn, dn);
            }
            if self.g.attacker_won(i) {
                self.g.flags[i as usize] &= !FL_ON_PATH;
                return (0, INF);
            }
            if self.g.defender_holds(i) {
                self.g.flags[i as usize] &= !FL_ON_PATH;
                return (INF, 0);
            }
        }
    }
}

fn child_numbers_local(g: &Graph, local: &[(u32, u32, bool); MAX_MOVES], ci: usize, c: u32) -> (u32, u32) {
    if local[ci].2 {
        (local[ci].0, local[ci].1)
    } else {
        g.child_numbers(c)
    }
}

pub(crate) use certificate::solve_certificate;

fn clip(x: u32, cap_n: u32) -> u32 {
    if x >= INF {
        INF
    } else {
        x.min(cap_n)
    }
}

/// The sum on the log scale: the larger number, plus one when the two are
/// within a factor of two; 0 (a proof) adds nothing, INF absorbs.
fn lsum(a: u32, b: u32) -> u32 {
    if a >= INF || b >= INF {
        return INF;
    }
    if a == 0 {
        return b;
    }
    if b == 0 {
        return a;
    }
    let (hi, lo) = (a.max(b), a.min(b));
    if hi - lo <= 1 {
        hi + 1
    } else {
        hi
    }
}

/// The smallest number v with sum_with(v) >= th (sum_with must be monotone).
fn lthreshold(th: u32, sum_with: impl Fn(u32) -> u32) -> u32 {
    if th >= INF {
        return INF;
    }
    let mut v = th.saturating_sub(2);
    while v < INF && sum_with(v) < th {
        v += 1;
    }
    v
}

/// Adds two node numbers, INF absorbing, finite sums below INF.
fn sat_add(a: u32, b: u32) -> u32 {
    if a >= INF || b >= INF {
        INF
    } else {
        (a + b).min(INF - 1)
    }
}

fn sat_sub(a: u32, b: u32) -> u32 {
    if a >= INF {
        INF
    } else {
        a.saturating_sub(b)
    }
}

fn fmt_dur(d: Duration) -> String {
    if d < Duration::from_secs(1) {
        format!("{} ms", d.as_millis())
    } else {
        format!("{:.2} s", d.as_secs_f64())
    }
}
