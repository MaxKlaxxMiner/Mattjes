//! The move generator: 64-bit sets per piece type and color, magic bitboards for
//! sliders, fully legal generation via pin and check masks (no make/check/unmake
//! per move) and an incremental 128-bit Zobrist key. Direct port of `mattjesGo/bitboard`.
//!
//! Bit layout: bit i is square `Pos(i)`, so a8 = bit 0 and h1 = bit 63. Consequently
//! "north" (towards rank 8) is a right shift, unlike in most engines where a1 is bit 0.

mod bits;
mod board;
mod domove;
mod encode;
mod magic;
mod movegen;
mod perft;
mod tables;
mod zobrist;

// The public API is wider than what the currently enabled experiment uses.
#[allow(unused_imports)]
pub use board::{Board, State};
#[allow(unused_imports)]
pub use encode::{packed_fixed_record, Codec, Packed, PackedFixed};
#[allow(unused_imports)]
pub use perft::{perft_breadth, perft_breadth_encoded, perft_divide, perft_iterative, perft_parallel, perft_recursive, BOARD_SIZE, FRAME_SIZE};
#[allow(unused_imports)]
pub use tables::{init_duration, TABLES};
#[allow(unused_imports)]
pub use zobrist::{Key, KEY_WORDS};
