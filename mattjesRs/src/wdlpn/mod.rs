//! The WDL search (milestone 7, docs/m7-wdl-search-design.md): df-pn without
//! a depth bound that proves or disproves "the attacker forces a win",
//! whatever the distance. Direct port of `mattjesGo/wdlpn`, see there for the
//! full explanation: two questions per position (the root as attacker or as
//! defender node), repetition on the path as a draw terminal, conditional
//! disproofs that carry the ancestors they rely on (graph history
//! interaction, Kishimoto and Müller 2004), a ply cap as the safety net.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::bitboard::Board;
use crate::chess::{new_buffer, Move, MoveBuffer, MAX_MOVES};
use crate::mateab::{Oracle, Verdict};
pub use crate::matepn::{Codec, Float, Saturating, INF};
use crate::tt::{Key, TransTable};

/// The top bits of every value: the class of the entry, so that a bucket
/// table with `set_priority_bits` never evicts a proof for an open estimate:
/// 0 open, 1 conditionally disproven (valid with its ancestors on the path),
/// 2 disproven, 3 proven.
pub const PRIORITY_BITS: u32 = 2;

const CLASS_OPEN: u64 = 0;
const CLASS_CONDITIONAL: u64 = 1;
const CLASS_DISPROVEN: u64 = 2;
const CLASS_PROVEN: u64 = 3;

/// Caps the codec width like in matepn.
pub const MAX_CODEC_BITS: u32 = 20;

/// The width a codec for this table should use.
pub fn codec_bits<T: TransTable>(t: &T) -> u32 {
    (t.value_bits() - PRIORITY_BITS).min(MAX_CODEC_BITS)
}

/// The ply cap of the search path (the root is ply 0).
pub const MAX_PLY: usize = 255;

const DEP_WORDS: usize = (MAX_PLY + 64) / 64;

/// A set of path plies a disproof relies on (repeated ancestors).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct DepSet([u64; DEP_WORDS]);

impl DepSet {
    fn set(&mut self, ply: usize) {
        self.0[ply >> 6] |= 1 << (ply & 63);
    }

    fn union(&mut self, o: &DepSet) {
        for i in 0..DEP_WORDS {
            self.0[i] |= o.0[i];
        }
    }

    fn is_empty(&self) -> bool {
        self.0.iter().all(|&w| w == 0)
    }

    fn contains(&self, ply: usize) -> bool {
        self.0[ply >> 6] >> (ply & 63) & 1 != 0
    }

    /// Drops every ply >= ply.
    fn clear_from(&mut self, ply: usize) {
        for i in 0..DEP_WORDS {
            let lo = i << 6;
            if lo >= ply {
                self.0[i] = 0;
            } else if lo + 64 > ply {
                self.0[i] &= (1u64 << (ply & 63)) - 1;
            }
        }
    }

    /// The shallowest ply in the set (MAX_PLY + 1 when empty).
    fn lowest(&self) -> usize {
        for (i, &w) in self.0.iter().enumerate() {
            if w != 0 {
                return (i << 6) + w.trailing_zeros() as usize;
            }
        }
        MAX_PLY + 1
    }
}

/// Result of one question.
#[derive(Clone, Default)]
pub struct Result {
    /// The attacker forces a win.
    pub proven: bool,
    /// The defender holds at least a draw.
    pub disproven: bool,
    /// The stop flag was set, nothing decided.
    pub aborted: bool,
    pub pv: Vec<Move>,
    /// Node visits.
    pub nodes: u64,
    /// Children seen for the first time.
    pub leaves: u64,
    /// Children found in the table.
    pub tt_hits: u64,
    /// Conditional disproofs reused (their ancestors were on the path).
    pub cond_hits: u64,
    /// Conditional disproofs found but unusable on this path.
    pub cond_miss: u64,
    /// Visits that hit MAX_PLY (unknown, not stored).
    pub capped: u64,
    /// Conditional disproofs kept next to the table.
    pub cond_size: usize,
    /// The root's final numbers (0/INF when decided).
    pub pn: u32,
    pub dn: u32,
}

/// The running counters of the current question (for progress lines).
#[derive(Clone, Copy, Default)]
pub struct Counters {
    pub leaves: u64,
    pub tt_hits: u64,
    pub cond_hits: u64,
    pub cond_miss: u64,
    pub cond_size: u64,
    pub capped: u64,
}

/// The progress callback type of `Searcher::progress`.
pub type ProgressFn<T> = Box<dyn FnMut(u64, &T, Counters)>;

/// The default interval of progress calls (a power of two).
pub const PROGRESS_NODES: u64 = 1 << 22;

/// The polling interval of the stop flag (a power of two).
pub const STOP_NODES: u64 = 1 << 12;

/// Distinguishes the two questions in one table.
const BLACK_ATTACKER_SALT: Key = [0x2545F4914F6CDD1D, 0x9E3779B97F4A7C15];

#[derive(Clone, Copy, Default)]
struct Child {
    mv: Move,
    key: Key,
    pn: u32,
    dn: u32,
    dep: DepSet,
}

/// The search state.
pub struct Searcher<O: Oracle, T: TransTable, C: Codec> {
    oracle: O,
    table: T,
    codec: C,
    mobility: bool,
    /// Widens the child's threshold (in eighths, see matepn).
    pub epsilon: u32,
    /// Called every `progress_every` visits (a power of two).
    pub progress: Option<ProgressFn<T>>,
    pub progress_every: u64,
    /// Polled every `STOP_NODES` visits.
    pub stop: Option<Arc<AtomicBool>>,
    aborted: bool,

    attacker_white: bool,
    /// Position keys of the current path, `path[ply]`.
    path: [Key; MAX_PLY + 2],
    /// Per conditionally disproven position (table key), the keys of the
    /// ancestors its disproof relies on.
    deps: HashMap<Key, Vec<Key>>,
    prio_shift: u32,
    value_mask: u64,

    nodes: u64,
    leaves: u64,
    tt_hits: u64,
    cond_hits: u64,
    cond_miss: u64,
    capped: u64,
}

impl<O: Oracle, T: TransTable, C: Codec> Searcher<O, T, C> {
    /// `mobility` initialises unknown children with their number of legal
    /// moves instead of 1.
    pub fn new(oracle: O, table: T, codec: C, mobility: bool) -> Self {
        let shift = table.value_bits() - PRIORITY_BITS;
        assert!(codec.pack(INF, INF) >> shift == 0, "wdlpn: the codec leaves no room for the priority bits (use codec_bits)");
        Searcher {
            oracle,
            table,
            codec,
            mobility,
            epsilon: 0,
            progress: None,
            progress_every: PROGRESS_NODES,
            stop: None,
            aborted: false,
            attacker_white: true,
            path: [[0; 2]; MAX_PLY + 2],
            deps: HashMap::new(),
            prio_shift: shift,
            value_mask: (1 << shift) - 1,
            nodes: 0,
            leaves: 0,
            tt_hits: 0,
            cond_hits: 0,
            cond_miss: 0,
            capped: 0,
        }
    }

    /// Forgets the conditional disproofs (call it together with the table's
    /// `clear`).
    pub fn clear(&mut self) {
        self.deps = HashMap::new();
    }

    pub fn counters(&self) -> Counters {
        Counters { leaves: self.leaves, tt_hits: self.tt_hits, cond_hits: self.cond_hits, cond_miss: self.cond_miss, cond_size: self.deps.len() as u64, capped: self.capped }
    }

    pub fn table(&self) -> &T {
        &self.table
    }

    pub fn into_table(self) -> T {
        self.table
    }

    fn key(&self, b: &Board) -> Key {
        if self.attacker_white {
            b.key
        } else {
            [b.key[0] ^ BLACK_ATTACKER_SALT[0], b.key[1] ^ BLACK_ATTACKER_SALT[1]]
        }
    }

    #[inline(always)]
    fn attacker(&self, b: &Board) -> bool {
        b.white_move == self.attacker_white
    }

    #[inline(always)]
    fn probe_oracle(&self, b: &Board) -> Verdict {
        self.oracle.probe(b).0
    }

    /// Answers one question: with `side_to_move_attacks` the root is an
    /// attacker node ("the side to move forces a win"), otherwise a defender
    /// node ("the opponent forces a win").
    pub fn solve(&mut self, root: &Board, side_to_move_attacks: bool) -> Result {
        self.attacker_white = root.white_move == side_to_move_attacks;
        self.nodes = 0;
        self.leaves = 0;
        self.tt_hits = 0;
        self.cond_hits = 0;
        self.cond_miss = 0;
        self.capped = 0;
        self.aborted = false;
        let (pn, dn, _) = self.mid(root, INF, INF, 0);
        let mut r = Result {
            proven: pn == 0,
            disproven: dn == 0,
            nodes: self.nodes,
            leaves: self.leaves,
            tt_hits: self.tt_hits,
            cond_hits: self.cond_hits,
            cond_miss: self.cond_miss,
            capped: self.capped,
            cond_size: self.deps.len(),
            pn,
            dn,
            ..Default::default()
        };
        if self.aborted {
            r.proven = false;
            r.disproven = false;
            r.aborted = true;
            return r;
        }
        if r.proven {
            r.pv = self.proof_line(root);
        }
        r
    }

    #[inline(always)]
    fn check_stop(&mut self) {
        if self.nodes & (STOP_NODES - 1) == 0 {
            if let Some(s) = &self.stop {
                if s.load(Ordering::Relaxed) {
                    self.aborted = true;
                }
            }
        }
    }

    /// Looks for b on the path above ply (same side to move, back to the
    /// last irreversible move) and returns the ply of the repeated ancestor.
    fn repetition(&self, b: &Board, ply: usize) -> Option<usize> {
        let limit = ply as i64 - b.halfmove_clock as i64;
        let mut i = ply as i64 - 2;
        while i >= 0 && i >= limit {
            if self.path[i as usize] == b.key {
                return Some(i as usize);
            }
            i -= 2;
        }
        None
    }

    /// Evaluates a node without expanding it: repetition on the path, oracle,
    /// no legal moves. `None` means the node must be expanded, n is then the
    /// number of legal moves in buf; else the numbers and the dependency.
    fn terminal(&self, b: &Board, buf: &mut MoveBuffer, ply: usize) -> (Option<(u32, u32, DepSet)>, usize) {
        let mut dep = DepSet::default();
        if let Some(r) = self.repetition(b, ply) {
            dep.set(r);
            return (Some((INF, 0, dep)), 0);
        }
        let attacker = self.attacker(b);
        match self.probe_oracle(b) {
            Verdict::Draw => return (Some((INF, 0, dep)), 0),
            Verdict::Win => return (Some(if attacker { (0, INF, dep) } else { (INF, 0, dep) }), 0),
            Verdict::Loss => return (Some(if attacker { (INF, 0, dep) } else { (0, INF, dep) }), 0),
            Verdict::Unknown => {}
        }
        let n = b.gen_moves(buf);
        if n == 0 {
            if !attacker && b.in_check() {
                return (Some((0, INF, dep)), 0); // mate
            }
            return (Some((INF, 0, dep)), 0); // stalemate, or the attacker is mated
        }
        (None, n)
    }

    /// Reads a child's table entry. A conditional disproof counts only when
    /// every ancestor it relies on is on the current path; the child then
    /// carries those plies as its dependency. `ply` is the parent's ply.
    fn probe_child(&mut self, c: &mut Child, ply: usize) -> bool {
        let Some(v) = self.table.probe(c.key) else {
            return false;
        };
        if v >> self.prio_shift != CLASS_CONDITIONAL {
            (c.pn, c.dn) = self.codec.unpack(v & self.value_mask);
            return true;
        }
        let Some(need) = self.deps.get(&c.key) else {
            return false; // the list was dropped with a clear, the entry is stale
        };
        let mut dep = DepSet::default();
        for k in need {
            let mut found = false;
            for i in (0..=ply).rev() {
                if self.path[i] == *k {
                    dep.set(i);
                    found = true;
                    break;
                }
            }
            if !found {
                self.cond_miss += 1;
                return false;
            }
        }
        self.cond_hits += 1;
        c.pn = INF;
        c.dn = 0;
        c.dep = dep;
        true
    }

    /// Nagai's multiple iterative deepening (see matepn): searches below b
    /// until pn reaches th_pn or dn reaches th_dn and returns the numbers
    /// plus the path plies a disproof relies on.
    fn mid(&mut self, b: &Board, th_pn: u32, th_dn: u32, ply: usize) -> (u32, u32, DepSet) {
        let none = DepSet::default();
        self.nodes += 1;
        if self.nodes & (self.progress_every - 1) == 0 {
            let counters = self.counters();
            if let Some(p) = self.progress.as_mut() {
                p(self.nodes, &self.table, counters);
            }
        }
        self.check_stop();
        if self.aborted {
            return (th_pn, th_dn, none);
        }
        if ply >= MAX_PLY {
            self.capped += 1;
            return (th_pn, th_dn, none); // unknown: the parent moves on, nothing is stored
        }
        self.path[ply] = b.key;
        let mut buf = new_buffer();
        let (done, n) = self.terminal(b, &mut buf, ply);
        if let Some((pn, dn, dep)) = done {
            self.store(b, pn, dn, &dep, ply);
            return (pn, dn, dep);
        }
        let attacker = self.attacker(b);

        let mut children = [Child::default(); MAX_MOVES];
        for (i, &mv) in buf[..n].iter().enumerate() {
            let mut c = *b;
            c.do_move(mv);
            children[i] = Child { mv, key: self.key(&c), ..Default::default() };
            self.table.prefetch(children[i].key);
        }
        for child in children[..n].iter_mut() {
            if self.probe_child(child, ply) {
                self.tt_hits += 1;
                continue;
            }
            self.leaves += 1;
            let mut c = *b;
            c.do_move(child.mv);
            (child.pn, child.dn, child.dep) = self.leaf_numbers(&c, ply + 1);
        }

        loop {
            let (mut best, mut second) = (INF, INF);
            let mut sum = 0u32;
            let mut bi = usize::MAX;
            for i in 0..n {
                let c = &children[i];
                let (k, other) = if attacker { (c.pn, c.dn) } else { (c.dn, c.pn) };
                sum = sat_add(sum, other);
                if k < best || (k == best && bi != usize::MAX && other < other_of(&children[bi], attacker)) {
                    second = best;
                    best = k;
                    bi = i;
                } else if k < second {
                    second = k;
                }
            }
            let (pn, dn) = if attacker { (best, sum) } else { (sum, best) };
            if pn >= th_pn || dn >= th_dn {
                let mut dep = none;
                if dn == 0 {
                    dep = disproof_dep(&children[..n], attacker);
                    dep.clear_from(ply); // a repetition of this node itself is resolved here
                }
                self.store(b, pn, dn, &dep, ply);
                return (pn, dn, dep);
            }
            let c = children[bi];
            let mut bound = sat_add(second, 1);
            if self.epsilon > 0 && second < INF {
                bound = sat_add(second, (((second as u64) * self.epsilon as u64) / 8).max(1) as u32);
            }
            let (c_th_pn, c_th_dn) = if attacker { (th_pn.min(bound), sat_sub(th_dn, sat_sub(dn, c.dn))) } else { (sat_sub(th_pn, sat_sub(pn, c.pn)), th_dn.min(bound)) };
            let mut cb = *b;
            cb.do_move(c.mv);
            let (npn, ndn, ndep) = self.mid(&cb, c_th_pn, c_th_dn, ply + 1);
            if self.aborted {
                return (pn, dn, none);
            }
            children[bi].pn = npn;
            children[bi].dn = ndn;
            children[bi].dep = ndep;
        }
    }

    /// Initialises a child without a table entry: a terminal child is
    /// recognised right away, otherwise 1/1 or, with mobility, the number of
    /// legal moves on the side that must prove all of them.
    fn leaf_numbers(&self, c: &Board, ply: usize) -> (u32, u32, DepSet) {
        let mut dep = DepSet::default();
        if !self.mobility {
            // still catch repetitions, they are the whole point of the draw side
            if let Some(r) = self.repetition(c, ply) {
                dep.set(r);
                return (INF, 0, dep);
            }
            return (1, 1, dep);
        }
        let mut buf = new_buffer();
        let (done, n) = self.terminal(c, &mut buf, ply);
        if let Some(r) = done {
            return r;
        }
        if self.attacker(c) {
            (1, n as u32, dep)
        } else {
            (n as u32, 1, dep)
        }
    }

    /// Writes the node; a conditional disproof (dep not empty) goes in as
    /// `CLASS_CONDITIONAL` with the keys of the ancestors it relies on.
    fn store(&mut self, b: &Board, pn: u32, dn: u32, dep: &DepSet, ply: usize) {
        let k = self.key(b);
        let class = if pn == 0 {
            CLASS_PROVEN
        } else if dn == 0 && dep.is_empty() {
            CLASS_DISPROVEN
        } else if dn == 0 {
            let mut need = Vec::with_capacity(2);
            for i in 0..ply {
                if dep.contains(i) {
                    need.push(self.path[i]);
                }
            }
            self.deps.insert(k, need);
            CLASS_CONDITIONAL
        } else {
            CLASS_OPEN
        };
        let v = self.codec.pack(pn, dn) | class << self.prio_shift;
        self.table.store(k, v);
    }

    /// Whether a child position is proven: terminal by itself (mate, oracle)
    /// or proven in the table. `terminal_only` restricts it to the terminal
    /// case.
    fn child_proven(&mut self, c: &Board, terminal_only: bool) -> bool {
        let attacker = self.attacker(c);
        match self.probe_oracle(c) {
            Verdict::Win => return attacker,
            Verdict::Loss => return !attacker,
            Verdict::Draw => return false,
            Verdict::Unknown => {}
        }
        if !attacker && !c.has_moves() {
            return c.in_check();
        }
        if terminal_only {
            return false;
        }
        match self.table.probe(self.key(c)) {
            Some(v) => v >> self.prio_shift == CLASS_PROVEN,
            None => false,
        }
    }

    /// Walks proven children from the root: the attacker prefers a terminal
    /// child (mate, won table position), the defender takes the first proven
    /// child; positions already on the line are skipped, so the walk is
    /// acyclic. A line of the proof, not the shortest.
    fn proof_line(&mut self, root: &Board) -> Vec<Move> {
        let mut visited: HashMap<Key, bool> = HashMap::new();
        let mut pv = Vec::new();
        let mut b = *root;
        while pv.len() < MAX_PLY {
            visited.insert(b.key, true);
            if self.probe_oracle(&b) != Verdict::Unknown {
                return pv;
            }
            let mut buf = new_buffer();
            let n = b.gen_moves(&mut buf);
            if n == 0 {
                return pv;
            }
            let mut chosen = usize::MAX;
            for pass in 0..2 {
                if chosen != usize::MAX {
                    break;
                }
                for (i, &mv) in buf[..n].iter().enumerate() {
                    let mut c = b;
                    c.do_move(mv);
                    if visited.contains_key(&c.key) {
                        continue;
                    }
                    if self.child_proven(&c, pass == 0) {
                        chosen = i;
                        break;
                    }
                }
            }
            if chosen == usize::MAX {
                return pv;
            }
            pv.push(buf[chosen]);
            b.do_move(buf[chosen]);
        }
        pv
    }
}

/// What the node's disproof relies on: an attacker node needs all children
/// disproven (the union of their dependencies), a defender node needs one
/// and takes the least conditional one (no dependency, else the one whose
/// shallowest ancestor is deepest).
fn disproof_dep(children: &[Child], attacker: bool) -> DepSet {
    let mut dep = DepSet::default();
    if attacker {
        for c in children {
            dep.union(&c.dep);
        }
        return dep;
    }
    let mut best: Option<(usize, usize)> = None; // (index, lowest)
    for (i, c) in children.iter().enumerate() {
        if c.dn != 0 {
            continue;
        }
        let low = c.dep.lowest();
        if best.is_none_or(|(_, bl)| low > bl) {
            best = Some((i, low));
        }
    }
    if let Some((bi, _)) = best {
        dep = children[bi].dep;
    }
    dep
}

fn other_of(c: &Child, attacker: bool) -> u32 {
    if attacker {
        c.dn
    } else {
        c.pn
    }
}

/// Adds two numbers; INF is absorbing, finite sums stay finite (a sum that
/// reached INF would look like a proof or disproof: without a depth bound the
/// DAG overcounting makes sums of tens of millions real).
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
