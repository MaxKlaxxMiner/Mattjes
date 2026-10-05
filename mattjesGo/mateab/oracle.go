package mateab

import (
	"math/bits"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// Verdict is what an Oracle knows about a position, from the point of view of
// the side to move.
type Verdict uint8

const (
	Unknown Verdict = iota
	Draw
	Win  // the side to move mates; the distance in plies is given when known, else 0
	Loss // the side to move gets mated; distance as above
)

// Oracle answers terminal questions the search cannot answer by itself. The
// material check is the first implementation; endgame tables (milestone 5)
// plug in here without touching the search.
type Oracle interface {
	Probe(b *bitboard.Board) (Verdict, int)
}

// lightSquares has a8 (bit 0) set: a8 is a light square.
const lightSquares uint64 = 0xAA55AA55AA55AA55

// Material answers Draw for dead positions, in which no sequence of legal moves
// can lead to mate (FIDE 5.2.2): king against king, king and one minor piece
// against king, and king and bishop against king and bishop with both bishops
// on the same square colour. Everything else is Unknown; note that two knights
// against a bare king can still mate with the defender's help, so it is not dead.
type Material struct{}

func (Material) Probe(b *bitboard.Board) (Verdict, int) {
	heavy := b.PieceBB(chess.White|chess.Pawn) | b.PieceBB(chess.Black|chess.Pawn) |
		b.PieceBB(chess.White|chess.Rook) | b.PieceBB(chess.Black|chess.Rook) |
		b.PieceBB(chess.White|chess.Queen) | b.PieceBB(chess.Black|chess.Queen)
	if heavy != 0 {
		return Unknown, 0
	}
	knights := b.PieceBB(chess.White|chess.Knight) | b.PieceBB(chess.Black|chess.Knight)
	bishops := b.PieceBB(chess.White|chess.Bishop) | b.PieceBB(chess.Black|chess.Bishop)
	switch bits.OnesCount64(knights | bishops) {
	case 0, 1:
		return Draw, 0
	case 2:
		if knights == 0 && (bishops&lightSquares == 0 || bishops&^lightSquares == 0) {
			return Draw, 0 // two bishops on the same colour, necessarily one per side (same side would be two bishops of one colour, which can mate)
		}
	}
	return Unknown, 0
}
