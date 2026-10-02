package mailbox

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// IsAttacked reports whether square sq is attacked by any piece of color `by`.
func (b *Board) IsAttacked(sq chess.Pos, by chess.Piece) bool {
	dist := &edgeDist[sq]

	// pawns: a white pawn attacks diagonally towards rank 8, so it attacks sq from the south-west/south-east
	if by == chess.White {
		if (dist[dirSW] > 0 && b.Fields[sq+dirDelta[dirSW]] == chess.WhitePawn) ||
			(dist[dirSE] > 0 && b.Fields[sq+dirDelta[dirSE]] == chess.WhitePawn) {
			return true
		}
	} else {
		if (dist[dirNW] > 0 && b.Fields[sq+dirDelta[dirNW]] == chess.BlackPawn) ||
			(dist[dirNE] > 0 && b.Fields[sq+dirDelta[dirNE]] == chess.BlackPawn) {
			return true
		}
	}

	// knights and king
	knight, king := by|chess.Knight, by|chess.King
	for _, t := range knightTargets[sq] {
		if b.Fields[t] == knight {
			return true
		}
	}
	for _, t := range kingTargets[sq] {
		if b.Fields[t] == king {
			return true
		}
	}

	// sliders: first piece seen on each ray decides
	for d := 0; d < 8; d++ {
		slider := chess.Queen | chess.Rook
		if d >= dirNW {
			slider = chess.Queen | chess.Bishop
		}
		t := sq
		for n := dist[d]; n > 0; n-- {
			t += dirDelta[d]
			f := b.Fields[t]
			if f == chess.None {
				continue
			}
			if f&by != 0 && f&slider != 0 {
				return true
			}
			break
		}
	}
	return false
}

// InCheck reports whether the side to move is in check.
func (b *Board) InCheck() bool {
	return b.IsAttacked(b.KingPos(b.SideToMove()), chess.Opponent(b.SideToMove()))
}
