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

pub use oracle::{Material, Oracle, Tables, Verdict};
pub use ttvalue::VALUE_BITS;

use ttvalue::{pack_value, same_move, unpack_value, TT_MATE, TT_NO_MATE};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::bitboard::Board;
use crate::chess::{new_buffer, Move, Piece, Pos};
use crate::tt::{Key, TransTable};

/// Separates the entries of the searches in which black attacks (even depths
/// from a white root, odd depths from a black root), see `Searcher::key`.
const BLACK_ATTACKER_SALT: Key = [0x5bd1e9955bd1e995, 0x9e3779b97f4a7c15];

/// Bounds the search depth in plies (the TT depth field holds 7 bits).
pub const MAX_PLY: usize = 128;

/// The polling interval of the stop flag (a power of two).
pub const STOP_NODES: u64 = 1 << 12;

/// Returned by the node functions when no forced mate exists within the
/// remaining depth.
const NO_MATE: i32 = -1;

/// Result of one depth of `solve`. `mate_plies` is the mate distance in plies
/// (odd, attacker to move at the root), 0 if no mate was found within the depth.
/// `pv` is the principal variation; with a table it may be shorter than
/// `mate_plies` when the entries needed to extend it were replaced.
#[derive(Clone, Default)]
pub struct Result {
    /// Odd and positive when the side to move mates, even and negative when
    /// it is mated (`mated` searches), 0 if nothing was found within the depth.
    pub mate_plies: i32,
    pub pv: Vec<Move>,
    pub nodes: u64,
    /// Probes that ended the node (mate or refutation taken from the table).
    pub tt_hits: u64,
    /// Nodes decided by the oracle with an exact distance (endgame tables).
    pub oracle_hits: u64,
    /// The deepest completed depth.
    pub plies: u32,
    /// The stop flag was set: the result is that of the last completed depth.
    pub aborted: bool,
}

/// Per-search state: the oracle, the optional table, node counters, principal
/// variation (triangular table) and one killer move per ply, the move that most
/// recently mated or refuted at that ply and is tried first.
pub struct Searcher<O: Oracle, T: TransTable> {
    oracle: O,
    table: Option<T>,
    nodes: u64,
    tt_hits: u64,
    oracle_hits: u64,
    pv: Box<[[Move; MAX_PLY]; MAX_PLY]>,
    pv_len: [usize; MAX_PLY],
    killer: [Move; MAX_PLY],
    /// Called every `progress_every` nodes of a depth (a power of two,
    /// `PROGRESS_NODES` unless changed) with the node count and the table, for
    /// long searches.
    pub progress: Option<ProgressFn<T>>,
    pub progress_every: u64,
    /// Polled every `STOP_NODES` nodes: once true, the search unwinds without
    /// storing anything and `solve` returns the last completed depth with
    /// `aborted` set (UCI "stop" and time limits).
    pub stop: Option<Arc<AtomicBool>>,
    aborted: bool,
    /// Also searches the even depths, in which the side to move is the
    /// defender and the question is "is it mated within d plies" (UCI: the
    /// unavoidable mate against the engine is reported too). Off in the
    /// experiments, which count the attacker's nodes only.
    pub mated: bool,
    /// Key salt of the current depth (see `key`).
    salt: Key,
}

/// The progress callback type of `Searcher::progress`.
pub type ProgressFn<T> = Box<dyn FnMut(u64, Option<&T>)>;

/// The interval of progress calls (a power of two).
pub const PROGRESS_NODES: u64 = 1 << 26;

const NO_SQUARE: Pos = Pos(-1);

impl<O: Oracle, T: TransTable> Searcher<O, T> {
    /// A searcher that asks `oracle` at every node and uses `table` (`None` for none).
    pub fn new(oracle: O, table: Option<T>) -> Searcher<O, T> {
        Searcher { oracle, table, nodes: 0, tt_hits: 0, oracle_hits: 0, pv: Box::new([[Move::NONE; MAX_PLY]; MAX_PLY]), pv_len: [0; MAX_PLY], killer: [Move::NONE; MAX_PLY], progress: None, progress_every: PROGRESS_NODES, stop: None, aborted: false, mated: false, salt: [0; 2] }
    }

    /// The table key of a position. The entries of the searches in which
    /// black is the attacker are salted: the same position is an attacker
    /// node in one and a defender node in the other, and an entry "mate in d"
    /// of the one would be read as "mated in d" by the other.
    #[inline(always)]
    fn key(&self, b: &Board) -> Key {
        [b.key[0] ^ self.salt[0], b.key[1] ^ self.salt[1]]
    }

    /// Polls the stop flag every `STOP_NODES` nodes.
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
        let (mut total_nodes, mut total_hits, mut total_oracle) = (0u64, 0u64, 0u64);
        let mut last = Result::default();
        let step = if self.mated { 1 } else { 2 }; // odd depths: the side to move mates; even: it is mated
        let mut d = 1u32;
        while d <= max_plies && (d as usize) < MAX_PLY {
            let r = self.solve_depth(root, d);
            total_nodes += r.nodes;
            total_hits += r.tt_hits;
            total_oracle += r.oracle_hits;
            if r.aborted {
                last.nodes = total_nodes;
                last.tt_hits = total_hits;
                last.oracle_hits = total_oracle;
                last.aborted = true;
                break;
            }
            last = r;
            last.nodes = total_nodes;
            last.tt_hits = total_hits;
            last.oracle_hits = total_oracle;
            report(d, &last);
            if last.mate_plies != 0 {
                break;
            }
            d += step;
        }
        last
    }

    /// Searches one depth: with an odd number of plies "the side to move
    /// mates within plies", with an even one "the side to move is mated
    /// within plies" (the root is then a defender node). Killer moves
    /// persist between calls, the node counters are those of this depth.
    pub fn solve_depth(&mut self, root: &Board, plies: u32) -> Result {
        self.nodes = 0;
        self.tt_hits = 0;
        self.oracle_hits = 0;
        self.aborted = false;
        let defender = plies.is_multiple_of(2);
        // the table entries belong to the attacker's colour: a position is an
        // attacker node when that colour is to move, whichever root asks
        self.salt = if root.white_move == defender { BLACK_ATTACKER_SALT } else { [0; 2] };
        let dist = if defender { self.defend(root, plies, 0) } else { self.attack(root, plies, 0) };
        let mut r = Result { nodes: self.nodes, tt_hits: self.tt_hits, oracle_hits: self.oracle_hits, plies, aborted: self.aborted, ..Default::default() };
        if self.aborted {
            r.plies = 0;
            return r;
        }
        if dist != NO_MATE {
            r.mate_plies = if defender { -dist } else { dist };
            r.pv = self.extend_pv(root, dist as usize);
        }
        r
    }

    /// Probes the table for the node. Returns `Some(result)` when the entry ends
    /// the node, else the entry's move squares for ordering (or `NO_SQUARE`).
    #[inline(always)]
    fn probe(&mut self, b: &Board, depth: u32, ply: usize) -> std::result::Result<(Pos, Pos), i32> {
        let k = self.key(b);
        let Some(t) = self.table.as_mut() else { return Ok((NO_SQUARE, NO_SQUARE)) };
        let Some(v) = t.probe(k) else { return Ok((NO_SQUARE, NO_SQUARE)) };
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
        let k = self.key(b);
        if let Some(t) = self.table.as_mut() {
            t.store(k, pack_value(typ, depth, m));
        }
    }

    /// An OR node: the attacker is to move with `depth` plies left (odd).
    /// Returns the mate distance in plies or `NO_MATE`. At depth 1 only checking
    /// moves are tried, because a mating move is always a check.
    fn attack(&mut self, b: &Board, depth: u32, ply: usize) -> i32 {
        self.nodes += 1;
        if self.nodes & (self.progress_every - 1) == 0 {
            if let Some(p) = self.progress.as_mut() {
                p(self.nodes, self.table.as_ref());
            }
        }
        self.check_stop();
        if self.aborted {
            return NO_MATE;
        }
        match self.oracle.probe(b) {
            (Verdict::Draw | Verdict::Loss, _) => return NO_MATE,
            (Verdict::Win, plies) if plies > 0 => {
                // an endgame table knows the exact distance
                self.oracle_hits += 1;
                if plies > depth {
                    return NO_MATE;
                }
                self.pv_len[ply] = 0;
                return plies as i32;
            }
            _ => {}
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
            if self.aborted {
                return NO_MATE; // nothing is stored on the way out
            }
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
        match self.oracle.probe(b) {
            (Verdict::Draw | Verdict::Win, _) => return NO_MATE,
            (Verdict::Loss, plies) if plies > 0 => {
                // the defender is mated in plies, known exactly
                self.oracle_hits += 1;
                if plies > depth {
                    return NO_MATE;
                }
                self.pv_len[ply] = 0;
                return plies as i32;
            }
            _ => {}
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
            if self.aborted {
                return NO_MATE;
            }
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
        if self.table.is_none() || pv.len() >= mate_plies {
            return pv;
        }
        let mut b = *root;
        for &m in &pv {
            b.do_move(m);
        }
        let rest = self.tt_line(b, mate_plies - pv.len());
        pv.extend(rest);
        pv
    }

    /// Follows the moves the transposition table remembers from `b` (mating
    /// move at attacker nodes, longest defence at defender nodes) for up to
    /// `want` plies, until a missing entry.
    fn tt_line(&mut self, mut b: Board, want: usize) -> Vec<Move> {
        let mut line = Vec::new();
        while line.len() < want {
            let k = self.key(&b);
            let Some(t) = self.table.as_mut() else { break };
            let Some(v) = t.probe(k) else { break };
            let (typ, _, from, to) = unpack_value(v);
            if typ != TT_MATE {
                break;
            }
            let mut buf = new_buffer();
            let n = b.gen_moves(&mut buf);
            match buf[..n].iter().find(|m| same_move(**m, from, to)) {
                Some(&m) => {
                    line.push(m);
                    b.do_move(m);
                }
                None => break,
            }
        }
        line
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
