//! A minimalistic 8x8 mailbox board with a legal move generator, a direct port
//! of `mattjesGo/mailbox`. It is the first, simplest generator in Mattjes and
//! serves as the correctness reference for others.

mod attack;
mod board;
mod domove;
mod fen;
mod movegen;
mod perft;
mod tables;

// The public API is wider than what the currently enabled experiment uses.
#[allow(unused_imports)]
pub use board::{Board, State, ALL_CASTLING, BLACK_KINGSIDE, BLACK_QUEENSIDE, WHITE_KINGSIDE, WHITE_QUEENSIDE};
#[allow(unused_imports)]
pub use fen::START_FEN;
#[allow(unused_imports)]
pub use movegen::{new_buffer, Move, MoveBuffer, MAX_MOVES};
#[allow(unused_imports)]
pub use perft::{perft_breadth, perft_divide, perft_iterative, perft_parallel, perft_recursive, BOARD_SIZE, FRAME_SIZE};
