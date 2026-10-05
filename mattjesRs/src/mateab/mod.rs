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

mod oracle;

pub use oracle::{Material, Oracle, Verdict};

use crate::bitboard::Board;
use crate::chess::{new_buffer, Move, Piece};

/// Bounds the search depth in plies.
pub const MAX_PLY: usize = 128;

/// Returned by the node functions when no forced mate exists within the
/// remaining depth.
const NO_MATE: i32 = -1;

/// Result of one depth of `solve`. `mate_plies` is the mate distance in plies
/// (odd, attacker to move at the root), 0 if no mate was found within the depth.
#[derive(Clone, Default)]
pub struct Result {
    pub mate_plies: u32,
    pub pv: Vec<Move>,
    pub nodes: u64,
}

/// Per-search state: the oracle, node counter, principal variation (triangular
/// table) and one killer move per ply, the move that most recently mated or
/// refuted at that ply and is tried first.
pub struct Searcher<O: Oracle> {
    oracle: O,
    nodes: u64,
    pv: Box<[[Move; MAX_PLY]; MAX_PLY]>,
    pv_len: [usize; MAX_PLY],
    killer: [Move; MAX_PLY],
}

impl<O: Oracle> Searcher<O> {
    /// A searcher that asks `oracle` at every node.
    pub fn new(oracle: O) -> Searcher<O> {
        Searcher { oracle, nodes: 0, pv: Box::new([[Move::NONE; MAX_PLY]; MAX_PLY]), pv_len: [0; MAX_PLY], killer: [Move::NONE; MAX_PLY] }
    }

    /// Looks for a forced mate for the side to move within `max_plies` plies,
    /// with iterative deepening over odd depths. `report` is called after every
    /// depth; the returned `Result` is the first depth that found a mate (or the
    /// last depth, with `mate_plies` 0). Nodes accumulate over all depths.
    pub fn solve(&mut self, root: &Board, max_plies: u32, mut report: impl FnMut(u32, &Result)) -> Result {
        self.killer = [Move::NONE; MAX_PLY];
        let mut total = 0u64;
        let mut last = Result::default();
        let mut d = 1u32;
        while d <= max_plies && (d as usize) < MAX_PLY {
            self.nodes = 0;
            let dist = self.attack(root, d, 0);
            total += self.nodes;
            last = Result { nodes: total, ..Default::default() };
            if dist != NO_MATE {
                last.mate_plies = dist as u32;
                last.pv = self.pv[0][..self.pv_len[0]].to_vec();
            }
            report(d, &last);
            if dist != NO_MATE {
                break;
            }
            d += 2;
        }
        last
    }

    /// An OR node: the attacker is to move with `depth` plies left (odd).
    /// Returns the mate distance in plies or `NO_MATE`. At depth 1 only checking
    /// moves are tried, because a mating move is always a check.
    fn attack(&mut self, b: &Board, depth: u32, ply: usize) -> i32 {
        self.nodes += 1;
        if matches!(self.oracle.probe(b).0, Verdict::Draw | Verdict::Loss) {
            return NO_MATE;
        }
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
                    return 1;
                }
            }
            return NO_MATE;
        }
        let n = b.gen_moves(&mut buf);
        self.order_attack(b, &mut buf[..n], ply);
        for &m in &buf[..n] {
            let mut child = *b;
            child.do_move(m);
            let r = self.defend(&child, depth - 1, ply + 1);
            if r != NO_MATE {
                self.set_pv(ply, m);
                self.killer[ply] = m;
                return r + 1;
            }
        }
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
        self.order_defend(&mut buf[..n], ply);
        let mut worst = 0;
        for (i, &m) in buf[..n].iter().enumerate() {
            let mut child = *b;
            child.do_move(m);
            let r = self.attack(&child, depth - 1, ply + 1);
            if r == NO_MATE {
                self.killer[ply] = m;
                return NO_MATE;
            }
            if r > worst || i == 0 {
                worst = r;
                self.set_pv(ply, m);
            }
        }
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

    /// Sorts the attacker's moves: killer, checks, captures, the rest.
    fn order_attack(&self, b: &Board, moves: &mut [Move], ply: usize) {
        let killer = self.killer[ply];
        let mut tmp = new_buffer();
        let mut k = 0;
        let mut push = |m: Move| {
            tmp[k] = m;
            k += 1;
        };
        for &m in moves.iter() {
            if m == killer {
                push(m);
            }
        }
        for &m in moves.iter() {
            if m != killer && b.gives_check(m) {
                push(m);
            }
        }
        for &m in moves.iter() {
            if m != killer && m.capture != Piece::NONE && !b.gives_check(m) {
                push(m);
            }
        }
        for &m in moves.iter() {
            if m != killer && m.capture == Piece::NONE && !b.gives_check(m) {
                push(m);
            }
        }
        moves.copy_from_slice(&tmp[..k]);
    }

    /// Moves the killer (the move that refuted most recently at this ply) to the
    /// front, then captures.
    fn order_defend(&self, moves: &mut [Move], ply: usize) {
        let killer = self.killer[ply];
        let mut tmp = new_buffer();
        let mut k = 0;
        let mut push = |m: Move| {
            tmp[k] = m;
            k += 1;
        };
        for &m in moves.iter() {
            if m == killer {
                push(m);
            }
        }
        for &m in moves.iter() {
            if m != killer && m.capture != Piece::NONE {
                push(m);
            }
        }
        for &m in moves.iter() {
            if m != killer && m.capture == Piece::NONE {
                push(m);
            }
        }
        moves.copy_from_slice(&tmp[..k]);
    }
}
