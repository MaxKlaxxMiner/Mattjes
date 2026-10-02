use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};

use super::board::Board;
use super::movegen::{new_buffer, Move, MoveBuffer};

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

/// One ply of the explicit search stack used by `perft_iterative`.
#[derive(Clone, Copy)]
struct Frame {
    board: Board,
    moves: MoveBuffer,
    count: usize,
    next: usize,
}

/// Memory per ply of `perft_iterative` in bytes.
pub const FRAME_SIZE: usize = std::mem::size_of::<Frame>();

/// Memory per stored position in bytes.
pub const BOARD_SIZE: usize = std::mem::size_of::<Board>();

/// List-based depth-first perft without recursion and without `undo_move`:
/// every ply owns a copy of the board ("copy-make"), the child position is
/// created by copying the parent and playing one move.
pub fn perft_iterative(root: &Board, depth: u32) -> u64 {
    let depth = depth as usize;
    let mut stack = vec![Frame { board: *root, moves: new_buffer(), count: 0, next: 0 }; depth.max(1)];
    {
        // `stack[0].board.gen_moves(&mut stack[0].moves)` would index the Vec mutably
        // twice in one expression, which the borrow checker rejects. Borrowing the
        // frame once and then two *different fields* of it is fine.
        let f = &mut stack[0];
        f.count = f.board.gen_moves(&mut f.moves);
    }
    if depth <= 1 {
        return stack[0].count as u64;
    }

    let mut total = 0u64;
    let mut d = 0;
    loop {
        // Rust forbids two live `&mut` into the same Vec, so split it into the
        // parent part (..=d) and the child part (d+1..) and borrow one frame from each.
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
            total += child.count as u64; // bulk count, same as the recursive version
        } else {
            d += 1;
        }
    }
}

/// Breadth-first perft: every ply is a complete list of all positions of that ply.
/// It needs memory proportional to the number of nodes of the second-to-last ply
/// and refuses to exceed `max_bytes`. It exists to measure what storing whole
/// position lists costs.
pub fn perft_breadth(root: &Board, depth: u32, max_bytes: usize) -> Result<u64, String> {
    let mut level = vec![*root];
    let mut buf = new_buffer();

    for ply in 1..depth {
        // first pass: count the children so the next level can be allocated exactly once
        let mut child_count = 0;
        for b in level.iter_mut() {
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
        for b in level.iter_mut() {
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
    for b in level.iter_mut() {
        total += b.gen_moves(&mut buf) as u64;
    }
    Ok(total)
}

/// Splits the root moves over `workers` threads, each running `perft_recursive`
/// on its own board copy. `thread::scope` guarantees all threads finish before
/// the function returns, so they may borrow `root` and `moves` without `Arc`.
pub fn perft_parallel(root: &Board, depth: u32, workers: usize) -> u64 {
    let mut buf = new_buffer();
    let mut b = *root;
    let n = b.gen_moves(&mut buf);
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

/// Prints the node count below every root move. This is the standard tool to
/// locate move generator bugs by comparing with another engine.
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
