//! A minimalistic 8x8 mailbox board with a legal move generator, a direct port
//! of `mattjesGo/mailbox`. It is the first, simplest generator in Mattjes and
//! serves as the correctness reference for others.

mod attack;
mod board;
mod domove;
mod movegen;
mod perft;
mod tables;

// The public API is wider than what the currently enabled experiment uses.
#[allow(unused_imports)]
pub use board::{Board, State};
#[allow(unused_imports)]
pub use perft::{perft_breadth, perft_divide, perft_iterative, perft_parallel, perft_recursive, BOARD_SIZE, FRAME_SIZE};
