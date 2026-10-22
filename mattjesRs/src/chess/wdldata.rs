//! Reference positions for the WDL search (milestone 7). Mirrors
//! `mattjesGo/chess/wdldata.go`.

use std::fmt;

/// Outcome of a position from the point of view of the side to move, without
/// the fifty-move rule.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Loss,
    Draw,
    Win,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Outcome::Win => "win",
            Outcome::Loss => "loss",
            Outcome::Draw => "draw",
        })
    }
}

/// A reference position for the WDL search: the result of the root and how
/// many root moves win, draw and lose (the MultiPV expectation). Checked with
/// Stockfish 19 and Syzygy on 2026-10-21 (test-positions.md).
pub struct WdlPosition {
    pub name: &'static str,
    pub fen: &'static str,
    pub result: Outcome,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    pub note: &'static str,
}

const fn p(name: &'static str, fen: &'static str, result: Outcome, wins: u32, draws: u32, losses: u32, note: &'static str) -> WdlPosition {
    WdlPosition { name, fen, result, wins, draws, losses, note }
}

/// The WDL test positions, smallest material first within each group.
pub static WDL_POSITIONS: &[WdlPosition] = &[
    // wins or losses of the side to move
    p("KQ-KQ queen hangs", "8/8/8/4k3/8/8/4q3/K2Q4 w - - 0 1", Outcome::Win, 1, 9, 8, "smoke test: Qxe2 in one ply"),
    p("KQ-K", "8/8/8/4k3/8/8/8/K2Q4 w - - 0 1", Outcome::Win, 20, 3, 0, ""),
    p("KR-K", "8/8/8/4k3/8/8/8/K2R4 w - - 0 1", Outcome::Win, 13, 3, 0, ""),
    p("KQ-KN", "7k/5n2/8/8/8/8/5Q2/K7 w - - 0 1", Outcome::Win, 24, 1, 0, "Qxf7 is stalemate"),
    p("KBB-K", "8/8/4k3/8/8/8/8/K2BB3 w - - 0 1", Outcome::Win, 17, 0, 0, ""),
    p("KBN-K", "8/8/8/8/3k4/8/N7/KB6 w - - 0 1", Outcome::Win, 7, 3, 0, "long win proof without a mate distance"),
    p("KP-K king in front, white", "4k3/8/4K3/4P3/8/8/8/8 w - - 0 1", Outcome::Win, 2, 2, 0, ""),
    p("KP-K king in front, black", "4k3/8/4K3/4P3/8/8/8/8 b - - 0 1", Outcome::Loss, 0, 0, 2, "both moves lose"),
    p("KBP-K right bishop", "k7/8/K7/8/8/8/P7/5B2 w - - 0 1", Outcome::Win, 11, 0, 0, ""),
    p("KQ-KR", "8/8/3k4/8/8/8/3r4/K2Q4 w - - 0 1", Outcome::Win, 12, 0, 3, "three moves hang the queen"),
    p("KQ-KP knight pawn", "7K/8/8/1Q6/8/8/1p6/k7 w - - 0 1", Outcome::Win, 4, 21, 0, "only four of 25 moves win"),
    p("KQQ-KN", "7k/5n2/8/8/8/8/Q4Q2/K7 w - - 0 1", Outcome::Win, 37, 2, 0, ""),
    p("KRR-KN benchmark, black", "8/8/4k3/8/8/8/RK6/2R2n2 b - - 1 1", Outcome::Loss, 0, 0, 12, "all twelve moves lose"),
    p("KRRN-KN lone knight, white", "8/8/4k3/8/8/8/R7/K1RN1n2 w - - 0 1", Outcome::Win, 27, 0, 0, "the author's position: every move wins, the lone knight never does"),
    p("KRRN-KN lone knight, black", "8/8/4k3/8/8/4N3/R7/K1R2n2 b - - 1 1", Outcome::Loss, 0, 0, 10, "after Ne3: the cost of disproving the lone knight's win"),
    p("KRRN-KNP seven pieces", "8/5p2/4k3/8/8/8/R7/K1RN1n2 w - - 0 1", Outcome::Win, 27, 0, 0, "the lone-knight position plus a black pawn: beyond the six-piece Syzygy tables, every capture lands in them"),
    p("KQ-KBN", "8/8/4k3/3bn3/8/4Q3/8/K7 w - - 0 1", Outcome::Win, 19, 1, 4, "mate in 39, out of reach without tables so far"),
    p("KQ-KRR", "k7/8/8/8/8/2r5/1r6/K5Q1 w - - 0 1", Outcome::Win, 3, 8, 10, "only Kb2, Qg8, Qh1 win"),
    p("KQ-KRB", "k7/8/8/8/8/8/5rb1/K2Q4 w - - 0 1", Outcome::Win, 2, 12, 7, "only Qd8 and Qa4 win"),
    // draws
    p("KP-K rook pawn", "k7/8/K7/8/8/8/P7/8 w - - 0 1", Outcome::Draw, 0, 5, 0, "stalemate in the corner"),
    p("KP-K opposition", "8/8/8/8/8/4k3/4P3/4K3 w - - 0 1", Outcome::Draw, 0, 2, 0, "the black king shuffles in front of the pawn"),
    p("KNN-K", "8/8/8/4k3/8/8/8/K1NN4 w - - 0 1", Outcome::Draw, 0, 11, 0, "hard: disproving the win needs the whole reachable graph"),
    p("KBP-K wrong bishop", "k7/8/K7/8/8/8/P7/2B5 w - - 0 1", Outcome::Draw, 0, 12, 0, "fortress in the corner"),
    p("KQ-KP c-pawn", "7K/8/8/Q7/8/8/2p5/1k6 w - - 0 1", Outcome::Draw, 0, 22, 2, "stalemate trick Ka1; Qa1+ and Qa2+ lose the queen"),
    p("KQ-KP a-pawn", "7K/8/8/Q7/8/8/p7/1k6 w - - 0 1", Outcome::Draw, 0, 23, 0, ""),
    p("KR-KB safe corner, black", "k7/8/K7/8/8/8/5b2/1R6 b - - 0 1", Outcome::Draw, 0, 6, 3, "three bishop moves allow Rb8 mate"),
    p("KR-KB bishop guards b6", "k7/8/K7/8/8/4b3/8/1R6 w - - 0 1", Outcome::Draw, 0, 16, 0, ""),
    p("KBP-KB opposite bishops", "8/8/8/3k4/8/2K5/2PB4/5b2 w - - 0 1", Outcome::Draw, 0, 9, 0, "blockade"),
    p("KQ-KRP fortress", "8/8/5kp1/5r2/8/8/8/K2Q4 w - - 0 1", Outcome::Draw, 0, 18, 5, "rook guarded by the pawn"),
    p("KQ-KRR perpetual check", "3k4/8/8/8/8/8/5rr1/K6Q w - - 0 1", Outcome::Draw, 0, 4, 11, "only Qh4+, Qh8+, Qb1, Qd1 hold"),
    p("KRB-KR Philidor, black", "8/8/8/8/8/1k6/4r3/K1BR4 b - - 0 1", Outcome::Draw, 0, 15, 4, ""),
    p("KNN-KP", "8/8/8/8/2k5/8/2p5/K1NN4 w - - 0 1", Outcome::Draw, 0, 4, 6, "six of ten moves lose"),
];
