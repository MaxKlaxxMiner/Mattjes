//! Generator-independent vocabulary shared by all board representations:
//! pieces, squares and perft reference data. Mirrors `mattjesGo/chess`.

mod perftdata;
mod piece;
mod pos;

#[allow(unused_imports)]
pub use perftdata::{PerftPosition, PERFT_POSITIONS};
pub use piece::Piece;
pub use pos::{Pos, FIELD_COUNT, HEIGHT, WIDTH};
