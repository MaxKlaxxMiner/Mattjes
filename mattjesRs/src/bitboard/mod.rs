//! The second move generator: 64-bit sets per piece type and color, magic
//! bitboards for sliders and fully legal generation via pin and check masks
//! (no make/check/unmake per move). Direct port of `mattjesGo/bitboard`.
//!
//! Bit layout: bit i is square `Pos(i)`, so a8 = bit 0 and h1 = bit 63, the
//! same numbering as the mailbox board. Consequently "north" (towards rank 8)
//! is a right shift, unlike in most engines where a1 is bit 0.

mod bits;
mod board;
mod domove;
mod magic;
mod movegen;
mod perft;
mod tables;

#[allow(unused_imports)]
pub use board::{Board, State};
#[allow(unused_imports)]
pub use perft::{perft_breadth, perft_divide, perft_iterative, perft_parallel, perft_recursive, BOARD_SIZE, FRAME_SIZE};
#[allow(unused_imports)]
pub use tables::{init_duration, TABLES};
