package chess

// Outcome of a position from the point of view of the side to move, without
// the fifty-move rule.
type Outcome int8

const (
	Loss Outcome = -1
	Draw Outcome = 0
	Win  Outcome = 1
)

func (o Outcome) String() string {
	switch o {
	case Win:
		return "win"
	case Loss:
		return "loss"
	}
	return "draw"
}

// WdlPosition is a reference position for the WDL search (milestone 7):
// the result of the root and how many root moves win, draw and lose (the
// MultiPV expectation). Checked with Stockfish 19 and Syzygy on 2026-10-21
// (test-positions.md, "WDL-Teststellungen").
type WdlPosition struct {
	Name                string
	FEN                 string
	Result              Outcome
	Wins, Draws, Losses int
	Note                string
}

// WdlPositions are the WDL test positions, smallest material first within
// each group.
var WdlPositions = []WdlPosition{
	// wins or losses of the side to move
	{"KQ-KQ queen hangs", "8/8/8/4k3/8/8/4q3/K2Q4 w - - 0 1", Win, 1, 9, 8, "smoke test: Qxe2 in one ply"},
	{"KQ-K", "8/8/8/4k3/8/8/8/K2Q4 w - - 0 1", Win, 20, 3, 0, ""},
	{"KR-K", "8/8/8/4k3/8/8/8/K2R4 w - - 0 1", Win, 13, 3, 0, ""},
	{"KQ-KN", "7k/5n2/8/8/8/8/5Q2/K7 w - - 0 1", Win, 24, 1, 0, "Qxf7 is stalemate"},
	{"KBB-K", "8/8/4k3/8/8/8/8/K2BB3 w - - 0 1", Win, 17, 0, 0, ""},
	{"KBN-K", "8/8/8/8/3k4/8/N7/KB6 w - - 0 1", Win, 7, 3, 0, "long win proof without a mate distance"},
	{"KP-K king in front, white", "4k3/8/4K3/4P3/8/8/8/8 w - - 0 1", Win, 2, 2, 0, ""},
	{"KP-K king in front, black", "4k3/8/4K3/4P3/8/8/8/8 b - - 0 1", Loss, 0, 0, 2, "both moves lose"},
	{"KBP-K right bishop", "k7/8/K7/8/8/8/P7/5B2 w - - 0 1", Win, 11, 0, 0, ""},
	{"KQ-KR", "8/8/3k4/8/8/8/3r4/K2Q4 w - - 0 1", Win, 12, 0, 3, "three moves hang the queen"},
	{"KQ-KP knight pawn", "7K/8/8/1Q6/8/8/1p6/k7 w - - 0 1", Win, 4, 21, 0, "only four of 25 moves win"},
	{"KQQ-KN", "7k/5n2/8/8/8/8/Q4Q2/K7 w - - 0 1", Win, 37, 2, 0, ""},
	{"KRR-KN benchmark, black", "8/8/4k3/8/8/8/RK6/2R2n2 b - - 1 1", Loss, 0, 0, 12, "all twelve moves lose"},
	{"KRRN-KN lone knight, white", "8/8/4k3/8/8/8/R7/K1RN1n2 w - - 0 1", Win, 27, 0, 0, "the author's position: every move wins, the lone knight never does"},
	{"KRRN-KN lone knight, black", "8/8/4k3/8/8/4N3/R7/K1R2n2 b - - 1 1", Loss, 0, 0, 10, "after Ne3: the cost of disproving the lone knight's win"},
	{"KRRN-KNP seven pieces", "8/5p2/4k3/8/8/8/R7/K1RN1n2 w - - 0 1", Win, 27, 0, 0, "the lone-knight position plus a black pawn: beyond the six-piece Syzygy tables, every capture lands in them"},
	{"KQ-KBN", "8/8/4k3/3bn3/8/4Q3/8/K7 w - - 0 1", Win, 19, 1, 4, "mate in 39, out of reach without tables so far"},
	{"KQ-KRR", "k7/8/8/8/8/2r5/1r6/K5Q1 w - - 0 1", Win, 3, 8, 10, "only Kb2, Qg8, Qh1 win"},
	{"KQ-KRB", "k7/8/8/8/8/8/5rb1/K2Q4 w - - 0 1", Win, 2, 12, 7, "only Qd8 and Qa4 win"},
	// draws
	{"KP-K rook pawn", "k7/8/K7/8/8/8/P7/8 w - - 0 1", Draw, 0, 5, 0, "stalemate in the corner"},
	{"KP-K opposition", "8/8/8/8/8/4k3/4P3/4K3 w - - 0 1", Draw, 0, 2, 0, "the black king shuffles in front of the pawn"},
	{"KNN-K", "8/8/8/4k3/8/8/8/K1NN4 w - - 0 1", Draw, 0, 11, 0, "hard: disproving the win needs the whole reachable graph"},
	{"KBP-K wrong bishop", "k7/8/K7/8/8/8/P7/2B5 w - - 0 1", Draw, 0, 12, 0, "fortress in the corner"},
	{"KQ-KP c-pawn", "7K/8/8/Q7/8/8/2p5/1k6 w - - 0 1", Draw, 0, 22, 2, "stalemate trick Ka1; Qa1+ and Qa2+ lose the queen"},
	{"KQ-KP a-pawn", "7K/8/8/Q7/8/8/p7/1k6 w - - 0 1", Draw, 0, 23, 0, ""},
	{"KR-KB safe corner, black", "k7/8/K7/8/8/8/5b2/1R6 b - - 0 1", Draw, 0, 6, 3, "three bishop moves allow Rb8 mate"},
	{"KR-KB bishop guards b6", "k7/8/K7/8/8/4b3/8/1R6 w - - 0 1", Draw, 0, 16, 0, ""},
	{"KBP-KB opposite bishops", "8/8/8/3k4/8/2K5/2PB4/5b2 w - - 0 1", Draw, 0, 9, 0, "blockade"},
	{"KQ-KRP fortress", "8/8/5kp1/5r2/8/8/8/K2Q4 w - - 0 1", Draw, 0, 18, 5, "rook guarded by the pawn"},
	{"KQ-KRR perpetual check", "3k4/8/8/8/8/8/5rr1/K6Q w - - 0 1", Draw, 0, 4, 11, "only Qh4+, Qh8+, Qb1, Qd1 hold"},
	{"KRB-KR Philidor, black", "8/8/8/8/8/1k6/4r3/K1BR4 b - - 0 1", Draw, 0, 15, 4, ""},
	{"KNN-KP", "8/8/8/8/2k5/8/2p5/K1NN4 w - - 0 1", Draw, 0, 4, 6, "six of ten moves lose"},
	{"KQP-KRRP perpetual, four draws", "3k4/p7/8/8/8/8/P4rr1/K6Q w - - 0 1", Draw, 0, 4, 13, "the perpetual-check position plus a pawn each: seven pieces, every capture lands in the Syzygy tables; Qh4+, Qh8+, Qb1, Qd1+ hold"},
	{"KQ-KRRPP perpetual, one draw", "3k4/p3p3/8/8/8/8/5rr1/K6Q w - - 0 1", Draw, 0, 1, 14, "only Qh8+ holds"},
	{"KQP-KRRP perpetual, two draws", "3k4/p7/8/8/8/7P/5rr1/K6Q w - - 0 1", Draw, 0, 2, 8, "Qb1 and Qd1+ hold"},
}
