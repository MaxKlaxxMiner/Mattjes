package mateab

import (
	"math/bits"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
)

// Verdict is what an Oracle knows about a position, from the point of view of
// the side to move.
type Verdict uint8

const (
	Unknown Verdict = iota
	Draw
	Win  // the side to move mates
	Loss // the side to move gets mated
)

// Unbounded is the upper bound of a win or loss whose distance the oracle
// does not know at all (a WDL table): lo is 1, hi is Unbounded.
const Unbounded = 1 << 20

// Oracle answers terminal questions the search cannot answer by itself. For
// Win and Loss the true distance in plies lies in [lo, hi]: an exact endgame
// table gives lo == hi, a quantised one its bucket, a WDL table 1..Unbounded.
// The search treats a position as proven when hi fits into the remaining
// depth, as refuted when lo exceeds it, and expands it otherwise, so coarse
// tables cost search instead of being wrong. The material check is the first
// implementation; endgame tables (milestone 5) plug in here without touching
// the search.
type Oracle interface {
	Probe(b *bitboard.Board) (v Verdict, lo, hi int)
}

// Tables answers from the endgame tables (milestone 5) for every position
// with up to four pieces (and every larger material that is loaded), with
// the distance as exactly as the table stores it, and falls back to the
// material check for everything else. Positions with an en passant right are
// not in the tables; the search simply continues there.
type Tables struct {
	Set *egtb.Set
}

func (o Tables) Probe(b *bitboard.Board) (Verdict, int, int) {
	if v, lo, hi, ok := o.Set.LookupBounds(b); ok {
		switch {
		case v.IsWin():
			return Win, lo, hi
		case v.IsLoss():
			return Loss, lo, hi
		case v == egtb.Draw:
			return Draw, 0, 0
		}
		return Unknown, 0, 0 // invalid position, cannot happen for legal input
	}
	return Material{}.Probe(b)
}

// lightSquares has a8 (bit 0) set: a8 is a light square.
const lightSquares uint64 = 0xAA55AA55AA55AA55

// Material answers Draw for dead positions, in which no sequence of legal moves
// can lead to mate (FIDE 5.2.2): king against king, king and one minor piece
// against king, and king and bishop against king and bishop with both bishops
// on the same square colour. Everything else is Unknown; note that two knights
// against a bare king can still mate with the defender's help, so it is not dead.
type Material struct{}

func (Material) Probe(b *bitboard.Board) (Verdict, int, int) {
	heavy := b.PieceBB(chess.White|chess.Pawn) | b.PieceBB(chess.Black|chess.Pawn) |
		b.PieceBB(chess.White|chess.Rook) | b.PieceBB(chess.Black|chess.Rook) |
		b.PieceBB(chess.White|chess.Queen) | b.PieceBB(chess.Black|chess.Queen)
	if heavy != 0 {
		return Unknown, 0, 0
	}
	knights := b.PieceBB(chess.White|chess.Knight) | b.PieceBB(chess.Black|chess.Knight)
	bishops := b.PieceBB(chess.White|chess.Bishop) | b.PieceBB(chess.Black|chess.Bishop)
	switch bits.OnesCount64(knights | bishops) {
	case 0, 1:
		return Draw, 0, 0
	case 2:
		if knights == 0 && (bishops&lightSquares == 0 || bishops&^lightSquares == 0) {
			return Draw, 0, 0 // two bishops on the same colour, necessarily one per side (same side would be two bishops of one colour, which can mate)
		}
	}
	return Unknown, 0, 0
}
