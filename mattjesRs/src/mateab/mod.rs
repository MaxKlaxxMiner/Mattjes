//! The first mate search: depth-first with a "mate window" and iterative
//! deepening, built as the simple, exact reference that the later searches are
//! checked against. Working title from the design document. Direct port of
//! `mattjesGo/mateab`.
//!
//! The tree is an AND/OR tree. At an attacker node (OR) one move that leads to
//! mate is enough, so the node returns at the first success. At a defender node
//! (AND) one move that escapes is enough to refute, so the node returns at the
//! first escape. There is no evaluation, only "mate in n plies" or "no mate
//! within the remaining depth". Iterative deepening over 1, 3, 5, ... plies makes
//! the first mate found the shortest one.
//!
//! With a transposition table (`ttvalue.rs`) finished nodes are remembered: a
//! proven mate is exact and depth independent, a refutation holds for every
//! remaining depth up to the one searched, and the stored move is tried first.

mod oracle;
mod ttvalue;

pub use oracle::{Material, Oracle, Verdict};
pub use ttvalue::VALUE_BITS;

use ttvalue::{pack_value, same_move, unpack_value, TT_MATE, TT_NO_MATE};

use crate::bitboard::Board;
use crate::chess::{new_buffer, Move, Piece, Pos};
use crate::tt::TransTable;

/// Bounds the search depth in plies (the TT depth field holds 7 bits).
pub const MAX_PLY: usize = 128;

/// Returned by the node functions when no forced mate exists within the
/// remaining depth.
const NO_MATE: i32 = -1;

/// Result of one depth of `solve`. `mate_plies` is the mate distance in plies
/// (odd, attacker to move at the root), 0 if no mate was found within the depth.
/// `pv` is the principal variation; with a table it may be shorter than
/// `mate_plies` when the entries needed to extend it were replaced.
#[derive(Clone, Default)]
pub struct Result {
    pub mate_plies: u32,
    pub pv: Vec<Move>,
    pub nodes: u64,
    /// Probes that ended the node (mate or refutation taken from the table).
    pub tt_hits: u64,
}

/// Per-search state: the oracle, the optional table, node counters, principal
/// variation (triangular table) and one killer move per ply, the move that most
/// recently mated or refuted at that ply and is tried first.
pub struct Searcher<O: Oracle, T: TransTable> {
    oracle: O,
    table: Option<T>,
    nodes: u64,
    tt_hits: u64,
    pv: Box<[[Move; MAX_PLY]; MAX_PLY]>,
    pv_len: [usize; MAX_PLY],
    killer: [Move; MAX_PLY],
    /// Called every `PROGRESS_NODES` nodes of a depth with the node count and the
    /// table, for long searches.
    pub progress: Option<ProgressFn<T>>,
}

/// The progress callback type of `Searcher::progress`.
pub type ProgressFn<T> = Box<dyn FnMut(u64, Option<&T>)>;

/// The interval of progress calls (a power of two).
pub const PROGRESS_NODES: u64 = 1 << 26;

const NO_SQUARE: Pos = Pos(-1);

impl<O: Oracle, T: TransTable> Searcher<O, T> {
    /// A searcher that asks `oracle` at every node and uses `table` (`None` for none).
    pub fn new(oracle: O, table: Option<T>) -> Searcher<O, T> {
        Searcher { oracle, table, nodes: 0, tt_hits: 0, pv: Box::new([[Move::NONE; MAX_PLY]; MAX_PLY]), pv_len: [0; MAX_PLY], killer: [Move::NONE; MAX_PLY], progress: None }
    }

    /// The table (for statistics after the search).
    pub fn table(&self) -> Option<&T> {
        self.table.as_ref()
    }

    /// Gives the table back, so the caller can reuse it for the next position.
    pub fn into_table(self) -> Option<T> {
        self.table
    }

    /// Looks for a forced mate for the side to move within `max_plies` plies,
    /// with iterative deepening over odd depths. `report` is called after every
    /// depth; the returned `Result` is the first depth that found a mate (or the
    /// last depth, with `mate_plies` 0). Nodes accumulate over all depths.
    pub fn solve(&mut self, root: &Board, max_plies: u32, mut report: impl FnMut(u32, &Result)) -> Result {
        self.killer = [Move::NONE; MAX_PLY];
        let (mut total_nodes, mut total_hits) = (0u64, 0u64);
        let mut last = Result::default();
        let mut d = 1u32;
        while d <= max_plies && (d as usize) < MAX_PLY {
            self.nodes = 0;
            self.tt_hits = 0;
            let dist = self.attack(root, d, 0);
            total_nodes += self.nodes;
            total_hits += self.tt_hits;
            last = Result { nodes: total_nodes, tt_hits: total_hits, ..Default::default() };
            if dist != NO_MATE {
                last.mate_plies = dist as u32;
                last.pv = self.extend_pv(root, dist as usize);
            }
            report(d, &last);
            if dist != NO_MATE {
                break;
            }
            d += 2;
        }
        last
    }

    /// Probes the table for the node. Returns `Some(result)` when the entry ends
    /// the node, else the entry's move squares for ordering (or `NO_SQUARE`).
    #[inline(always)]
    fn probe(&mut self, b: &Board, depth: u32, ply: usize) -> std::result::Result<(Pos, Pos), i32> {
        let Some(t) = self.table.as_mut() else { return Ok((NO_SQUARE, NO_SQUARE)) };
        let Some(v) = t.probe(b.key) else { return Ok((NO_SQUARE, NO_SQUARE)) };
        let (typ, d, from, to) = unpack_value(v);
        if typ == TT_MATE && d <= depth {
            self.tt_hits += 1;
            self.pv_len[ply] = 0; // the line continues in the table, see extend_pv
            return Err(d as i32);
        }
        if typ == TT_NO_MATE && d >= depth {
            self.tt_hits += 1;
            return Err(NO_MATE);
        }
        Ok((from, to))
    }

    fn store(&mut self, b: &Board, typ: u32, depth: u32, m: Move) {
        if let Some(t) = self.table.as_mut() {
            t.store(b.key, pack_value(typ, depth, m));
        }
    }

    /// An OR node: the attacker is to move with `depth` plies left (odd).
    /// Returns the mate distance in plies or `NO_MATE`. At depth 1 only checking
    /// moves are tried, because a mating move is always a check.
    fn attack(&mut self, b: &Board, depth: u32, ply: usize) -> i32 {
        self.nodes += 1;
        if self.nodes & (PROGRESS_NODES - 1) == 0 {
            if let Some(p) = self.progress.as_mut() {
                p(self.nodes, self.table.as_ref());
            }
        }
        if matches!(self.oracle.probe(b).0, Verdict::Draw | Verdict::Loss) {
            return NO_MATE;
        }
        let (tt_from, tt_to) = match self.probe(b, depth, ply) {
            Err(r) => return r,
            Ok(sq) => sq,
        };
        let mut buf = new_buffer();
        if depth == 1 {
            let n = b.gen_checks(&mut buf);
            for &m in &buf[..n] {
                let mut child = *b;
                child.do_move(m);
                self.nodes += 1;
                if !child.has_moves() {
                    self.pv[ply][0] = m;
                    self.pv_len[ply] = 1;
                    self.killer[ply] = m;
                    self.store(b, TT_MATE, 1, m);
                    return 1;
                }
            }
            self.store(b, TT_NO_MATE, 1, Move::NONE);
            return NO_MATE;
        }
        let n = b.gen_moves(&mut buf);
        self.order_attack(b, &mut buf[..n], ply, tt_from, tt_to);
        for &m in &buf[..n] {
            let mut child = *b;
            child.do_move(m);
            let r = self.defend(&child, depth - 1, ply + 1);
            if r != NO_MATE {
                self.set_pv(ply, m);
                self.killer[ply] = m;
                self.store(b, TT_MATE, r as u32 + 1, m);
                return r + 1;
            }
        }
        self.store(b, TT_NO_MATE, depth, Move::NONE);
        NO_MATE
    }

    /// An AND node: the defender is to move with `depth` plies left (even, at
    /// least 2). Every defender move must lead to mate; the longest of these mates
    /// is the distance. The first escape refutes the node.
    fn defend(&mut self, b: &Board, depth: u32, ply: usize) -> i32 {
        self.nodes += 1;
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        if n == 0 {
            if b.in_check() {
                self.pv_len[ply] = 0;
                return 0; // mated already (the attacker's previous move mated before the last ply)
            }
            return NO_MATE; // stalemate
        }
        if matches!(self.oracle.probe(b).0, Verdict::Draw | Verdict::Win) {
            return NO_MATE;
        }
        let (tt_from, tt_to) = match self.probe(b, depth, ply) {
            Err(r) => return r,
            Ok(sq) => sq,
        };
        self.order_defend(&mut buf[..n], ply, tt_from, tt_to);
        let mut worst = 0;
        let mut worst_move = Move::NONE;
        for (i, &m) in buf[..n].iter().enumerate() {
            let mut child = *b;
            child.do_move(m);
            let r = self.attack(&child, depth - 1, ply + 1);
            if r == NO_MATE {
                self.killer[ply] = m;
                self.store(b, TT_NO_MATE, depth, m);
                return NO_MATE;
            }
            if r > worst || i == 0 {
                worst = r;
                worst_move = m;
                self.set_pv(ply, m);
            }
        }
        self.store(b, TT_MATE, worst as u32 + 1, worst_move);
        worst + 1
    }

    /// Records m as the move at `ply` and appends the child's line.
    fn set_pv(&mut self, ply: usize, m: Move) {
        let len = self.pv_len[ply + 1];
        let (head, tail) = self.pv.split_at_mut(ply + 1);
        head[ply][0] = m;
        head[ply][1..=len].copy_from_slice(&tail[0][..len]);
        self.pv_len[ply] = len + 1;
    }

    /// The principal variation of a found mate: the moves from the triangular
    /// table, then, where a table hit cut the line short, the moves the
    /// transposition table remembers for each position (mating move at attacker
    /// nodes, longest defence at defender nodes), until the mate or a missing entry.
    fn extend_pv(&mut self, root: &Board, mate_plies: usize) -> Vec<Move> {
        let mut pv = self.pv[0][..self.pv_len[0]].to_vec();
        let Some(t) = self.table.as_mut() else { return pv };
        let mut b = *root;
        for &m in &pv {
            b.do_move(m);
        }
        while pv.len() < mate_plies {
            let Some(v) = t.probe(b.key) else { break };
            let (typ, _, from, to) = unpack_value(v);
            if typ != TT_MATE {
                break;
            }
            let mut buf = new_buffer();
            let n = b.gen_moves(&mut buf);
            match buf[..n].iter().find(|m| same_move(**m, from, to)) {
                Some(&m) => {
                    pv.push(m);
                    b.do_move(m);
                }
                None => break,
            }
        }
        pv
    }

    /// Sorts the attacker's moves: table move, killer, checks, captures, the rest.
    fn order_attack(&self, b: &Board, moves: &mut [Move], ply: usize, tt_from: Pos, tt_to: Pos) {
        let killer = self.killer[ply];
        let first = |m: Move| same_move(m, tt_from, tt_to) || m == killer;
        let mut tmp = new_buffer();
        let mut k = 0;
        let mut push = |m: Move| {
            tmp[k] = m;
            k += 1;
        };
        for &m in moves.iter() {
            if same_move(m, tt_from, tt_to) {
                push(m);
            }
        }
        for &m in moves.iter() {
            if m == killer && !same_move(m, tt_from, tt_to) {
                push(m);
            }
        }
        for &m in moves.iter() {
            if !first(m) && b.gives_check(m) {
                push(m);
            }
        }
        for &m in moves.iter() {
            if !first(m) && m.capture != Piece::NONE && !b.gives_check(m) {
                push(m);
            }
        }
        for &m in moves.iter() {
            if !first(m) && m.capture == Piece::NONE && !b.gives_check(m) {
                push(m);
            }
        }
        moves.copy_from_slice(&tmp[..k]);
    }

    /// Sorts the defender's moves: table move (the known escape), killer,
    /// captures, the rest.
    fn order_defend(&self, moves: &mut [Move], ply: usize, tt_from: Pos, tt_to: Pos) {
        let killer = self.killer[ply];
        let first = |m: Move| same_move(m, tt_from, tt_to) || m == killer;
        let mut tmp = new_buffer();
        let mut k = 0;
        let mut push = |m: Move| {
            tmp[k] = m;
            k += 1;
        };
        for &m in moves.iter() {
            if same_move(m, tt_from, tt_to) {
                push(m);
            }
        }
        for &m in moves.iter() {
            if m == killer && !same_move(m, tt_from, tt_to) {
                push(m);
            }
        }
        for &m in moves.iter() {
            if !first(m) && m.capture != Piece::NONE {
                push(m);
            }
        }
        for &m in moves.iter() {
            if !first(m) && m.capture == Piece::NONE {
                push(m);
            }
        }
        moves.copy_from_slice(&tmp[..k]);
    }
}
