//! Generator-independent vocabulary shared by all board representations:
//! pieces, squares, moves, the validated FEN `Setup` and perft reference data.
//! Mirrors `mattjesGo/chess`.

mod fen;
mod mv;
mod perftdata;
mod piece;
mod pos;

#[allow(unused_imports)]
pub use fen::{Setup, START_FEN};
#[allow(unused_imports)]
pub use mv::{
    new_buffer, Castling, Move, MoveBuffer, ALL_CASTLING, BLACK_KINGSIDE, BLACK_QUEENSIDE, MAX_MOVES, WHITE_KINGSIDE, WHITE_QUEENSIDE,
};
#[allow(unused_imports)]
pub use perftdata::{PerftDetail, PerftPosition, PERFT_POSITIONS, UNKNOWN};
pub use piece::Piece;
pub use pos::{Pos, FIELD_COUNT, HEIGHT, WIDTH};
