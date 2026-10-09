package chess

// RootMove is what a mate search knows about one root move after a depth:
// a proven mate with its length in plies and line (MatePlies 0 with Proven
// set when the length is unknown, i.e. the proof tree lost entries), or an
// unproven move. Searches return their root moves best first, so a GUI's
// MultiPV list stays complete even before a mate is found.
type RootMove struct {
	Move      Move
	Proven    bool
	MatePlies int
	PV        []Move
}
