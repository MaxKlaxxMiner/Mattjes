//! The WDL search on an explicit graph (milestone 7, step 2,
//! docs/m7-wdl-search.md). Direct port of `mattjesGo/wdllist`, see there
//! for the full explanation: positions and edges are stored, wins are
//! propagated backwards with counters as they appear, a draw is the
//! greatest fixed point "the defender always has a move that keeps the
//! attacker away from a win", and two searchers race per question: the
//! proof-number descent (good at wins) and the certificate builder (good at
//! draws with a small safe set).

mod certificate;
mod chase;

use chase::{ChaseResult, CHASE_MAX_DEPTH, CHASE_NODE_BUDGET, CH_HOLD, CH_UNKNOWN};

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::bitboard::{packed_fixed_record, Board, Codec, PackedFixed};
use crate::chess::{new_buffer, Move, MAX_MOVES};
use crate::mateab::{Material, Oracle, Tables, Verdict};
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
    /// Certificate builder: check chains started (chase.rs).
    pub chases: usize,
    /// Nodes the chase visited.
    pub chase_nodes: u64,
    /// Bytes of the graph's arrays and store, by capacity (the memory accounting).
    pub bytes: usize,
    pub aborted: bool,
    /// Aborted because the descent made no progress (a bug, never a result).
    pub stuck: bool,
}

/// The progress callback: shared by the two racing searchers.
pub type Progress = Arc<dyn Fn(&str) + Send + Sync>;

/// Options of the search.
#[derive(Clone, Default)]
pub struct Options<'a> {
    /// The endgame tables as oracle: every capture into a table is a
    /// terminal, which a draw certificate needs wherever the attacker can
    /// give up material (the certificate builder cannot prove the
    /// defender's wins itself). None = dead material only.
    pub oracle: Option<Tables<'a>>,
    /// Expands the whole reachable graph, no guidance.
    pub brute_force: bool,
    /// Runs the proof-number descent alone (no certificate builder).
    pub no_race: bool,
    /// Runs the certificate builder without the perpetual-check chase
    /// (chase.rs), the measured alternative.
    pub no_chase: bool,
    /// Runs the certificate builder alone (no proof-number descent racing
    /// it), for measuring the builder.
    pub no_descent: bool,
    /// Adds Patricia's network to the certificate builder's policy: the
    /// defender prefers the moves after which the attacker's evaluation is
    /// lowest, net_weight points per centipawn (0 = off; nnue, one
    /// accumulator per defender node, incremental per move); the measured
    /// experiment.
    pub net_weight: i32,
    /// Initialises the proof numbers of new nodes in the descent from
    /// Patricia's network (bias_numbers: an attacker the net sees ahead
    /// gets a small proof number and a large disproof number, and the
    /// other way round); the measured experiment. 0 = off, 1 = one doubling
    /// per 150 cp up to three, 2 = one per 100 cp up to five.
    pub net_init: u8,
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

/// The polling interval of the stop flags inside the fixed point, in nodes
/// scanned or dequeued (a power of two).
pub const STOP_POLL: usize = 1 << 16;

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

/// An entry of an edge arena: the node at the other end and the next entry
/// of the same list (NO_EDGE at the end).
#[derive(Clone, Copy)]
pub(super) struct Edge {
    node: u32,
    next: u32,
}

pub(super) const NO_EDGE: u32 = u32::MAX;

const FL_EXPANDED: u8 = 1; // children added
const FL_ON_PATH: u8 = 2; // on the current descent path
const FL_UNSAFE: u8 = 4; // scratch flag of the fixed point
const FL_REGION: u8 = 8; // scratch flag of exhaust and the closure

type Record = [u8; PackedFixed::MAX_BYTES];

/// The explicit position graph of one question.
pub struct Graph<'a> {
    /// Decides positions on sight (tables), None = dead material only.
    oracle: Option<Tables<'a>>,
    attacker_white: bool,
    log: bool,
    store: Store,
    max_slots: usize,
    /// The position budget; an expansion starts only below it.
    max_positions: usize,
    /// The positions (32 bytes each, the key is recomputed from them).
    recs: Vec<Record>,
    /// The edges live in two arenas of singly linked lists (Edge): the
    /// children of a node in insertion order (first_child to last_child),
    /// the parents newest first (first_parent); 12 bytes per node and 16
    /// per edge, where a Vec per list cost 48 bytes of headers per node
    /// plus the growth slack (docs/m7-wdl-search.md); nchildren counts
    /// the children.
    cedges: Vec<Edge>,
    pedges: Vec<Edge>,
    first_child: Vec<u32>,
    last_child: Vec<u32>,
    first_parent: Vec<u32>,
    nchildren: Vec<u8>,
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
    /// The chase memo per node (certificate builder only, CH_* in
    /// chase.rs); chase_depth the depth on the path, the depth of the
    /// ancestor a conditional hold relies on, or the depth a cut node had
    /// left; chase_quiet the quiet-move budget a failure or cut was tried
    /// with; chase_visits the node count.
    chase: Vec<u8>,
    chase_depth: Vec<u16>,
    chase_quiet: Vec<u8>,
    chase_visits: u64,
    /// Receives the certificate's shape line (diagnostics).
    shape: Option<Progress>,
    /// Options.net_weight for the policy (certificate builder only).
    net_weight: i32,
}

impl<'a> Graph<'a> {
    fn new(root: &Board, side_to_move_attacks: bool, log: bool, max_positions: usize, certificate: bool, oracle: Option<Tables<'a>>) -> Graph<'a> {
        let attacker_white = root.white_move == side_to_move_attacks;
        let mut g = Graph {
            oracle,
            attacker_white,
            log,
            store: Store::with_slots(INITIAL_SLOTS),
            max_slots: INITIAL_SLOTS,
            max_positions: 0,
            recs: Vec::new(),
            cedges: Vec::new(),
            pedges: Vec::new(),
            first_child: Vec::new(),
            last_child: Vec::new(),
            first_parent: Vec::new(),
            nchildren: Vec::new(),
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
            chase: Vec::new(),
            chase_depth: Vec::new(),
            chase_quiet: Vec::new(),
            chase_visits: 0,
            shape: None,
            net_weight: 0,
        };
        g.set_budget(max_positions);
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
    /// Sets the position budget (raised between the runs of a Solver). The
    /// store may grow to hold the budget plus one full expansion, so that
    /// an expansion started below the budget never finds it full: a node
    /// is either expanded completely or not at all, which a resumed run
    /// relies on.
    fn set_budget(&mut self, max_positions: usize) {
        let max_positions = if max_positions == 0 { 1 << 26 } else { max_positions };
        self.max_positions = max_positions;
        self.max_slots = self.max_slots.max(INITIAL_SLOTS);
        while self.max_slots * MAX_LOAD_PERCENT / 100 < max_positions + MAX_MOVES {
            self.max_slots *= 2;
        }
    }

    /// The oracle's verdict for the side to move (tables, or dead material).
    fn probe(&self, b: &Board) -> Verdict {
        match self.oracle {
            Some(t) => t.probe(b).0,
            None => Material.probe(b).0,
        }
    }

    /// The position budget is used up, no further expansion starts.
    fn full(&self) -> bool {
        self.recs.len() >= self.max_positions
    }

    fn get_or_put(&mut self, key: crate::tt::Key) -> (u32, bool, bool) {
        let next = self.recs.len() as u64;
        let (idx, is_new, ok) = self.store.get_or_put(key, next);
        if ok {
            return (idx as u32, is_new, true);
        }
        if self.store.slots() >= self.max_slots {
            return (0, false, false);
        }
        // doubling (a fourfold jump left the store four times the graph
        // right after it: 66 bytes per position measured), the keys
        // rehashed from the records instead of a 16-byte copy per node
        let mut grown = Store::with_slots((self.store.slots() * 2).min(self.max_slots));
        for i in 0..self.recs.len() {
            grown.put(self.board(i as u32).key, i as u64);
        }
        self.store = grown;
        let (idx, is_new, ok) = self.store.get_or_put(key, next);
        (idx as u32, is_new, ok)
    }

    /// Appends the edge i -> c: c to the children of i (insertion order), i
    /// to the parents of c (newest first).
    fn add_edge(&mut self, i: u32, c: u32) {
        let e = self.cedges.len() as u32;
        self.cedges.push(Edge { node: c, next: NO_EDGE });
        let iu = i as usize;
        if self.last_child[iu] == NO_EDGE {
            self.first_child[iu] = e;
        } else {
            self.cedges[self.last_child[iu] as usize].next = e;
        }
        self.last_child[iu] = e;
        self.nchildren[iu] += 1;
        let p = self.pedges.len() as u32;
        self.pedges.push(Edge { node: i, next: self.first_parent[c as usize] });
        self.first_parent[c as usize] = p;
        self.edges += 1;
    }

    /// Copies the children of node i into buf and returns them.
    fn children_of<'b>(&self, i: u32, buf: &'b mut [u32; MAX_MOVES]) -> &'b [u32] {
        let mut n = 0;
        let mut e = self.first_child[i as usize];
        while e != NO_EDGE {
            buf[n] = self.cedges[e as usize].node;
            n += 1;
            e = self.cedges[e as usize].next;
        }
        &buf[..n]
    }

    /// The memory of the graph by the capacities of its arrays and the
    /// store's slots (what the process holds for it, growth slack included).
    pub fn bytes(&self) -> usize {
        let mut n = self.recs.capacity() * std::mem::size_of::<Record>();
        n += (self.cedges.capacity() + self.pedges.capacity()) * 8;
        n += (self.first_child.capacity() + self.last_child.capacity() + self.first_parent.capacity()) * 4;
        n += self.nchildren.capacity() + self.status.capacity() + self.white.capacity() + self.nmoves.capacity() + self.remaining.capacity() + self.flags.capacity();
        n += self.level.capacity() * 2 + (self.pn.capacity() + self.dn.capacity()) * 4;
        n += self.unsafe_level.as_ref().map_or(0, |l| l.capacity() * 2) + self.chase.capacity() + self.chase_depth.capacity() * 2 + self.chase_quiet.capacity();
        n + self.store.bytes()
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
        self.first_child.push(NO_EDGE);
        self.last_child.push(NO_EDGE);
        self.first_parent.push(NO_EDGE);
        self.nchildren.push(0);
        self.white.push(b.white_move);
        self.level.push(0);
        self.flags.push(0);
        let attacker = b.white_move == self.attacker_white;
        let mut status = ST_UNKNOWN;
        let mut n = 0;
        let attacker_side = if self.attacker_white { 0 } else { 1 };
        // a bare king never gives check, so the attacker can never mate: a
        // terminal hold, sound without any search (dead material is the
        // general FIDE 5.2.2 case, this one is the attacker's half of it)
        let bare_attacker = b.by_color[attacker_side] == b.pieces[attacker_side][0];
        // the oracle (tables: win, loss or draw for the side to move; without
        // tables only dead material) decides a node on sight, it is never
        // expanded (nmoves 0, which also seeds the win levels)
        let v = self.probe(b);
        if v == Verdict::Win {
            status = ST_WIN;
        } else if v == Verdict::Loss {
            status = ST_LOSS;
        } else if v == Verdict::Draw || bare_attacker {
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
            self.chase.push(CH_UNKNOWN);
            self.chase_depth.push(0);
            self.chase_quiet.push(0);
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
        if self.defender_configs.is_none() {
            // the descent's numbers; the certificate builder's graph has none
            self.pn.push(pn);
            self.dn.push(dn);
        }
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
                    let mut e = self.first_parent[c as usize];
                    while e != NO_EDGE {
                        let p = self.pedges[e as usize].node as usize;
                        e = self.pedges[e as usize].next;
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
                    let mut e = self.first_parent[c as usize];
                    while e != NO_EDGE {
                        let p = self.pedges[e as usize].node as usize;
                        e = self.pedges[e as usize].next;
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
    /// The stop flags (None = none) are polled every STOP_POLL steps: a
    /// stopped run marks nothing (the scratch flags are reset by the next
    /// run) and returns true; with 16 Mi positions a fixed point takes
    /// seconds, and a GUI's stop waited for it.
    #[allow(clippy::needless_range_loop)]
    fn safe_set(&mut self, stop: Option<&AtomicBool>, race_stop: Option<&AtomicBool>) -> bool {
        self.safe_checks += 1;
        self.last_check = self.expansions;
        let n = self.recs.len();
        let mut safe_children = vec![0u16; n];
        if let Some(l) = self.unsafe_level.as_mut() {
            l.clear();
            l.resize(n, 0);
        }
        let poll = |step: usize| step & (STOP_POLL - 1) == 0 && (stop.is_some_and(|s| s.load(Ordering::Relaxed)) || race_stop.is_some_and(|s| s.load(Ordering::Relaxed)));
        let mut queue: Vec<u32> = Vec::new();
        for i in 0..n {
            if poll(i) {
                return true;
            }
            self.flags[i] &= !FL_UNSAFE;
            let unsafe_ = if self.status[i] != ST_UNKNOWN {
                self.attacker_won(i as u32)
            } else if self.flags[i] & FL_EXPANDED == 0 {
                true
            } else {
                safe_children[i] = self.nchildren[i] as u16;
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
            if poll(head) {
                return true;
            }
            let c = queue[head] as usize;
            head += 1;
            let mut e = self.first_parent[c];
            while e != NO_EDGE {
                let p = self.pedges[e as usize].node as usize;
                e = self.pedges[e as usize].next;
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
        false
    }

    /// Counts the positions reachable from the root through
    /// defender-holding children only, the attacker with all moves, the
    /// defender with one (the draw certificate), and reports the
    /// certificate's shape.
    fn reachable_safe(&self, from: u32) -> usize {
        let mut seen = vec![false; self.recs.len()];
        let mut stack = vec![from];
        seen[from as usize] = true;
        let mut count = 0;
        let mut king_hist = [0usize; 64];
        let (attacker_side, defender_side) = if self.attacker_white { (0, 1) } else { (1, 0) };
        let mut placements: std::collections::HashSet<[u64; 6]> = std::collections::HashSet::new();
        let mut defender_placements: std::collections::HashSet<[u64; 6]> = std::collections::HashSet::new();
        while let Some(i) = stack.pop() {
            count += 1;
            if count <= 1 << 20 && self.shape.is_some() {
                let b = self.board(i);
                king_hist[b.pieces[attacker_side][0].trailing_zeros() as usize] += 1;
                placements.insert(b.pieces[attacker_side]);
                defender_placements.insert(b.pieces[defender_side]);
            }
            let attacker = self.attacker_at(i);
            let mut e = self.first_child[i as usize];
            while e != NO_EDGE {
                let c = self.cedges[e as usize].node;
                e = self.cedges[e as usize].next;
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
        if let Some(shape) = &self.shape {
            let squares = king_hist.iter().filter(|&&n| n > 0).count();
            shape(&format!("certificate: {} positions, attacker king on {} squares, {} attacker placements, {} defender placements", count, squares, placements.len(), defender_placements.len()));
        }
        count
    }

    /// Recomputes win and loss levels from the mates (for the line).
    fn levels(&mut self) {
        let n = self.recs.len();
        let mut remaining = vec![0u8; n];
        let mut done = vec![false; n];
        let mut queue: Vec<u32> = Vec::new();
        let mut win_seeds: Vec<u32> = Vec::new();
        for i in 0..n {
            remaining[i] = self.nmoves[i];
            self.level[i] = 0;
            if self.nmoves[i] != 0 {
                continue;
            }
            if self.status[i] == ST_LOSS {
                // mate, or a table loss: level 0
                done[i] = true;
                queue.push(i as u32);
            } else if self.status[i] == ST_WIN {
                // a table win: level 1, joins the first wave of wins
                done[i] = true;
                self.level[i] = 1;
                win_seeds.push(i as u32);
            }
        }
        let mut lvl = 0u16;
        while !queue.is_empty() || (lvl == 0 && !win_seeds.is_empty()) {
            let mut next: Vec<u32> = if lvl == 0 { std::mem::take(&mut win_seeds) } else { Vec::new() };
            for &c in &queue {
                let mut e = self.first_parent[c as usize];
                while e != NO_EDGE {
                    let p = self.pedges[e as usize].node as usize;
                    e = self.pedges[e as usize].next;
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

    /// Reads the outcome of the question node n and the graph's counters
    /// into r.
    fn fill_result(&mut self, r: &mut Result, n: u32) {
        if self.attacker_won(n) {
            r.outcome = Outcome::Proven;
        } else if self.defender_holds(n) {
            r.outcome = Outcome::Disproven;
            r.safe = self.reachable_safe(n);
        }
        r.won = self.count_won();
        r.positions = self.recs.len();
        r.expansions = self.expansions;
        r.edges = self.edges;
        r.visits = self.visits;
        r.safe_checks = self.safe_checks;
        r.chase_nodes = self.chase_visits;
        r.bytes = self.bytes();
        if r.outcome == Outcome::Proven {
            self.levels();
            r.win_plies = self.level[n as usize] as usize;
            r.pv = self.line(n);
        }
    }
}

/// Answers one question. With `side_to_move_attacks` the root is an
/// attacker node ("the side to move forces a win"), otherwise a defender
/// node. Unless told otherwise it races the proof-number descent against
/// the certificate builder on two threads; the first decision wins and
/// stops the other. One run of a Solver with `Options::max_positions`.
pub fn solve(root: &Board, side_to_move_attacks: bool, opt: Options<'_>) -> Result {
    let extra = if opt.max_positions == 0 { 1 << 26 } else { opt.max_positions };
    Solver::new(root, side_to_move_attacks, opt).run(extra)
}

/// Answers questions on one shared pair of graphs (the descent's and the
/// certificate builder's), rooted at one position: the question node is
/// the root itself (`run`) or the position after a root move
/// (`run_move`). Every run continues the race until the question is
/// decided, the stop flag is set or each graph has grown by the given
/// number of positions (aborted, call again), and keeps the graphs, so
/// that the root moves of a position share their transpositions and the
/// wins and safe sets found for one move, and a driver can share its time
/// over them without restarting anything. `Options::max_positions` caps
/// each graph.
pub struct Solver<'a> {
    root: Board,
    opt: Options<'a>,
    /// The proof-number descent.
    pn: Searcher<'a>,
    /// The certificate builder (None without the race).
    cert: Option<Searcher<'a>>,
}

impl<'a> Solver<'a> {
    /// Prepares the graphs (they hold only the root). With
    /// `side_to_move_attacks` the side to move at the root is the attacker.
    pub fn new(root: &Board, side_to_move_attacks: bool, opt: Options<'a>) -> Solver<'a> {
        let pn = Searcher::new_pn(root, side_to_move_attacks, opt.clone());
        let cert = if opt.brute_force || opt.no_race { None } else { Some(Searcher::new_cert(root, side_to_move_attacks, opt.clone())) };
        Solver { root: *root, opt, pn, cert }
    }

    /// The number of positions in both graphs (the memory).
    pub fn positions(&self) -> usize {
        self.pn.g.recs.len() + self.cert.as_ref().map_or(0, |s| s.g.recs.len())
    }

    /// Continues the question of the root itself ("the attacker forces a
    /// win from the root"); `extra` is the number of positions each graph
    /// may add. The result's counts are those of the deciding graph, or of
    /// the descent's graph while undecided.
    pub fn run(&mut self, extra: usize) -> Result {
        let root = self.root;
        self.run_node(&root, extra)
    }

    /// Continues the question of the position after a root move (the root
    /// is expanded in each graph on the first such question).
    pub fn run_move(&mut self, m: Move, extra: usize) -> Result {
        let mut c = self.root;
        c.do_move(m);
        self.run_node(&c, extra)
    }

    fn run_node(&mut self, q: &Board, extra: usize) -> Result {
        let q = *q;
        let Solver { pn, cert, opt, .. } = self;
        let Some(cert) = cert.as_mut() else {
            let brute = opt.brute_force;
            let mut r = on_big_stack(move || pn.run_pn(&q, extra));
            r.method = if brute { "brute force" } else { "pn" };
            return r;
        };
        if opt.no_descent {
            cert.opt.stop = opt.stop.clone();
            let mut r = on_big_stack(move || cert.run_cert(&q, extra));
            r.method = "certificate";
            return r;
        }
        let stop_pn = Arc::new(AtomicBool::new(false));
        let stop_cert = Arc::new(AtomicBool::new(false));
        cert.opt.race_stop = Some(stop_cert.clone());
        pn.opt.race_stop = Some(stop_pn.clone());
        let (rp, rc) = std::thread::scope(|s| {
            let cert_stop_pn = stop_pn.clone();
            let cert = std::thread::Builder::new()
                .stack_size(THREAD_STACK)
                .spawn_scoped(s, move || {
                    let mut r = cert.run_cert(&q, extra);
                    r.method = "certificate";
                    cert_stop_pn.store(true, Ordering::Relaxed);
                    r
                })
                .expect("certificate thread");
            let pn = std::thread::Builder::new()
                .stack_size(THREAD_STACK)
                .spawn_scoped(s, move || {
                    let mut r = pn.run_pn(&q, extra);
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
            rp
        } else if rc.outcome != Outcome::Undecided {
            rc
        } else {
            let mut r = rp;
            r.aborted = r.aborted || rc.aborted;
            r.stuck = r.stuck || rc.stuck;
            r
        }
    }
}

impl<'a> Searcher<'a> {
    /// Starts a run: the budget is the graph's size plus `extra`, capped by
    /// `Options::max_positions`.
    fn begin(&mut self, extra: usize) {
        self.aborted = false;
        let cap = if self.opt.max_positions == 0 { 1 << 26 } else { self.opt.max_positions };
        self.g.set_budget((self.g.recs.len() + extra).min(cap).max(1));
    }

    /// Finds the question node: the root, or a child of the root after a
    /// root move (the root is expanded on the first such question; None
    /// when that expansion was cut off by the stop flag or the budget).
    fn locate(&mut self, q: &Board) -> Option<u32> {
        if q.key == self.g.board(0).key {
            return Some(0);
        }
        if self.g.flags[0] & FL_EXPANDED == 0 {
            self.expand(0);
            if self.aborted {
                return None;
            }
        }
        self.g.index(q)
    }
}

/// Runs a search on a thread with a large stack (the descent recurses).
fn on_big_stack(f: impl FnOnce() -> Result + Send) -> Result {
    std::thread::scope(|s| std::thread::Builder::new().stack_size(THREAD_STACK).spawn_scoped(s, f).expect("search thread").join().expect("search thread panicked"))
}

impl<'a> Searcher<'a> {
    /// Sets up the proof-number descent of a question.
    fn new_pn(root: &Board, side_to_move_attacks: bool, opt: Options<'a>) -> Searcher<'a> {
        let g = Graph::new(root, side_to_move_attacks, opt.log_numbers, opt.max_positions, false, opt.oracle);
        Searcher::new(g, opt)
    }
}

impl<'a> Searcher<'a> {
/// Continues the proof-number descent for the question node q.
fn run_pn(&mut self, q: &Board, extra: usize) -> Result {
    let s = self;
    s.begin(extra);
    let mut r = Result::default();
    let Some(n) = s.locate(q) else {
        r.aborted = true;
        r.positions = s.g.recs.len();
        return r;
    };
    if s.opt.brute_force {
        let mut i = 0;
        while i < s.g.recs.len() && !s.aborted {
            if s.g.status[i] == ST_UNKNOWN && s.g.flags[i] & FL_EXPANDED == 0 {
                s.expand(i as u32);
            }
            i += 1;
        }
        if !s.aborted && s.fixed_point() {
            s.aborted = true;
        }
    } else {
        while !s.aborted {
            let (pn, dn) = s.mid(n, INF, INF, 0);
            if s.g.attacker_won(n) || s.g.defender_holds(n) {
                break;
            }
            if dn == 0 || pn == 0 {
                // the heuristic decided, the fixed point has to confirm it
                if s.fixed_point() {
                    s.aborted = true;
                    break;
                }
                if s.g.defender_holds(n) || s.g.attacker_won(n) {
                    break;
                }
                s.resets += 1;
                s.g.reset_numbers();
            }
        }
    }
    r.stuck = s.stuck;
    r.exhausts = s.exhausts;
    r.resets = s.resets;
    s.g.fill_result(&mut r, n);
    r.aborted = s.aborted && r.outcome == Outcome::Undecided;
    let line = format!("done: {} positions, {} expansions, {} edges, {} visits, {} won, {} fixed-point runs, {} resets, {}", s.g.recs.len(), s.g.expansions, s.g.edges, s.g.visits, r.won, s.g.safe_checks, r.resets, fmt_dur(s.start.elapsed()));
    s.report(&line);
    r
}
}

/// The search state around a graph.
pub(crate) struct Searcher<'a> {
    g: Graph<'a>,
    opt: Options<'a>,
    start: Instant,
    cap_n: u32,
    aborted: bool,
    stuck: bool,
    exhausts: usize,
    /// Descent: heuristic disproofs the fixed point rejected.
    resets: usize,
    /// Certificate builder: widening rounds so far.
    rounds: usize,
    /// Certificate builder: the nodes the next closure starts from.
    frontier: Vec<u32>,
    /// Certificate builder: check chains started.
    chases: usize,
    /// cond_lists[d] (chase) lists the nodes whose hold relies on the path
    /// node at depth d, settled when that node finishes.
    cond_lists: Vec<Vec<u32>>,
    /// The current depth cap of the chase.
    chase_cap: usize,
    /// Chase node count at which a budgeted chase is cut (0 = none).
    chase_limit: u64,
    /// Chase at policy nodes too (the defender is the weaker side).
    chase_nodes: bool,
    chase_fails: usize,
    chase_cuts: usize,
    chase_holds: usize,
    chase_cond: usize,
    /// Quiet moves the chase tried.
    quiet_moves: usize,
}

impl<'a> Searcher<'a> {
    fn new(g: Graph<'a>, mut opt: Options<'a>) -> Searcher<'a> {
        if opt.progress_every == 0 {
            opt.progress_every = 1 << 20;
        }
        let cap_n = if opt.number_cap == 0 { DEFAULT_NUMBER_CAP } else { opt.number_cap };
        Searcher {
            g,
            opt,
            start: Instant::now(),
            cap_n,
            aborted: false,
            stuck: false,
            exhausts: 0,
            resets: 0,
            rounds: 0,
            frontier: vec![0],
            chases: 0,
            cond_lists: Vec::new(),
            chase_cap: 0,
            chase_limit: 0,
            chase_nodes: false,
            chase_fails: 0,
            chase_cuts: 0,
            chase_holds: 0,
            chase_cond: 0,
            quiet_moves: 0,
        }
    }

    fn report(&self, line: &str) {
        if let Some(p) = &self.opt.progress {
            p(line);
        }
    }

    fn stop_requested(&self) -> bool {
        self.opt.stop.as_ref().is_some_and(|s| s.load(Ordering::Relaxed)) || self.opt.race_stop.as_ref().is_some_and(|s| s.load(Ordering::Relaxed))
    }

    /// Runs the fixed point with the stop flags; true when it was stopped
    /// (nothing marked).
    fn fixed_point(&mut self) -> bool {
        let stop = self.opt.stop.clone();
        let race_stop = self.opt.race_stop.clone();
        self.g.safe_set(stop.as_deref(), race_stop.as_deref())
    }

    /// Adds all children of a node and propagates wins. It aborts (the
    /// stop flag, the position budget) before touching the node, never in
    /// the middle of it.
    fn expand(&mut self, i: u32) {
        if self.g.full() || self.g.expansions & 1023 == 0 && self.stop_requested() {
            self.aborted = true;
            return;
        }
        let g = &mut self.g;
        let b = g.board(i);
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        g.flags[i as usize] |= FL_EXPANDED;
        g.expansions += 1;
        let net = if self.opt.net_init > 0 && g.defender_configs.is_none() { Some(crate::nnue::load(crate::nnue::phase_of(&b))) } else { None };
        let mut acc = crate::nnue::Accumulator::default();
        if let Some(net) = net {
            acc.refresh(net, &b);
        }
        // (child index, the attacker's view of it, fresh = new and open)
        let mut children: Vec<(u32, i32, bool)> = Vec::with_capacity(n);
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
            let mut eval = 0;
            if let Some(net) = net {
                let d = crate::nnue::Delta::of(&b, m);
                let mut child = crate::nnue::Accumulator::default();
                child.update(net, &acc, &d);
                eval = child.evaluate(net, c.white_move); // for the side to move in the child
                if !g.attacker_at(ci) {
                    eval = -eval;
                }
            }
            children.push((ci, eval, is_new && g.status[ci as usize] == ST_UNKNOWN));
            g.add_edge(i, ci);
        }
        if net.is_some() {
            // (a rank among the siblings instead of the absolute view was
            // measured and dropped: KRR-KN 116 thousand expansions instead
            // of 5 thousand, docs/nnue-patricia.md)
            for &(ci, eval, fresh) in &children {
                if fresh {
                    let (pn, dn) = bias_numbers(g.pn[ci as usize], g.dn[ci as usize], eval, self.opt.net_init);
                    g.pn[ci as usize] = pn;
                    g.dn[ci as usize] = dn;
                }
            }
        }
        // wins through the new edges: a losing child wins the node, a
        // winning child is one legal move less for the node
        let mut e = g.first_child[i as usize];
        while e != NO_EDGE {
            let c = g.cedges[e as usize].node as usize;
            e = g.cedges[e as usize].next;
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
            let fp_start = Instant::now();
            if self.fixed_point() {
                self.aborted = true;
                return;
            }
            let line = format!("fixed point at {} expansions: {} positions, took {}, {}", self.g.expansions, self.g.recs.len(), fmt_dur(fp_start.elapsed()), fmt_dur(self.start.elapsed()));
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
            let mut e = self.g.first_child[i as usize];
            while e != NO_EDGE {
                let ch = self.g.cedges[e as usize].node;
                e = self.g.cedges[e as usize].next;
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
            // a threshold is never a decision: an infinite one (the root's,
            // or passed down along single children) returned as a number
            // would make the parent's sum infinite, and an infinite number
            // at the root ends every descent at once without a decision
            // (seen on KRR-KN after Nh2: 25 million visits per five
            // seconds, no expansion)
            return (finite(th_pn), finite(th_dn));
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
                return (finite(th_pn), finite(th_dn));
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
        let mut child_buf = [0u32; MAX_MOVES];
        let children = self.g.children_of(i, &mut child_buf);
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
                return (finite(pn.max(th_pn)), finite(dn.max(th_dn)));
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
                if !self.aborted && self.fixed_point() {
                    self.aborted = true;
                }
                if self.g.attacker_won(i) {
                    self.g.flags[i as usize] &= !FL_ON_PATH;
                    return (0, INF);
                }
                if self.g.defender_holds(i) {
                    self.g.flags[i as usize] &= !FL_ON_PATH;
                    return (INF, 0);
                }
                if self.aborted {
                    // the budget or the stop flag ended the exhaust: not a decision, not a bug
                    self.g.flags[i as usize] &= !FL_ON_PATH;
                    return (pn, dn);
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

fn child_numbers_local(g: &Graph<'_>, local: &[(u32, u32, bool); MAX_MOVES], ci: usize, c: u32) -> (u32, u32) {
    if local[ci].2 {
        (local[ci].0, local[ci].1)
    } else {
        g.child_numbers(c)
    }
}

/// Scales the fresh numbers of a node by the network's view (eval in
/// centipawns for the attacker): level 1 = every 150 cp one doubling, at
/// most three; level 2 = every 100 cp, at most five. An attacker the net
/// sees ahead gets a smaller proof number and a larger disproof number,
/// one it sees behind the other way round; the mobility stays the base of
/// the number that counts the moves.
fn bias_numbers(pn: u32, dn: u32, eval: i32, level: u8) -> (u32, u32) {
    let (step, limit) = if level >= 2 { (100, 5) } else { (150, 3) };
    let bias = (eval / step).clamp(-limit, limit);
    let (up, down) = (bias.max(0) as u32, (-bias).max(0) as u32);
    (clip(pn << down, DEFAULT_NUMBER_CAP), clip(dn << up, DEFAULT_NUMBER_CAP))
}

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

/// Caps a threshold handed back as a number below INF.
fn finite(x: u32) -> u32 {
    x.min(INF - 1)
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
