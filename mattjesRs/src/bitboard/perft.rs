use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};

use super::board::Board;
use crate::chess::{new_buffer, Move, MoveBuffer};

// The perft variants mirror module mailbox exactly, see there for the descriptions.

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
