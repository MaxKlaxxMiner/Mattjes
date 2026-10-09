//! The list-based mate search: breadth-first enumeration of every position
//! reachable from the root (deduplicated, each expanded exactly once), then
//! retrograde analysis over that finite graph. Working title from the design
//! document. Direct port of `mattjesGo/matelist`.
//!
//! Where the depth-first search with iterative deepening re-searches the same
//! positions once per depth (KBN-K: 192 million expansions for 13 million
//! positions), this touches every position once forwards and every edge once
//! backwards. The result is the exact mate distance for every reachable
//! position, not only for the root, so the principal variation is complete. For
//! few pieces this is already the endgame table of milestone 5, restricted to the
//! positions reachable from the root and keyed by hash instead of by index.
//!
//! Memory: a 32-byte record per position, 16 bytes per store slot, 4 bytes per
//! edge (child list during enumeration, parent list afterwards) plus a few bytes
//! of counters per position.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::bitboard::{packed_fixed_record, Board, Codec, PackedFixed};
use crate::chess::{new_buffer, Move, RootMove};
use crate::ttstore::{Store, MAX_LOAD_PERCENT};

type Record = [u8; PackedFixed::MAX_BYTES];

/// Result of `solve`. `mate_plies` is the mate distance of the root in plies for
/// the side to move (0 = no forced mate within the enumerated graph). `pv` is
/// the complete principal variation.
#[derive(Clone, Default)]
pub struct Result {
    pub mate_plies: u32,
    pub pv: Vec<Move>,
    /// The root moves best first: the mating ones by length, the rest in
    /// generation order.
    pub root: Vec<RootMove>,
    /// Enumerated positions.
    pub positions: usize,
    /// Positions whose moves were generated (all but the horizon layer).
    pub expanded: usize,
    pub edges: usize,
    /// Breadth-first depth reached.
    pub plies: u32,
    /// Positions with a known mate distance for either side.
    pub resolved: usize,
    /// Longest mate distance found anywhere in the graph.
    pub max_level: u32,
    /// One proof DAG of the root's mate: the attacker plays one shortest move per
    /// position, the defender every move. No search can prove the mate with fewer
    /// positions than the smallest such DAG, so this bounds what a smarter forward
    /// search could save against brute force.
    pub proof_positions: usize,
    pub proof_edges: usize,
}

// status per position: 0 unknown, 1..127 side to move wins (mates) in n plies,
// 128|n side to move loses (is mated) in n plies. Mate itself is lose in 0.
const UNKNOWN: u8 = 0;
const LOSE_FLAG: u8 = 128;

fn win_in(n: u32) -> u8 {
    n as u8
}

fn lose_in(n: u32) -> u8 {
    LOSE_FLAG | n as u8
}

/// Enumerates the positions reachable from `root` within `max_plies` (every
/// position once, whichever ply reaches it first), gives up with an error beyond
/// `max_positions`, and resolves mate distances backwards. The horizon layer is
/// not expanded; positions whose proof would need it stay unknown, which is safe.
/// `progress` receives one line per phase and per breadth-first ply. `stop`
/// (optional) is polled before every ply: once set, the enumeration ends at
/// that ply boundary and the backward phase resolves what was reached.
pub fn solve(root: &Board, max_plies: u32, max_positions: usize, stop: Option<&AtomicBool>, progress: &mut dyn FnMut(&str)) -> std::result::Result<Result, String> {
    let mut slots = 1024;
    while slots * MAX_LOAD_PERCENT / 100 < max_positions {
        slots *= 2;
    }
    let mut store = Store::with_slots(slots);

    let mut recs: Vec<Record> = Vec::with_capacity(1 << 16);
    let mut status: Vec<u8> = Vec::with_capacity(1 << 16);
    let mut child_count: Vec<u8> = Vec::with_capacity(1 << 16); // legal moves of the position (0 at the horizon or when there are none)
    let mut parent_count: Vec<u32> = Vec::with_capacity(1 << 16); // edges arriving at the position
    let mut child_start: Vec<u32> = Vec::with_capacity(1 << 16); // CSR offsets into children; positions are expanded in index order
    let mut children: Vec<u32> = Vec::with_capacity(1 << 20); // child index per edge, in expansion order
    fn add(recs: &mut Vec<Record>, status: &mut Vec<u8>, child_count: &mut Vec<u8>, parent_count: &mut Vec<u32>, b: &Board) {
        recs.push(packed_fixed_record(b));
        status.push(UNKNOWN);
        child_count.push(0);
        parent_count.push(0);
    }
    store.put(root.key, 0);
    add(&mut recs, &mut status, &mut child_count, &mut parent_count, root);

    // --- forward: breadth-first enumeration, remembering the child edges ---
    let mut buf = new_buffer();
    let mut mates: Vec<u32> = Vec::new(); // positions where the side to move is mated (level 0)
    let mut level: Vec<u32> = vec![0];
    let mut plies = 0u32;
    while plies < max_plies && !level.is_empty() {
        if stop.is_some_and(|s| s.load(Ordering::Relaxed)) {
            break; // the current level becomes the horizon
        }
        let mut next: Vec<u32> = Vec::new();
        for &i in &level {
            child_start.push(children.len() as u32);
            let b = PackedFixed::decode(&recs[i as usize]).0;
            let n = b.gen_moves(&mut buf);
            child_count[i as usize] = n as u8;
            if n == 0 {
                if b.in_check() {
                    status[i as usize] = lose_in(0);
                    mates.push(i);
                }
                continue;
            }
            for &m in &buf[..n] {
                let mut child = b;
                child.do_move(m);
                let (idx, is_new, ok) = store.get_or_put(child.key, recs.len() as u64);
                if !ok {
                    return Err(format!("more than {} positions at ply {}", max_positions, plies + 1));
                }
                if is_new {
                    add(&mut recs, &mut status, &mut child_count, &mut parent_count, &child);
                    next.push(idx as u32);
                }
                parent_count[idx as usize] += 1;
                children.push(idx as u32);
            }
        }
        plies += 1;
        progress(&format!("ply {:>3}: {:>12} new positions, {:>12} total, {:>12} edges, {} mates so far", plies, next.len(), recs.len(), children.len(), mates.len()));
        level = next;
    }
    let edges = children.len();
    let expanded = child_start.len(); // the last level (horizon or empty) was not expanded
    child_start.push(edges as u32);
    if !level.is_empty() {
        progress(&format!("horizon at ply {}: {} positions not expanded", plies, level.len()));
    }

    // --- parent lists (CSR) by counting sort of the child edges, no generator, no hash ---
    let mut parent_start: Vec<u32> = vec![0; recs.len() + 1];
    for (i, &c) in parent_count.iter().enumerate() {
        parent_start[i + 1] = parent_start[i] + c;
    }
    let mut parents: Vec<u32> = vec![0; edges];
    let mut fill = parent_count; // reuse as cursor
    fill.iter_mut().for_each(|f| *f = 0);
    for p in 0..expanded {
        for &c in &children[child_start[p] as usize..child_start[p + 1] as usize] {
            parents[(parent_start[c as usize] + fill[c as usize]) as usize] = p as u32;
            fill[c as usize] += 1;
        }
    }
    drop(children); // no longer needed; the retrograde walks parents only
    drop(child_start);
    progress(&format!("parent lists: {} edges for {} positions", edges, recs.len()));

    // --- backward: retrograde levels ---
    let mut remaining = child_count; // open children per position; a position is lost when all children are won by the opponent
    let mut queue = mates;
    let mut resolved = queue.len();
    let mut max_level = 0u32;
    let mut n = 0u32;
    while !queue.is_empty() {
        let mut next: Vec<u32> = Vec::new();
        if n.is_multiple_of(2) {
            // queue holds "lose in n": every parent that is still unknown wins in n+1
            for &c in &queue {
                for &p in &parents[parent_start[c as usize] as usize..parent_start[c as usize + 1] as usize] {
                    if status[p as usize] == UNKNOWN {
                        status[p as usize] = win_in(n + 1);
                        next.push(p);
                    }
                }
            }
        } else {
            // queue holds "win in n": a parent loses in n+1 once all its children are won
            for &c in &queue {
                for &p in &parents[parent_start[c as usize] as usize..parent_start[c as usize + 1] as usize] {
                    remaining[p as usize] -= 1;
                    if remaining[p as usize] == 0 && status[p as usize] == UNKNOWN {
                        status[p as usize] = lose_in(n + 1);
                        next.push(p);
                    }
                }
            }
        }
        if !next.is_empty() {
            max_level = n + 1;
            resolved += next.len();
        }
        queue = next;
        n += 1;
    }
    progress(&format!("retrograde: {} of {} positions resolved, longest mate {} plies", resolved, recs.len(), max_level));

    let mut r = Result { positions: recs.len(), expanded, edges, plies, resolved, max_level, ..Default::default() };
    r.root = root_moves(root, &store, &status);
    let s = status[0];
    if s != UNKNOWN && s & LOSE_FLAG == 0 {
        r.mate_plies = s as u32;
        r.pv = principal_variation(root, 0, &store, &status);
        (r.proof_positions, r.proof_edges) = proof_size(&recs, &store, &status);
        progress(&format!("proof DAG: {} positions, {} edges (attacker one shortest move, defender all moves)", r.proof_positions, r.proof_edges));
    }
    Ok(r)
}

/// Walks one proof DAG of the root's mate breadth-first: at a winning position
/// the first child that loses in n-1 is taken, at a losing position all children.
/// Returns the number of distinct positions and edges in it.
fn proof_size(recs: &[Record], store: &Store, status: &[u8]) -> (usize, usize) {
    let mut in_proof = vec![false; recs.len()];
    in_proof[0] = true;
    let mut queue: Vec<u32> = vec![0];
    let (mut positions, mut edges) = (1usize, 0usize);
    let mut buf = new_buffer();
    while !queue.is_empty() {
        let mut next: Vec<u32> = Vec::new();
        for &i in &queue {
            let s = status[i as usize];
            if s == lose_in(0) {
                continue;
            }
            let b = PackedFixed::decode(&recs[i as usize]).0;
            let n = b.gen_moves(&mut buf);
            let attacker = s & LOSE_FLAG == 0;
            let want = lose_in(s as u32 - 1);
            for &m in &buf[..n] {
                let mut child = b;
                child.do_move(m);
                let Some(idx) = store.get(child.key) else { continue };
                let idx = idx as usize;
                if attacker && status[idx] != want {
                    continue;
                }
                edges += 1;
                if !in_proof[idx] {
                    in_proof[idx] = true;
                    positions += 1;
                    next.push(idx as u32);
                }
                if attacker {
                    break;
                }
            }
        }
        queue = next;
    }
    (positions, edges)
}

/// Follows the mate distances from the root: the winner picks a child that
/// loses in n-1, the loser a child that wins in n-1.
/// Lists the root moves best first: those that mate by the length of the
/// mate, then the rest in generation order (the graph knows them as draws,
/// losses or unresolved).
fn root_moves(root: &Board, store: &Store, status: &[u8]) -> Vec<RootMove> {
    let mut buf = new_buffer();
    let n = root.gen_moves(&mut buf);
    let mut out: Vec<RootMove> = Vec::with_capacity(n);
    for &m in &buf[..n] {
        let mut child = *root;
        child.do_move(m);
        let mut e = RootMove { mv: m, ..Default::default() };
        if let Some(idx) = store.get(child.key) {
            let s = status[idx as usize];
            if s & LOSE_FLAG != 0 {
                e.proven = true;
                e.mate_plies = (s & !LOSE_FLAG) as u32 + 1;
                e.pv = std::iter::once(m).chain(principal_variation(&child, idx as usize, store, status)).collect();
            }
        }
        out.push(e);
    }
    out.sort_by(|a, b| b.proven.cmp(&a.proven).then_with(|| if a.proven { a.mate_plies.cmp(&b.mate_plies) } else { std::cmp::Ordering::Equal }));
    out
}

/// Follows the mate distances from the position at index `cur`: the winner
/// picks a child that loses in n-1, the loser a child that wins in n-1.
fn principal_variation(root: &Board, mut cur: usize, store: &Store, status: &[u8]) -> Vec<Move> {
    let mut pv = Vec::new();
    let mut b = *root;
    let mut buf = new_buffer();
    loop {
        let s = status[cur];
        if s == UNKNOWN || s == lose_in(0) {
            return pv;
        }
        let want = if s & LOSE_FLAG == 0 { lose_in(s as u32 - 1) } else { win_in((s & !LOSE_FLAG) as u32 - 1) };
        let n = b.gen_moves(&mut buf);
        let mut found = false;
        for &m in &buf[..n] {
            let mut child = b;
            child.do_move(m);
            if let Some(idx) = store.get(child.key) {
                if status[idx as usize] == want {
                    pv.push(m);
                    b = child;
                    cur = idx as usize;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            return pv;
        }
    }
}
