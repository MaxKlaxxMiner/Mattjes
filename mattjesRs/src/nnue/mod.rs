//! A native port of Patricia's evaluation network (Patricia 5.1 by Adam
//! Kulju, MIT licence, `mattjesGo/nnue/nets/LICENSE-Patricia`), mirroring
//! `mattjesGo/nnue`: the three nets are embedded, the accumulator is
//! updated incrementally per move, the output is Patricia's raw score for
//! the side to move. See the Go package for the architecture notes
//! ((768 -> 1024)x2 -> 1, SCReLU, QA 255, QB 64, SCALE 400, three nets
//! picked by material).

mod accumulator;

pub use accumulator::{simd, Accumulator, Delta};

use std::sync::OnceLock;

use crate::bitboard::Board;
use crate::chess::{Piece, Pos};

/// 2 colours x 6 kinds x 64 squares.
pub const INPUT_SIZE: usize = 768;
/// Neurons per perspective.
pub const HIDDEN_SIZE: usize = 1024;
/// Scale of the feature layer (and the SCReLU clamp).
pub const QA: i32 = 255;
/// Scale of the output layer.
pub const QB: i32 = 64;
/// Centipawn scale of the output.
pub const SCALE: i64 = 400;
/// The total material (`material`) below which Patricia switches to the endgame net.
pub const PHASE_BOUND: i32 = 3500;

/// Selects one of the three nets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    /// fingolfin.nnue: Patricia's search uses it when it is behind.
    Middlegame = 0,
    /// finarfin.nnue: material below PHASE_BOUND.
    Endgame = 1,
    /// feanor.nnue: the default net above PHASE_BOUND.
    Sacrifice = 2,
}

impl Phase {
    pub const ALL: [Phase; 3] = [Phase::Middlegame, Phase::Endgame, Phase::Sacrifice];
}

/// One network's parameters in Patricia's memory layout: the feature
/// weights row by row (one row of HIDDEN_SIZE per input feature), the
/// hidden biases, the output weights (the side to move's half first), the
/// output bias. The file is exactly these arrays as little-endian int16.
pub struct Net {
    pub feature: Vec<i16>,
    pub bias: Vec<i16>,
    pub output: Vec<i16>,
    pub output_bias: i16,
}

/// The size of a net file.
pub const FILE_BYTES: usize = (INPUT_SIZE * HIDDEN_SIZE + HIDDEN_SIZE + 2 * HIDDEN_SIZE + 1) * 2;

static FINGOLFIN: &[u8] = include_bytes!("../../../mattjesGo/nnue/nets/fingolfin.nnue");
static FINARFIN: &[u8] = include_bytes!("../../../mattjesGo/nnue/nets/finarfin.nnue");
static FEANOR: &[u8] = include_bytes!("../../../mattjesGo/nnue/nets/feanor.nnue");

static NETS: OnceLock<[Net; 3]> = OnceLock::new();

/// The net of a phase (parsed from the embedded file on first use).
pub fn load(phase: Phase) -> &'static Net {
    &NETS.get_or_init(|| [parse(FINGOLFIN), parse(FINARFIN), parse(FEANOR)])[phase as usize]
}

fn parse(raw: &[u8]) -> Net {
    assert_eq!(raw.len(), FILE_BYTES, "nnue: net file has the wrong size");
    let values: Vec<i16> = raw.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
    let (feature, rest) = values.split_at(INPUT_SIZE * HIDDEN_SIZE);
    let (bias, rest) = rest.split_at(HIDDEN_SIZE);
    let (output, rest) = rest.split_at(2 * HIDDEN_SIZE);
    Net { feature: feature.to_vec(), bias: bias.to_vec(), output: output.to_vec(), output_bias: rest[0] }
}

impl Net {
    /// The feature weights of one input feature.
    pub fn row(&self, feature: usize) -> &[i16] {
        &self.feature[feature * HIDDEN_SIZE..(feature + 1) * HIDDEN_SIZE]
    }
}

/// The bitboard kind index of a piece (trailing zero of its type bit:
/// king 0, queen 1, rook 2, bishop 3, knight 4, pawn 5).
fn kind_idx(p: Piece) -> usize {
    (p.0 & Piece::TYPE_MASK.0).trailing_zeros() as usize
}

/// The input feature of a piece on a square from white's and from black's
/// perspective. Patricia numbers the squares a1 = 0 .. h8 = 63 and its
/// kinds pawn 0 .. king 5 (the reverse of the bitboard order), Mattjes
/// a8 = 0 .. h1 = 63, so Patricia's square is pos ^ 56; black's
/// perspective flips the board vertically and swaps the colours.
pub fn feature_indices(p: Piece, sq: Pos) -> (usize, usize) {
    let kind = 5 - kind_idx(p);
    let colour = if p.0 & Piece::COLOR_MASK.0 == Piece::BLACK.0 { 1 } else { 0 };
    let psq = sq.idx() ^ 56;
    (colour * 384 + kind * 64 + psq, (colour ^ 1) * 384 + kind * 64 + (psq ^ 56))
}

/// Patricia's total material of both sides (pawn 100, knight and bishop
/// 300, rook 500, queen 900), the phase criterion.
pub fn material(b: &Board) -> i32 {
    let values = [0, 900, 500, 300, 300, 100];
    (0..2).map(|side| (0..6).map(|kind| values[kind] * b.pieces[side][kind].count_ones() as i32).sum::<i32>()).sum()
}

/// The net Patricia starts a search with: the endgame net below
/// PHASE_BOUND, the sacrifice net above.
pub fn phase_of(b: &Board) -> Phase {
    if material(b) < PHASE_BOUND {
        Phase::Endgame
    } else {
        Phase::Sacrifice
    }
}

/// Evaluates board b from scratch with the net of a phase.
pub fn raw(b: &Board, phase: Phase) -> i32 {
    let net = load(phase);
    let mut acc = Accumulator::default();
    acc.refresh(net, b);
    acc.evaluate(net, b.white_move)
}

/// Patricia's static evaluation of a position as its search sees it at
/// the root: the raw score of the net `phase_of` picks, scaled by the
/// material ((750 + material/25)/1024, a tenth less when the side to move
/// has no queen or the material is below 4000 and the score is in its
/// favour). Patricia's search adds history-dependent terms on top that
/// have no meaning for a single position.
pub fn evaluate(b: &Board) -> i32 {
    let raw = raw(b, phase_of(b));
    let material = material(b);
    // as in Patricia: material/25 in integers, the rest in float, the
    // tenth subtracted in double precision
    let mut multiplier = (750 + material / 25) as f32 / 1024.0;
    let side = if b.white_move { 0 } else { 1 };
    let no_queen = b.pieces[side][1] == 0;
    if no_queen || material < 4000 && raw > 0 {
        multiplier = (multiplier as f64 - 0.1) as f32;
    }
    (raw as f32 * multiplier) as i32
}
