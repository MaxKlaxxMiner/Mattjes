package mailbox

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// Directions. The first four are orthogonal (rook), the last four diagonal (bishop).
const (
	dirN = iota // towards rank 8 (smaller index)
	dirS
	dirW
	dirE
	dirNW
	dirNE
	dirSW
	dirSE
)

var dirDelta = [8]chess.Pos{-chess.Width, +chess.Width, -1, +1, -chess.Width - 1, -chess.Width + 1, +chess.Width - 1, +chess.Width + 1}

// edgeDist[sq][dir] is the number of squares from sq to the board edge in that direction.
var edgeDist [chess.FieldCount][8]int8

// knightTargets[sq] and kingTargets[sq] list all on-board destination squares.
var knightTargets [chess.FieldCount][]chess.Pos
var kingTargets [chess.FieldCount][]chess.Pos

// castleClear[sq] holds the castling rights that are lost when a piece moves from or to sq.
var castleClear [chess.FieldCount]Castling

func init() {
	min := func(a, b int) int {
		if a < b {
			return a
		}
		return b
	}
	knightJumps := [8][2]int{{1, 2}, {2, 1}, {2, -1}, {1, -2}, {-1, -2}, {-2, -1}, {-2, 1}, {-1, 2}}

	for sq := 0; sq < chess.FieldCount; sq++ {
		x, y := sq%chess.Width, sq/chess.Width
		n, s, w, e := y, chess.Height-1-y, x, chess.Width-1-x
		edgeDist[sq] = [8]int8{int8(n), int8(s), int8(w), int8(e), int8(min(n, w)), int8(min(n, e)), int8(min(s, w)), int8(min(s, e))}

		for _, j := range knightJumps {
			if t := chess.PosFromXY(x+j[0], y+j[1]); t.Valid() {
				knightTargets[sq] = append(knightTargets[sq], t)
			}
		}
		for d := 0; d < 8; d++ {
			if edgeDist[sq][d] > 0 {
				kingTargets[sq] = append(kingTargets[sq], chess.Pos(sq)+dirDelta[d])
			}
		}
	}

	castleClear[0] = BlackQueenside
	castleClear[4] = BlackKingside | BlackQueenside
	castleClear[7] = BlackKingside
	castleClear[56] = WhiteQueenside
	castleClear[60] = WhiteKingside | WhiteQueenside
	castleClear[63] = WhiteKingside
}
