package chess

// PerftPosition is a reference position with known node counts.
// Nodes[i] is the number of leaf nodes at depth i+1. Details[i], where present,
// are the extra columns of the chessprogramming.org tables for the same depth.
type PerftPosition struct {
	Name    string
	FEN     string
	Nodes   []uint64
	Details []PerftDetail
}

// PerftDetail classifies the leaf moves of one depth: how many capture, are en
// passant, castle, promote, give check and mate. Following the tables,
// DiscoveryChecks counts checks by exactly one piece that is not the moved one,
// DoubleChecks counts checks by two pieces (never both for one move). A cell
// the tables leave empty is Unknown and not compared. The counts verify
// GivesCheck, HasMoves and the move flags against foreign data.
type PerftDetail struct {
	Captures, EnPassant, Castles, Promotions          uint64
	Checks, DiscoveryChecks, DoubleChecks, Checkmates uint64
}

// Unknown marks a detail cell without reference value.
const Unknown = ^uint64(0)

// PerftPositions are the standard test positions from
// https://www.chessprogramming.org/Perft_Results
var PerftPositions = []PerftPosition{
	{
		Name:  "start position",
		FEN:   "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
		Nodes: []uint64{20, 400, 8902, 197281, 4865609, 119060324, 3195901860},
		Details: []PerftDetail{
			{0, 0, 0, 0, 0, 0, 0, 0},
			{0, 0, 0, 0, 0, 0, 0, 0},
			{34, 0, 0, 0, 12, 0, 0, 0},
			{1576, 0, 0, 0, 469, 0, 0, 8},
			{82719, 258, 0, 0, 27351, 6, 0, 347},
			{2812008, 5248, 0, 0, 809099, 329, 46, 10828},
			{108329926, 319617, 883453, 0, 33103848, 18026, 1628, 435767},
		},
	},
	{
		Name:  "kiwipete",
		FEN:   "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
		Nodes: []uint64{48, 2039, 97862, 4085603, 193690690, 8031647685},
		Details: []PerftDetail{
			{8, 0, 2, 0, 0, 0, 0, 0},
			{351, 1, 91, 0, 3, 0, 0, 0},
			{17102, 45, 3162, 0, 993, 0, 0, 1},
			{757163, 1929, 128013, 15172, 25523, 42, 6, 43},
			{35043416, 73365, 4993637, 8392, 3309887, 19883, 2645, 30171}, // double checks: the table says 2637 with a note that 2645 (Talkchess) may be right; 2645 is confirmed by recomputation after the move
			{1558445089, 3577504, 184513607, 56627920, 92238050, 568417, 54948, 360003},
		},
	},
	{
		Name:  "position 3 (endgame, en passant)",
		FEN:   "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
		Nodes: []uint64{14, 191, 2812, 43238, 674624, 11030083, 178633661, 3009794393},
		Details: []PerftDetail{
			{1, 0, 0, 0, 2, 0, 0, 0},
			{14, 0, 0, 0, 10, 0, 0, 0},
			{209, 2, 0, 0, 267, 3, 0, 0},
			{3348, 123, 0, 0, 1680, 106, 0, 17},
			{52051, 1165, 0, 0, 52950, 1292, 3, 0},
			{940350, 33325, 0, 7552, 452473, 26067, 0, 2733},
			{14519036, 294874, 0, 140024, 12797406, 370630, 3612, 87},
			{267586558, 8009239, 0, 6578076, 135626805, 7181487, 1630, 450410},
		},
	},
	{
		Name:  "position 4 (promotions)",
		FEN:   "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
		Nodes: []uint64{6, 264, 9467, 422333, 15833292, 706045033},
		Details: []PerftDetail{ // the table leaves discovery/double checks empty here
			{0, 0, 0, 0, 0, Unknown, Unknown, 0},
			{87, 0, 6, 48, 10, Unknown, Unknown, 0},
			{1021, 4, 0, 120, 38, Unknown, Unknown, 22},
			{131393, 0, 7795, 60032, 15492, Unknown, Unknown, Unknown},
			{2046173, 6512, 0, 329464, 200568, Unknown, Unknown, 50562},
			{210369132, 212, 10882006, 81102984, 26973664, Unknown, Unknown, 81076},
		},
	},
	{
		Name:  "position 4 mirrored",
		FEN:   "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1",
		Nodes: []uint64{6, 264, 9467, 422333, 15833292, 706045033},
	},
	{
		Name:  "position 5",
		FEN:   "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
		Nodes: []uint64{44, 1486, 62379, 2103487, 89941194},
	},
	{
		Name:  "position 6",
		FEN:   "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
		Nodes: []uint64{46, 2079, 89890, 3894594, 164075551, 6923051137},
	},
}
