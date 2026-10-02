//! The switchable test runs for milestone 1. Enable them in `main`.

use crate::bitboard::Codec; // trait in scope for FastFen::append / Packed::append
use crate::{bitboard, chess, mailbox, perft};

fn workers_or_cpus(workers: usize) -> usize {
    if workers == 0 {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    } else {
        workers
    }
}

// --- mailbox generator ---

pub fn mailbox_perft_recursive(max_nodes: u64) {
    perft::run(
        "mailbox / recursive (make-unmake)",
        |fen, depth| {
            let mut b = mailbox::Board::from_fen(fen)?;
            Ok(mailbox::perft_recursive(&mut b, depth))
        },
        max_nodes,
    );
}

pub fn mailbox_perft_iterative(max_nodes: u64) {
    println!("frame size per ply: {} bytes (board {} bytes)", mailbox::FRAME_SIZE, mailbox::BOARD_SIZE);
    perft::run(
        "mailbox / iterative (explicit stack, copy-make)",
        |fen, depth| {
            let b = mailbox::Board::from_fen(fen)?;
            Ok(mailbox::perft_iterative(&b, depth))
        },
        max_nodes,
    );
}

pub fn mailbox_perft_breadth(max_nodes: u64, max_mb: usize) {
    println!("board size: {} bytes, memory limit: {} MB", mailbox::BOARD_SIZE, max_mb);
    perft::run(
        "mailbox / breadth-first (full position lists per ply)",
        |fen, depth| {
            let b = mailbox::Board::from_fen(fen)?;
            mailbox::perft_breadth(&b, depth, max_mb << 20)
        },
        max_nodes,
    );
}

pub fn mailbox_perft_parallel(max_nodes: u64, workers: usize) {
    let workers = workers_or_cpus(workers);
    perft::run(
        &format!("mailbox / parallel root split ({} workers)", workers),
        |fen, depth| {
            let b = mailbox::Board::from_fen(fen)?;
            Ok(mailbox::perft_parallel(&b, depth, workers))
        },
        max_nodes,
    );
}

pub fn mailbox_divide(fen: &str, depth: u32) {
    let mut b = mailbox::Board::from_fen(fen).expect("valid FEN");
    println!("{}\n\ndivide depth {}:", b, depth);
    mailbox::perft_divide(&mut b, depth);
}

// --- bitboard generator ---

fn bitboard_info() {
    let init = bitboard::init_duration();
    let entries = bitboard::TABLES.magics.table_len();
    println!("bitboard tables: {} magic entries ({} KB), init {:?}", entries, (entries * 8) >> 10, init);
}

pub fn bitboard_perft_recursive(max_nodes: u64) {
    bitboard_info();
    perft::run(
        "bitboard / recursive (make-unmake)",
        |fen, depth| {
            let mut b = bitboard::Board::from_fen(fen)?;
            Ok(bitboard::perft_recursive(&mut b, depth))
        },
        max_nodes,
    );
}

pub fn bitboard_perft_iterative(max_nodes: u64) {
    bitboard_info();
    println!("frame size per ply: {} bytes (board {} bytes)", bitboard::FRAME_SIZE, bitboard::BOARD_SIZE);
    perft::run(
        "bitboard / iterative (explicit stack, copy-make)",
        |fen, depth| {
            let b = bitboard::Board::from_fen(fen)?;
            Ok(bitboard::perft_iterative(&b, depth))
        },
        max_nodes,
    );
}

pub fn bitboard_perft_breadth(max_nodes: u64, max_mb: usize) {
    bitboard_info();
    println!("board size: {} bytes, memory limit: {} MB", bitboard::BOARD_SIZE, max_mb);
    perft::run(
        "bitboard / breadth-first (full position lists per ply)",
        |fen, depth| {
            let b = bitboard::Board::from_fen(fen)?;
            bitboard::perft_breadth(&b, depth, max_mb << 20)
        },
        max_nodes,
    );
}

pub fn bitboard_perft_breadth_encoded<C: bitboard::Codec>(max_nodes: u64, max_mb: usize) {
    bitboard_info();
    println!("codec {}: max {} bytes/position, memory limit: {} MB", C::NAME, C::MAX_BYTES, max_mb);
    perft::run(
        &format!("bitboard / breadth-first, {} encoded position streams", C::NAME),
        |fen, depth| {
            let b = bitboard::Board::from_fen(fen)?;
            bitboard::perft_breadth_encoded::<C>(&b, depth, max_mb << 20)
        },
        max_nodes,
    );
}

/// Checks both codecs on all reference positions one move deep.
pub fn bitboard_encode_roundtrip() {
    fn check<C: bitboard::Codec>(b: &bitboard::Board, fen: &str, m: chess::Move) {
        let mut enc = Vec::new();
        C::append(b, &mut enc);
        let (dec, n) = C::decode(&enc);
        if n != enc.len() || dec.setup().fen() != b.fen() || dec.pieces != b.pieces || dec.by_color != b.by_color {
            panic!("{} roundtrip failed for {} after {}", C::NAME, fen, m);
        }
    }
    for p in chess::PERFT_POSITIONS {
        let root = bitboard::Board::from_fen(p.fen).expect("valid FEN");
        for m in root.moves() {
            let mut b = root;
            b.do_move(m);
            check::<bitboard::FastFen>(&b, p.fen, m);
            check::<bitboard::Packed>(&b, p.fen, m);
            check::<bitboard::PackedFixed>(&b, p.fen, m);
        }
        let (mut ff, mut pk) = (Vec::new(), Vec::new());
        bitboard::FastFen::append(&root, &mut ff);
        bitboard::Packed::append(&root, &mut pk);
        println!("{:<36} fastfen {:2} bytes, packed {:2} bytes", p.name, ff.len(), pk.len());
    }
    println!("roundtrip ok");
}

pub fn bitboard_perft_parallel(max_nodes: u64, workers: usize) {
    bitboard_info();
    let workers = workers_or_cpus(workers);
    perft::run(
        &format!("bitboard / parallel root split ({} workers)", workers),
        |fen, depth| {
            let b = bitboard::Board::from_fen(fen)?;
            Ok(bitboard::perft_parallel(&b, depth, workers))
        },
        max_nodes,
    );
}

pub fn bitboard_divide(fen: &str, depth: u32) {
    let mut b = bitboard::Board::from_fen(fen).expect("valid FEN");
    println!("{}\n\ndivide depth {}:", b, depth);
    bitboard::perft_divide(&mut b, depth);
}
