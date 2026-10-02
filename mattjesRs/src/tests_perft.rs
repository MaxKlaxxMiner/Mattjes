//! The switchable test runs for milestone 1. Enable them in `main`.

use crate::bitboard::{self, Board, Codec};
use crate::{chess, perft};

fn workers_or_cpus(workers: usize) -> usize {
    if workers == 0 {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    } else {
        workers
    }
}

fn bitboard_info() {
    let init = bitboard::init_duration();
    let entries = bitboard::TABLES.magics.table_len();
    println!(
        "bitboard tables: {} magic entries ({} KB), init {:?}, zobrist {} bit, board {} bytes",
        entries,
        (entries * 8) >> 10,
        init,
        bitboard::KEY_WORDS * 64,
        bitboard::BOARD_SIZE
    );
}

pub fn bitboard_perft_recursive(max_nodes: u64) {
    bitboard_info();
    perft::run(
        "bitboard / recursive (make-unmake)",
        |fen, depth| {
            let mut b = Board::from_fen(fen)?;
            Ok(bitboard::perft_recursive(&mut b, depth))
        },
        max_nodes,
    );
}

pub fn bitboard_perft_iterative(max_nodes: u64) {
    bitboard_info();
    println!("frame size per ply: {} bytes", bitboard::FRAME_SIZE);
    perft::run(
        "bitboard / iterative (explicit stack, copy-make)",
        |fen, depth| {
            let b = Board::from_fen(fen)?;
            Ok(bitboard::perft_iterative(&b, depth))
        },
        max_nodes,
    );
}

pub fn bitboard_perft_breadth(max_nodes: u64, max_mb: usize) {
    bitboard_info();
    println!("memory limit: {} MB", max_mb);
    perft::run(
        "bitboard / breadth-first (full position lists per ply)",
        |fen, depth| {
            let b = Board::from_fen(fen)?;
            bitboard::perft_breadth(&b, depth, max_mb << 20)
        },
        max_nodes,
    );
}

pub fn bitboard_perft_breadth_encoded<C: Codec>(max_nodes: u64, max_mb: usize) {
    bitboard_info();
    println!("codec {}: max {} bytes/position, memory limit: {} MB", C::NAME, C::MAX_BYTES, max_mb);
    perft::run(
        &format!("bitboard / breadth-first, {} encoded position streams", C::NAME),
        |fen, depth| {
            let b = Board::from_fen(fen)?;
            bitboard::perft_breadth_encoded::<C>(&b, depth, max_mb << 20)
        },
        max_nodes,
    );
}

/// Checks both codecs (including the Zobrist key) on all reference positions one move deep.
pub fn bitboard_encode_roundtrip() {
    fn check<C: Codec>(b: &Board, fen: &str, m: chess::Move) {
        let mut enc = Vec::new();
        C::append(b, &mut enc);
        let (dec, n) = C::decode(&enc);
        if n != enc.len() || dec.setup().fen() != b.fen() || dec.pieces != b.pieces || dec.by_color != b.by_color || dec.key != b.key {
            panic!("{} roundtrip failed for {} after {}", C::NAME, fen, m);
        }
    }
    for p in chess::PERFT_POSITIONS {
        let root = Board::from_fen(p.fen).expect("valid FEN");
        for m in root.moves() {
            let mut b = root;
            b.do_move(m);
            check::<bitboard::Packed>(&b, p.fen, m);
            check::<bitboard::PackedFixed>(&b, p.fen, m);
        }
        let mut pk = Vec::new();
        bitboard::Packed::append(&root, &mut pk);
        println!("{:<36} packed {:2} bytes", p.name, pk.len());
    }
    println!("roundtrip ok");
}

pub fn bitboard_perft_parallel(max_nodes: u64, workers: usize) {
    bitboard_info();
    let workers = workers_or_cpus(workers);
    perft::run(
        &format!("bitboard / parallel root split ({} workers)", workers),
        |fen, depth| {
            let b = Board::from_fen(fen)?;
            Ok(bitboard::perft_parallel(&b, depth, workers))
        },
        max_nodes,
    );
}

pub fn bitboard_divide(fen: &str, depth: u32) {
    let mut b = Board::from_fen(fen).expect("valid FEN");
    println!("{}\n\ndivide depth {}:", b, depth);
    bitboard::perft_divide(&mut b, depth);
}
