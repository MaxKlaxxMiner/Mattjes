package mailbox

import (
	"fmt"
	"strconv"
	"strings"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

const StartFEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"

// New returns the start position.
func New() Board {
	b, _ := FromFEN(StartFEN)
	return b
}

// FromFEN parses a FEN string. The halfmove clock and move number are optional.
func FromFEN(fen string) (Board, error) {
	var b Board
	err := b.SetFEN(fen)
	return b, err
}

// SetFEN replaces the position with the given FEN string.
func (b *Board) SetFEN(fen string) error {
	b.Clear()

	parts := strings.Fields(fen)
	if len(parts) < 4 || len(parts) > 6 {
		return fmt.Errorf("invalid FEN: expected 4 to 6 fields, got %d", len(parts))
	}

	// 1. pieces
	ranks := strings.Split(parts[0], "/")
	if len(ranks) != chess.Height {
		return fmt.Errorf("invalid FEN: expected %d ranks, got %d", chess.Height, len(ranks))
	}
	for y, rank := range ranks {
		x := 0
		for i := 0; i < len(rank); i++ {
			c := rank[i]
			if c >= '1' && c <= '8' {
				x += int(c - '0')
				continue
			}
			p := chess.PieceFromChar(c)
			if p == chess.Blocked {
				return fmt.Errorf("invalid FEN: unknown piece %q", c)
			}
			if x >= chess.Width {
				return fmt.Errorf("invalid FEN: rank %d too long", chess.Height-y)
			}
			b.SetField(chess.PosFromXY(x, y), p)
			x++
		}
		if x != chess.Width {
			return fmt.Errorf("invalid FEN: rank %d has %d files", chess.Height-y, x)
		}
	}
	if !b.WhiteKing.Valid() || !b.BlackKing.Valid() {
		return fmt.Errorf("invalid FEN: both kings are required")
	}

	// 2. side to move
	switch parts[1] {
	case "w":
		b.WhiteMove = true
	case "b":
		b.WhiteMove = false
	default:
		return fmt.Errorf("invalid FEN: side to move %q", parts[1])
	}

	// 3. castling rights (validated against king and rook squares)
	if parts[2] != "-" {
		for i := 0; i < len(parts[2]); i++ {
			var right Castling
			var kingSq, rookSq chess.Pos
			var king, rook chess.Piece
			switch parts[2][i] {
			case 'K':
				right, kingSq, rookSq, king, rook = WhiteKingside, 60, 63, chess.WhiteKing, chess.WhiteRook
			case 'Q':
				right, kingSq, rookSq, king, rook = WhiteQueenside, 60, 56, chess.WhiteKing, chess.WhiteRook
			case 'k':
				right, kingSq, rookSq, king, rook = BlackKingside, 4, 7, chess.BlackKing, chess.BlackRook
			case 'q':
				right, kingSq, rookSq, king, rook = BlackQueenside, 4, 0, chess.BlackKing, chess.BlackRook
			default:
				return fmt.Errorf("invalid FEN: castling %q", parts[2])
			}
			if b.Fields[kingSq] != king || b.Fields[rookSq] != rook {
				return fmt.Errorf("invalid FEN: castling right %c without king/rook on home squares", parts[2][i])
			}
			b.Castling |= right
		}
	}

	// 4. en passant (kept only if a pawn can actually capture, see DoMove)
	if parts[3] != "-" {
		ep := chess.PosFromString(parts[3])
		if !ep.Valid() {
			return fmt.Errorf("invalid FEN: en passant %q", parts[3])
		}
		wantY, pawnDir := 2, chess.Pos(chess.Width) // white to move: ep on rank 6, black pawn below it
		if !b.WhiteMove {
			wantY, pawnDir = 5, -chess.Width
		}
		if ep.Y() != wantY || b.Fields[ep] != chess.None || b.Fields[ep+pawnDir] != chess.Opponent(b.SideToMove())|chess.Pawn {
			return fmt.Errorf("invalid FEN: en passant %q does not match the pawns", parts[3])
		}
		if b.pawnCanCaptureEnPassant(ep + pawnDir) {
			b.EnPassant = ep
		}
	}

	// 5. halfmove clock
	if len(parts) > 4 {
		n, err := strconv.Atoi(parts[4])
		if err != nil || n < 0 || n > 9999 {
			return fmt.Errorf("invalid FEN: halfmove clock %q", parts[4])
		}
		b.HalfmoveClock = uint16(n)
	}

	// 6. move number
	if len(parts) > 5 {
		n, err := strconv.Atoi(parts[5])
		if err != nil || n < 1 || n > 9999 {
			return fmt.Errorf("invalid FEN: move number %q", parts[5])
		}
		b.MoveNumber = uint16(n)
	}

	return nil
}

// FEN returns the position as FEN string.
func (b *Board) FEN() string {
	var sb strings.Builder
	for y := 0; y < chess.Height; y++ {
		if y > 0 {
			sb.WriteByte('/')
		}
		empty := 0
		for x := 0; x < chess.Width; x++ {
			p := b.Fields[chess.PosFromXY(x, y)]
			if p == chess.None {
				empty++
				continue
			}
			if empty > 0 {
				sb.WriteByte(byte('0' + empty))
				empty = 0
			}
			sb.WriteByte(p.Char())
		}
		if empty > 0 {
			sb.WriteByte(byte('0' + empty))
		}
	}

	if b.WhiteMove {
		sb.WriteString(" w ")
	} else {
		sb.WriteString(" b ")
	}

	if b.Castling == 0 {
		sb.WriteByte('-')
	} else {
		for i, c := range []byte("KQkq") {
			if b.Castling&(1<<i) != 0 {
				sb.WriteByte(c)
			}
		}
	}

	sb.WriteByte(' ')
	sb.WriteString(b.EnPassant.String())
	sb.WriteString(" " + strconv.Itoa(int(b.HalfmoveClock)) + " " + strconv.Itoa(int(b.MoveNumber)))
	return sb.String()
}
