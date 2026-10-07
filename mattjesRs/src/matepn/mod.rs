//! The proof-number mate search, variant df-pn (Nagai 2002): depth-first, with
//! proof and disproof numbers kept in the transposition table, so the table is
//! the tree. Working title from the design document. Direct port of
//! `mattjesGo/matepn`, see there for the full explanation.
//!
//! The proof goal is "mate in at most N plies". The remaining depth is part of
//! the table key (like perft_tt), so every entry is path-independent.

mod codec;

pub use codec::{Codec, Float, Saturating, INF};

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::bitboard::Board;
use crate::chess::{new_buffer, Move, MoveBuffer};
use crate::mateab::{Oracle, Verdict};
use crate::tt::{Key, TransTable};

/// The deepest proof goal (the depth salt table size).
pub const MAX_PLIES: usize = 127;

/// Result of a search.
#[derive(Clone, Default)]
pub struct Result {
    /// A forced mate within the depth exists.
    pub proven: bool,
    /// No forced mate within the depth.
    pub disproven: bool,
    /// The length of the mate along the proof tree in the table; with a single
    /// depth an upper bound of the shortest mate, with iterative deepening the
    /// shortest. 0 when the tree is incomplete (entries overwritten).
    pub mate_plies: usize,
    pub pv: Vec<Move>,
    /// Node visits (each regenerates its moves).
    pub nodes: u64,
    /// Children seen for the first time (no table entry).
    pub leaves: u64,
    /// Children found in the table.
    pub tt_hits: u64,
    /// Children decided by a depth-free final entry.
    pub final_hits: u64,
}

/// The progress callback type of `Searcher::progress`.
pub type ProgressFn<T> = Box<dyn FnMut(u64, &T)>;

/// The interval of progress calls (a power of two).
pub const PROGRESS_NODES: u64 = 1 << 22;

/// XORed into both key words, so (position, remaining depth) is simply another
/// 128-bit key. Same generator as perft_tt.
static DEPTH_SALT: LazyLock<[Key; MAX_PLIES + 1]> = LazyLock::new(|| {
    let mut s = [[0u64; 2]; MAX_PLIES + 1];
    let mut x: u64 = 0x9E3779B97F4A7C15;
    for row in s.iter_mut() {
        for w in row.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *w = x;
        }
    }
    s
});

fn key(b: &Board, depth: usize) -> Key {
    let s = &DEPTH_SALT[depth];
    [b.key[0] ^ s[0], b.key[1] ^ s[1]]
}

#[derive(Clone, Copy, Default)]
struct Child {
    mv: Move,
    pn: u32,
    dn: u32,
}

/// The search state.
pub struct Searcher<O: Oracle, T: TransTable, C: Codec> {
    oracle: O,
    table: T,
    codec: C,
    mobility: bool,
    /// Widens the child's threshold from second+1 to second*(1+epsilon)
    /// (Pawlewicz & Lew 2007), in eighths: 0 = off, 1 = 1/8, 4 = 1/2.
    pub epsilon: u32,
    /// Reuses proven and disproven results across depths under the unsalted key
    /// (smallest proven and largest disproven depth).
    pub final_entries: bool,
    pub progress: Option<ProgressFn<T>>,
    nodes: u64,
    leaves: u64,
    tt_hits: u64,
    final_hits: u64,
}

impl<O: Oracle, T: TransTable, C: Codec> Searcher<O, T, C> {
    /// `mobility` initialises unknown children with their number of legal moves
    /// instead of 1 (one generator call per new leaf).
    pub fn new(oracle: O, table: T, codec: C, mobility: bool) -> Self {
        Searcher { oracle, table, codec, mobility, epsilon: 0, final_entries: false, progress: None, nodes: 0, leaves: 0, tt_hits: 0, final_hits: 0 }
    }

    pub fn table(&self) -> &T {
        &self.table
    }

    pub fn into_table(self) -> T {
        self.table
    }

    /// Proves or disproves "the side to move mates within plies" (odd).
    pub fn solve(&mut self, root: &Board, plies: usize) -> Result {
        assert!((1..=MAX_PLIES).contains(&plies) && plies % 2 == 1, "matepn: plies must be odd and at most MAX_PLIES");
        self.nodes = 0;
        self.leaves = 0;
        self.tt_hits = 0;
        self.final_hits = 0;
        let (pn, dn) = self.mid(root, INF, INF, plies);
        let mut r = Result { proven: pn == 0, disproven: dn == 0, nodes: self.nodes, leaves: self.leaves, tt_hits: self.tt_hits, final_hits: self.final_hits, ..Default::default() };
        if r.proven {
            let mut memo = HashMap::new();
            let l = self.proof_length(root, plies, &mut memo);
            if l > 0 {
                r.mate_plies = l as usize;
                r.pv = self.proof_line(root, plies, &memo);
            }
        }
        r
    }

    /// `solve` with iterative deepening over odd depths up to max_plies; report
    /// is called after every depth. The first proven depth is the shortest
    /// mate. Nodes accumulate.
    pub fn solve_shortest(&mut self, root: &Board, max_plies: usize, mut report: impl FnMut(usize, &Result)) -> Result {
        let mut total = Result::default();
        let mut d = 1;
        while d <= max_plies && d <= MAX_PLIES {
            let mut r = self.solve(root, d);
            total.nodes += r.nodes;
            total.leaves += r.leaves;
            total.tt_hits += r.tt_hits;
            total.final_hits += r.final_hits;
            r.nodes = total.nodes;
            r.leaves = total.leaves;
            r.tt_hits = total.tt_hits;
            r.final_hits = total.final_hits;
            report(d, &r);
            if r.proven {
                return r;
            }
            total.disproven = r.disproven;
            d += 2;
        }
        total
    }

    /// Evaluates a node without expanding it: oracle, no legal moves, horizon.
    /// Returns `None` when the node must be expanded (n moves are then in buf,
    /// only checks for the attacker at depth 1), else the final numbers.
    /// Attacker nodes have odd depth.
    fn terminal(&self, b: &Board, buf: &mut MoveBuffer, depth: usize) -> (Option<(u32, u32)>, usize) {
        let attacker = depth % 2 == 1;
        let (v, plies) = self.oracle.probe(b);
        match v {
            Verdict::Draw => return (Some((INF, 0)), 0),
            Verdict::Loss if attacker => return (Some((INF, 0)), 0),
            Verdict::Win if !attacker => return (Some((INF, 0)), 0),
            Verdict::Win | Verdict::Loss if plies > 0 => {
                // the mate exists; too long for this depth = disproven here
                return (Some(if plies as usize > depth { (INF, 0) } else { (0, INF) }), 0);
            }
            _ => {} // unknown, or a win/loss without distance: search
        }
        let n = if attacker && depth == 1 { b.gen_checks(buf) } else { b.gen_moves(buf) };
        if n == 0 {
            if !attacker && b.in_check() {
                return (Some((0, INF)), 0); // mate
            }
            return (Some((INF, 0)), 0); // stalemate, no move, or no check at depth 1
        }
        if depth == 0 {
            return (Some((INF, 0)), n); // horizon: the defender still has moves
        }
        (None, n)
    }

    /// Nagai's "multiple iterative deepening": searches below b until the node's
    /// proof number reaches th_pn or its disproof number reaches th_dn, and
    /// returns the numbers at that point.
    fn mid(&mut self, b: &Board, th_pn: u32, th_dn: u32, depth: usize) -> (u32, u32) {
        self.nodes += 1;
        if self.nodes & (PROGRESS_NODES - 1) == 0 {
            if let Some(p) = self.progress.as_mut() {
                p(self.nodes, &self.table);
            }
        }
        let mut buf = new_buffer();
        let (done, n) = self.terminal(b, &mut buf, depth);
        if let Some((pn, dn)) = done {
            self.store(b, depth, pn, dn);
            return (pn, dn);
        }
        let attacker = depth % 2 == 1;

        // expand: numbers of all children from the table or as fresh leaves
        let mut children = [Child::default(); crate::chess::MAX_MOVES];
        for (i, &mv) in buf[..n].iter().enumerate() {
            let mut c = *b;
            c.do_move(mv);
            children[i].mv = mv;
            if let Some((pn, dn)) = self.probe_final(&c, depth - 1) {
                self.final_hits += 1;
                children[i].pn = pn;
                children[i].dn = dn;
                continue;
            }
            if let Some(v) = self.table.probe(key(&c, depth - 1)) {
                self.tt_hits += 1;
                (children[i].pn, children[i].dn) = self.codec.unpack(v);
                continue;
            }
            self.leaves += 1;
            (children[i].pn, children[i].dn) = self.leaf_numbers(&c, depth - 1);
        }

        loop {
            // the node's numbers, the best child and the second-best number
            let (mut best, mut second) = (INF, INF); // best = min pn (attacker) or min dn (defender), second = runner-up
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
                self.store(b, depth, pn, dn);
                return (pn, dn);
            }
            // thresholds for the best child: it may grow until it stops being the
            // best (second + 1, widened by epsilon), and the other number until the
            // node's threshold would be reached through the sum
            let c = children[bi];
            let mut bound = sat_add(second, 1);
            if self.epsilon > 0 && second < INF {
                bound = sat_add(second, (((second as u64) * self.epsilon as u64) / 8).max(1) as u32);
            }
            let (c_th_pn, c_th_dn) = if attacker { (th_pn.min(bound), sat_sub(th_dn, sat_sub(dn, c.dn))) } else { (sat_sub(th_pn, sat_sub(pn, c.pn)), th_dn.min(bound)) };
            let mut cb = *b;
            cb.do_move(c.mv);
            let (npn, ndn) = self.mid(&cb, c_th_pn, c_th_dn, depth - 1);
            children[bi].pn = npn;
            children[bi].dn = ndn;
        }
    }

    /// Initialises a child that has no table entry: 1/1, or with mobility the
    /// number of legal moves on the side that must prove all of them (defender:
    /// pn, attacker: dn). A terminal child is recognised right away.
    fn leaf_numbers(&self, c: &Board, depth: usize) -> (u32, u32) {
        if !self.mobility {
            return (1, 1);
        }
        let mut buf = new_buffer();
        let (done, n) = self.terminal(c, &mut buf, depth);
        if let Some(r) = done {
            return r;
        }
        if depth % 2 == 1 {
            (1, n as u32) // attacker: every move must be refuted to disprove
        } else {
            (n as u32, 1) // defender: every move must be answered to prove
        }
    }

    fn store(&mut self, b: &Board, depth: usize, pn: u32, dn: u32) {
        let v = self.codec.pack(pn, dn);
        self.table.store(key(b, depth), v);
        if self.final_entries && (pn == 0 || dn == 0) {
            self.store_final(b, depth, pn == 0);
        }
    }

    /// Final entries live under the unsalted key: 7 bits smallest proven depth
    /// + 1 and 7 bits largest disproven depth + 1 (0 = none each).
    fn store_final(&mut self, b: &Board, depth: usize, proven: bool) {
        let (mut pd, mut dd) = match self.table.probe(b.key) {
            Some(v) => unpack_final(v),
            None => (-1, -1),
        };
        let depth = depth as i32;
        if proven {
            if pd < 0 || depth < pd {
                pd = depth;
            }
        } else if depth > dd {
            dd = depth;
        }
        self.table.store(b.key, ((pd + 1) as u64) << 7 | (dd + 1) as u64);
    }

    /// Answers from the depth-free entry when it decides the node at this depth.
    fn probe_final(&mut self, b: &Board, depth: usize) -> Option<(u32, u32)> {
        if !self.final_entries {
            return None;
        }
        let (pd, dd) = unpack_final(self.table.probe(b.key)?);
        let depth = depth as i32;
        if pd >= 0 && pd <= depth {
            Some((0, INF))
        } else if dd >= depth {
            Some((INF, 0))
        } else {
            None
        }
    }

    /// Walks the proof tree in the table: at an attacker node the shortest proven
    /// child, at a defender node the longest. Returns -1 when a needed entry is
    /// missing (overwritten). memo is keyed by the salted key.
    fn proof_length(&mut self, b: &Board, depth: usize, memo: &mut HashMap<Key, i32>) -> i32 {
        let k = key(b, depth);
        if let Some(&l) = memo.get(&k) {
            return l;
        }
        let attacker = depth % 2 == 1;
        let (v, plies) = self.oracle.probe(b);
        if plies > 0 && ((attacker && v == Verdict::Win) || (!attacker && v == Verdict::Loss)) {
            memo.insert(k, plies as i32); // the oracle (endgame table) ended the line
            return plies as i32;
        }
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        if n == 0 {
            if !attacker && b.in_check() {
                memo.insert(k, 0);
                return 0;
            }
            return -1;
        }
        if depth == 0 {
            return -1;
        }
        let mut result = -1;
        for &mv in &buf[..n] {
            let mut c = *b;
            c.do_move(mv);
            // proven when the table says so (salted or final entry) or when the
            // child is terminal itself (mate, oracle): terminal children found as
            // fresh leaves were never visited and have no entry
            let proven = if let Some((pn, _)) = self.probe_final(&c, depth - 1) {
                pn == 0
            } else if let Some(v) = self.table.probe(key(&c, depth - 1)) {
                self.codec.unpack(v).0 == 0
            } else {
                let mut cbuf = new_buffer();
                matches!(self.terminal(&c, &mut cbuf, depth - 1).0, Some((0, _)))
            };
            let l = if proven { self.proof_length(&c, depth - 1, memo) } else { -1 };
            if l < 0 {
                if attacker {
                    continue;
                }
                return -1;
            }
            if attacker {
                if result < 0 || l + 1 < result {
                    result = l + 1;
                }
            } else {
                result = result.max(l + 1);
            }
        }
        if result >= 0 {
            memo.insert(k, result);
        }
        result
    }

    /// Follows the lengths computed by `proof_length`.
    fn proof_line(&self, root: &Board, mut depth: usize, memo: &HashMap<Key, i32>) -> Vec<Move> {
        let mut pv = Vec::new();
        let mut b = *root;
        loop {
            let want = match memo.get(&key(&b, depth)) {
                Some(&w) if w > 0 => w,
                _ => return pv,
            };
            let mut buf = new_buffer();
            let n = b.gen_moves(&mut buf);
            let mut found = false;
            for &mv in &buf[..n] {
                let mut c = b;
                c.do_move(mv);
                if memo.get(&key(&c, depth - 1)) == Some(&(want - 1)) {
                    pv.push(mv);
                    b = c;
                    depth -= 1;
                    found = true;
                    break;
                }
            }
            if !found {
                return pv;
            }
        }
    }
}

fn unpack_final(v: u64) -> (i32, i32) {
    ((v >> 7 & 127) as i32 - 1, (v & 127) as i32 - 1)
}

fn other_of(c: &Child, attacker: bool) -> u32 {
    if attacker {
        c.dn
    } else {
        c.pn
    }
}

fn sat_add(a: u32, b: u32) -> u32 {
    if a >= INF || b >= INF || a + b >= INF {
        INF
    } else {
        a + b
    }
}

fn sat_sub(a: u32, b: u32) -> u32 {
    if a >= INF {
        INF
    } else {
        a.saturating_sub(b)
    }
}
