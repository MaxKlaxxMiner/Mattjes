package chess

// MatePosition is a reference position with a known shortest mate.
// MateIn counts attacker moves; the search depth in plies is 2*MateIn-1.
type MatePosition struct {
	Name   string
	FEN    string
	MateIn int
	Note   string
}

// MatePositions are the test positions from the old C# code. The eight endgames
// (up to five pieces) were confirmed against a tablebase on 2026-10-05, see
// docs/m4-mate-search-design.md section 7. The pawn test is unconfirmed and
// only checkable by the own search in both languages.
var MatePositions = []MatePosition{
	{"KQQ-K", "8/8/8/4k3/8/Q7/Q7/K7 w - - 0 1", 3, "smoke test"},
	{"KQR-K", "8/8/8/4k3/8/Q7/R7/K7 w - - 0 1", 5, ""},
	{"KRR-K", "8/8/8/4k3/8/R7/R7/K7 w - - 0 1", 7, "ladder mate, quiet moves"},
	{"KQ-KN", "7k/5n2/8/8/8/8/5Q2/K7 w - - 0 1", 12, "zugzwang, stalemate traps"},
	{"KR-KR", "8/5rK1/6R1/8/4k3/8/8/8 w - - 0 1", 15, "defender counterplay"},
	{"KBB-K", "8/8/4k3/8/8/8/8/K2BB3 w - - 0 1", 17, "long quiet manoeuvres"},
	{"KBN-K", "8/8/8/8/3k4/8/N7/KB6 w - - 0 1", 31, "needs df-pn or TT transpositions"},
	{"KQ-KBN", "8/8/4k3/3bn3/8/4Q3/8/K7 w - - 0 1", 39, "longest test, two defending pieces"},
	{"pawns", "5k2/5P1P/4P3/pP6/P6q/3P2P1/2P5/K7 w - a6 0 1", 6, "promotion, en passant, black queen; unconfirmed"},
}
