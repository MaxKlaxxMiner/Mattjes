/// A reference position with a known shortest mate. `mate_in` counts attacker
/// moves; the search depth in plies is 2*mate_in-1. `proof_positions` is the
/// size of one proof DAG of the shortest mate (attacker one shortest move,
/// defender all moves), counted by matelist; 0 = unknown. No search can prove
/// the mate with fewer positions, so nodes / proof_positions measures the
/// overhead of a search.
pub struct MatePosition {
    pub name: &'static str,
    pub fen: &'static str,
    pub mate_in: u32,
    pub proof_positions: usize,
    pub note: &'static str,
}

/// The test positions from the old C# code. The eight endgames (up to five
/// pieces) were confirmed against a tablebase on 2026-10-05, see
/// docs/m4-mate-search-design.md section 7. The pawn test is unconfirmed and
/// only checkable by the own search in both languages.
pub static MATE_POSITIONS: &[MatePosition] = &[
    MatePosition { name: "KQQ-K", fen: "8/8/8/4k3/8/Q7/Q7/K7 w - - 0 1", mate_in: 3, proof_positions: 0, note: "smoke test" },
    MatePosition { name: "KQR-K", fen: "8/8/8/4k3/8/Q7/R7/K7 w - - 0 1", mate_in: 5, proof_positions: 0, note: "" },
    MatePosition { name: "KRR-K", fen: "8/8/8/4k3/8/R7/R7/K7 w - - 0 1", mate_in: 7, proof_positions: 907, note: "ladder mate, quiet moves" },
    MatePosition { name: "KQ-KN", fen: "7k/5n2/8/8/8/8/5Q2/K7 w - - 0 1", mate_in: 12, proof_positions: 2687, note: "zugzwang, stalemate traps" },
    MatePosition { name: "KR-KR", fen: "8/5rK1/6R1/8/4k3/8/8/8 w - - 0 1", mate_in: 15, proof_positions: 0, note: "defender counterplay" },
    MatePosition { name: "KBB-K", fen: "8/8/4k3/8/8/8/8/K2BB3 w - - 0 1", mate_in: 17, proof_positions: 9825, note: "long quiet manoeuvres" },
    MatePosition { name: "KBN-K", fen: "8/8/8/8/3k4/8/N7/KB6 w - - 0 1", mate_in: 31, proof_positions: 29958, note: "needs df-pn or TT transpositions" },
    MatePosition { name: "KQ-KBN", fen: "8/8/4k3/3bn3/8/4Q3/8/K7 w - - 0 1", mate_in: 39, proof_positions: 0, note: "longest test, two defending pieces" },
    MatePosition { name: "KP-KP", fen: "8/7k/1p6/1P6/7K/8/8/8 w - - 0 1", mate_in: 25, proof_positions: 4125, note: "opposition study: 1.Kh5! wins, 1.Kg5? and black to move draw; value from the own KPKP table, confirmed by matelist" },
    MatePosition { name: "pawns", fen: "5k2/5P1P/4P3/pP6/P6q/3P2P1/2P5/K7 w - a6 0 1", mate_in: 6, proof_positions: 0, note: "promotion, en passant, black queen; unconfirmed" },
];
