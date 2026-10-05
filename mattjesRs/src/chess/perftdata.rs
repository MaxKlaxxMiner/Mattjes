/// A reference position with known node counts. `nodes[i]` is the number of leaf
/// nodes at depth i+1. `details[i]`, where present, are the extra columns of the
/// chessprogramming.org tables for the same depth.
pub struct PerftPosition {
    pub name: &'static str,
    pub fen: &'static str,
    pub nodes: &'static [u64],
    pub details: &'static [PerftDetail],
}

/// Classifies the leaf moves of one depth: how many capture, are en passant,
/// castle, promote, give check and mate. Following the tables,
/// `discovery_checks` counts checks by exactly one piece that is not the moved
/// one, `double_checks` counts checks by two pieces (never both for one move).
/// A cell the tables leave empty is `UNKNOWN` and not compared.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct PerftDetail {
    pub captures: u64,
    pub en_passant: u64,
    pub castles: u64,
    pub promotions: u64,
    pub checks: u64,
    pub discovery_checks: u64,
    pub double_checks: u64,
    pub checkmates: u64,
}

/// Marks a detail cell without reference value.
pub const UNKNOWN: u64 = u64::MAX;

#[allow(clippy::too_many_arguments)]
const fn d(captures: u64, en_passant: u64, castles: u64, promotions: u64, checks: u64, discovery_checks: u64, double_checks: u64, checkmates: u64) -> PerftDetail {
    PerftDetail { captures, en_passant, castles, promotions, checks, discovery_checks, double_checks, checkmates }
}

/// The standard test positions from https://www.chessprogramming.org/Perft_Results
pub static PERFT_POSITIONS: &[PerftPosition] = &[
    PerftPosition {
        name: "start position",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        nodes: &[20, 400, 8902, 197281, 4865609, 119060324, 3195901860],
        details: &[
            d(0, 0, 0, 0, 0, 0, 0, 0),
            d(0, 0, 0, 0, 0, 0, 0, 0),
            d(34, 0, 0, 0, 12, 0, 0, 0),
            d(1576, 0, 0, 0, 469, 0, 0, 8),
            d(82719, 258, 0, 0, 27351, 6, 0, 347),
            d(2812008, 5248, 0, 0, 809099, 329, 46, 10828),
            d(108329926, 319617, 883453, 0, 33103848, 18026, 1628, 435767),
        ],
    },
    PerftPosition {
        name: "kiwipete",
        fen: "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        nodes: &[48, 2039, 97862, 4085603, 193690690, 8031647685],
        details: &[
            d(8, 0, 2, 0, 0, 0, 0, 0),
            d(351, 1, 91, 0, 3, 0, 0, 0),
            d(17102, 45, 3162, 0, 993, 0, 0, 1),
            d(757163, 1929, 128013, 15172, 25523, 42, 6, 43),
            // double checks: the table says 2637 with a note that 2645 (Talkchess) may be right;
            // 2645 is confirmed by recomputation after the move
            d(35043416, 73365, 4993637, 8392, 3309887, 19883, 2645, 30171),
            d(1558445089, 3577504, 184513607, 56627920, 92238050, 568417, 54948, 360003),
        ],
    },
    PerftPosition {
        name: "position 3 (endgame, en passant)",
        fen: "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        nodes: &[14, 191, 2812, 43238, 674624, 11030083, 178633661, 3009794393],
        details: &[
            d(1, 0, 0, 0, 2, 0, 0, 0),
            d(14, 0, 0, 0, 10, 0, 0, 0),
            d(209, 2, 0, 0, 267, 3, 0, 0),
            d(3348, 123, 0, 0, 1680, 106, 0, 17),
            d(52051, 1165, 0, 0, 52950, 1292, 3, 0),
            d(940350, 33325, 0, 7552, 452473, 26067, 0, 2733),
            d(14519036, 294874, 0, 140024, 12797406, 370630, 3612, 87),
            d(267586558, 8009239, 0, 6578076, 135626805, 7181487, 1630, 450410),
        ],
    },
    PerftPosition {
        name: "position 4 (promotions)",
        fen: "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        nodes: &[6, 264, 9467, 422333, 15833292, 706045033],
        details: &[
            // the table leaves discovery/double checks empty here
            d(0, 0, 0, 0, 0, UNKNOWN, UNKNOWN, 0),
            d(87, 0, 6, 48, 10, UNKNOWN, UNKNOWN, 0),
            d(1021, 4, 0, 120, 38, UNKNOWN, UNKNOWN, 22),
            d(131393, 0, 7795, 60032, 15492, UNKNOWN, UNKNOWN, UNKNOWN),
            d(2046173, 6512, 0, 329464, 200568, UNKNOWN, UNKNOWN, 50562),
            d(210369132, 212, 10882006, 81102984, 26973664, UNKNOWN, UNKNOWN, 81076),
        ],
    },
    PerftPosition {
        name: "position 4 mirrored",
        fen: "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
        nodes: &[6, 264, 9467, 422333, 15833292, 706045033],
        details: &[],
    },
    PerftPosition {
        name: "position 5",
        fen: "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        nodes: &[44, 1486, 62379, 2103487, 89941194],
        details: &[],
    },
    PerftPosition {
        name: "position 6",
        fen: "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
        nodes: &[46, 2079, 89890, 3894594, 164075551, 6923051137],
        details: &[],
    },
];
