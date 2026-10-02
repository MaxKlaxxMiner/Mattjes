package bitboard

import (
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

var knightAttacks, kingAttacks [chess.FieldCount]uint64

// pawnAttacks[color][sq] are the squares a pawn of that color on sq attacks.
// Mirror property: a pawn of color c on `from` attacks `to` iff from ∈ pawnAttacks[!c][to].
var pawnAttacks [2][chess.FieldCount]uint64

// between[a][b] are the squares strictly between a and b if they share a line, else 0.
// line[a][b] is the whole line through a and b (board edge to edge) if they share one, else 0.
var between, line [chess.FieldCount][chess.FieldCount]uint64

// castleClear[sq] holds the castling rights lost when a piece moves from or to sq.
var castleClear [chess.FieldCount]chess.Castling

// InitDuration is the time the package needed to build all tables (mostly the magic search).
var InitDuration time.Duration

// AttackTableSize is the number of entries in the magic attack table.
var AttackTableSize int

func init() {
	start := time.Now()

	for sq := chess.Pos(0); sq < chess.FieldCount; sq++ {
		b := bit(sq)
		knightAttacks[sq] = north(north(east(b))) | north(east(east(b))) | south(east(east(b))) | south(south(east(b))) |
			south(south(west(b))) | south(west(west(b))) | north(west(west(b))) | north(north(west(b)))
		kingAttacks[sq] = north(b) | south(b) | west(b) | east(b) | northWest(b) | northEast(b) | southWest(b) | southEast(b)
		pawnAttacks[0][sq] = northWest(b) | northEast(b)
		pawnAttacks[1][sq] = southWest(b) | southEast(b)
	}

	initMagics()
	AttackTableSize = len(attackTable)

	for a := chess.Pos(0); a < chess.FieldCount; a++ {
		for b := chess.Pos(0); b < chess.FieldCount; b++ {
			if a == b {
				continue
			}
			if rookAttacks(a, 0)&bit(b) != 0 {
				between[a][b] = rookAttacks(a, bit(b)) & rookAttacks(b, bit(a))
				line[a][b] = (rookAttacks(a, 0) & rookAttacks(b, 0)) | bit(a) | bit(b)
			} else if bishopAttacks(a, 0)&bit(b) != 0 {
				between[a][b] = bishopAttacks(a, bit(b)) & bishopAttacks(b, bit(a))
				line[a][b] = (bishopAttacks(a, 0) & bishopAttacks(b, 0)) | bit(a) | bit(b)
			}
		}
	}

	castleClear[0] = chess.BlackQueenside
	castleClear[4] = chess.BlackKingside | chess.BlackQueenside
	castleClear[7] = chess.BlackKingside
	castleClear[56] = chess.WhiteQueenside
	castleClear[60] = chess.WhiteKingside | chess.WhiteQueenside
	castleClear[63] = chess.WhiteKingside

	InitDuration = time.Since(start)
}
