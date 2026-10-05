use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};

use super::board::Board;
use super::encode::Codec;
use crate::chess::{new_buffer, Move, MoveBuffer};

/// Classic recursive perft with make/unmake on a single board. Leaf nodes are
/// bulk-counted: at depth 1 the number of legal moves is returned without playing them.
pub fn perft_recursive(b: &mut Board, depth: u32) -> u64 {
    let mut buf = new_buffer();
    let n = b.gen_moves(&mut buf);
    if depth <= 1 {
        return n as u64;
    }
    let mut total = 0;
    let s = b.state();
    for &m in &buf[..n] {
        b.do_move(m);
        total += perft_recursive(b, depth - 1);
        b.undo_move(m, s);
    }
    total
}

#[derive(Clone, Copy)]
struct Frame {
    board: Board,
    moves: MoveBuffer,
    count: usize,
    next: usize,
}

pub const FRAME_SIZE: usize = std::mem::size_of::<Frame>();
pub const BOARD_SIZE: usize = std::mem::size_of::<Board>();

/// List-based depth-first perft without recursion and without `undo_move`: every
/// ply owns a copy of the board ("copy-make"). Measured as fast as or faster than
/// make/unmake, see docs.
pub fn perft_iterative(root: &Board, depth: u32) -> u64 {
    let depth = depth as usize;
    let mut stack = vec![Frame { board: *root, moves: new_buffer(), count: 0, next: 0 }; depth.max(1)];
    {
        let f = &mut stack[0];
        f.count = f.board.gen_moves(&mut f.moves);
    }
    if depth <= 1 {
        return stack[0].count as u64;
    }

    let mut total = 0u64;
    let mut d = 0;
    loop {
        let (parents, children) = stack.split_at_mut(d + 1);
        let f = &mut parents[d];
        if f.next >= f.count {
            if d == 0 {
                return total;
            }
            d -= 1;
            continue;
        }
        let m = f.moves[f.next];
        f.next += 1;

        let child = &mut children[0];
        child.board = f.board;
        child.board.do_move(m);
        child.count = child.board.gen_moves(&mut child.moves);
        child.next = 0;
        if d + 1 == depth - 1 {
            total += child.count as u64;
        } else {
            d += 1;
        }
    }
}

/// Breadth-first perft: every ply is a complete list of all positions of that ply
/// (200-byte boards). Refuses to exceed `max_bytes`.
pub fn perft_breadth(root: &Board, depth: u32, max_bytes: usize) -> Result<u64, String> {
    let mut level = vec![*root];
    let mut buf = new_buffer();

    for ply in 1..depth {
        let mut child_count = 0;
        for b in level.iter() {
            child_count += b.gen_moves(&mut buf);
        }
        let need = child_count * BOARD_SIZE;
        if need > max_bytes {
            return Err(format!(
                "ply {} needs {} positions = {} MB, limit is {} MB",
                ply,
                child_count,
                need >> 20,
                max_bytes >> 20
            ));
        }
        let mut next = Vec::with_capacity(child_count);
        for b in level.iter() {
            let n = b.gen_moves(&mut buf);
            for &m in &buf[..n] {
                let mut child = *b;
                child.do_move(m);
                next.push(child);
            }
        }
        level = next;
    }

    let mut total = 0u64;
    for b in level.iter() {
        total += b.gen_moves(&mut buf) as u64;
    }
    Ok(total)
}

/// `perft_breadth` with every ply stored as one byte stream of compactly encoded
/// positions instead of a `Vec` of 184-byte boards. The codec is a type
/// parameter, so each instantiation is compiled separately with the codec inlined.
/// Prints the size of the largest ply.
pub fn perft_breadth_encoded<C: Codec>(root: &Board, depth: u32, max_bytes: usize) -> Result<u64, String> {
    let mut level = Vec::new();
    C::append(root, &mut level);
    let mut level_count = 1usize;
    let (mut peak_bytes, mut peak_count) = (level.len(), 1usize);
    let mut buf = new_buffer();

    for ply in 1..depth {
        // first pass: count children, estimate the stream size from the current average record size
        let mut child_count = 0usize;
        let mut i = 0;
        while i < level.len() {
            let (b, n) = C::decode(&level[i..]);
            i += n;
            child_count += b.gen_moves(&mut buf);
        }
        let estimate = child_count * level.len() / level_count + child_count;
        if estimate > max_bytes {
            return Err(format!("ply {} needs {} positions ≈ {} MB, limit is {} MB", ply, child_count, estimate >> 20, max_bytes >> 20));
        }
        let mut next = Vec::with_capacity(estimate);
        i = 0;
        while i < level.len() {
            let (b, n) = C::decode(&level[i..]);
            i += n;
            let moves = b.gen_moves(&mut buf);
            for &m in &buf[..moves] {
                let mut child = b;
                child.do_move(m);
                C::append(&child, &mut next);
            }
        }
        if next.len() > max_bytes {
            return Err(format!("ply {} needs {} MB, limit is {} MB", ply, next.len() >> 20, max_bytes >> 20));
        }
        level = next;
        level_count = child_count;
        if level.len() > peak_bytes {
            peak_bytes = level.len();
            peak_count = level_count;
        }
    }

    let mut total = 0u64;
    let mut i = 0;
    while i < level.len() {
        let (b, n) = C::decode(&level[i..]);
        i += n;
        total += b.gen_moves(&mut buf) as u64;
    }
    if peak_count > 1000 {
        println!(
            "             {}: largest ply {} positions in {:.1} MB = {:.1} bytes/position",
            C::NAME,
            peak_count,
            peak_bytes as f64 / (1u64 << 20) as f64,
            peak_bytes as f64 / peak_count as f64
        );
    }
    Ok(total)
}

/// Splits the root moves over `workers` threads, each running `perft_recursive` on
/// its own board copy. `thread::scope` lets the threads borrow `root` and the move
/// list without `Arc`.
pub fn perft_parallel(root: &Board, depth: u32, workers: usize) -> u64 {
    let mut buf = new_buffer();
    let n = root.gen_moves(&mut buf);
    if depth <= 1 {
        return n as u64;
    }
    let workers = workers.max(1);
    let moves: &[Move] = &buf[..n];
    let next_move = AtomicUsize::new(0);
    let total = AtomicU64::new(0);

    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                let mut sum = 0u64;
                loop {
                    let i = next_move.fetch_add(1, Relaxed);
                    if i >= moves.len() {
                        break;
                    }
                    let mut child = *root;
                    child.do_move(moves[i]);
                    sum += perft_recursive(&mut child, depth - 1);
                }
                total.fetch_add(sum, Relaxed);
            });
        }
    });
    total.load(Relaxed)
}

/// A perft count together with the classification of its leaf moves, the extra
/// columns of the chessprogramming.org tables.
#[derive(Clone, Copy, Default)]
pub struct PerftDetail {
    pub nodes: u64,
    pub detail: crate::chess::PerftDetail,
}

/// `perft_recursive` that additionally classifies every leaf move (capture, en
/// passant, castling, promotion, check kinds, mate). Checks are found with
/// `checkers_after` without playing the move; only checking moves are played to
/// test for mate with `has_moves`.
pub fn perft_detailed(b: &mut Board, depth: u32) -> PerftDetail {
    let mut d = PerftDetail::default();
    perft_detailed_rec(b, depth, &mut d);
    d
}

fn perft_detailed_rec(b: &mut Board, depth: u32, d: &mut PerftDetail) {
    use super::bits::{kind_idx, K_KING, K_PAWN};
    let mut buf = new_buffer();
    let n = b.gen_moves(&mut buf);
    let s = b.state();
    if depth > 1 {
        for &m in &buf[..n] {
            b.do_move(m);
            perft_detailed_rec(b, depth - 1, d);
            b.undo_move(m, s);
        }
        return;
    }
    d.nodes += n as u64;
    for &m in &buf[..n] {
        let p = b.squares[m.from.idx()];
        let is_ep = kind_idx(p) == K_PAWN && m.to == b.en_passant && b.en_passant.valid();
        if m.capture != crate::chess::Piece::NONE || is_ep {
            d.detail.captures += 1;
        }
        if is_ep {
            d.detail.en_passant += 1;
        }
        if kind_idx(p) == K_KING && (m.to.0 - m.from.0 == 2 || m.from.0 - m.to.0 == 2) {
            d.detail.castles += 1;
        }
        if m.promo != crate::chess::Piece::NONE {
            d.detail.promotions += 1;
        }
        let (checkers, direct) = b.checkers_after(m);
        if checkers == 0 {
            continue;
        }
        d.detail.checks += 1;
        let double = checkers & (checkers - 1) != 0;
        if !direct && !double {
            d.detail.discovery_checks += 1; // chessprogramming.org: a single checker that is not the moved piece
        }
        if double {
            d.detail.double_checks += 1;
        }
        b.do_move(m);
        if !b.has_moves() {
            d.detail.checkmates += 1;
        }
        b.undo_move(m, s);
    }
}

/// Prints the node count below every root move, the standard tool to locate move
/// generator bugs by comparing with another engine.
pub fn perft_divide(b: &mut Board, depth: u32) -> u64 {
    let mut buf = new_buffer();
    let n = b.gen_moves(&mut buf);
    let mut total = 0u64;
    let s = b.state();
    for &m in &buf[..n] {
        b.do_move(m);
        let sub = if depth > 1 { perft_recursive(b, depth - 1) } else { 1 };
        b.undo_move(m, s);
        println!("  {:<6} {:>12}", m.uci(), sub);
        total += sub;
    }
    println!("  total  {:>12} ({} moves)", total, n);
    total
}
