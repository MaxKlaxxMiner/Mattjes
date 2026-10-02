package chess

import (
	"fmt"
	"strconv"
	"strings"
)

const StartFEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"

// Setup is a validated, representation-independent position. Every board type
// converts from and to it, so FEN parsing and validation exist only once.
//
// Invariants guaranteed after ParseFEN (generators may rely on them):
//   - exactly one king per color, no pawns on rank 1 or 8
//   - castling rights only with king and rook on their home squares
//   - EnPassant is set only if a pawn of the side to move can actually capture there
type Setup struct {
	Squares       [FieldCount]Piece
	WhiteMove     bool
	Castling      Castling
	EnPassant     Pos
	HalfmoveClock uint16
	MoveNumber    uint16
}

// SideToMove returns the color to move.
func (s *Setup) SideToMove() Piece {
	if s.WhiteMove {
		return White
	}
	return Black
}

// ParseFEN parses and validates a FEN string. Halfmove clock and move number are optional.
func ParseFEN(fen string) (Setup, error) {
	s := Setup{WhiteMove: true, EnPassant: NoPos, MoveNumber: 1}

	parts := strings.Fields(fen)
	if len(parts) < 4 || len(parts) > 6 {
		return s, fmt.Errorf("invalid FEN: expected 4 to 6 fields, got %d", len(parts))
	}

	// 1. pieces
	ranks := strings.Split(parts[0], "/")
	if len(ranks) != Height {
		return s, fmt.Errorf("invalid FEN: expected %d ranks, got %d", Height, len(ranks))
	}
	kings := [2]int{}
	for y, rank := range ranks {
		x := 0
		for i := 0; i < len(rank); i++ {
			c := rank[i]
			if c >= '1' && c <= '8' {
				x += int(c - '0')
				continue
			}
			p := PieceFromChar(c)
			if p == Blocked {
				return s, fmt.Errorf("invalid FEN: unknown piece %q", c)
			}
			if x >= Width {
				return s, fmt.Errorf("invalid FEN: rank %d too long", Height-y)
			}
			if p.Is(Pawn) && (y == 0 || y == Height-1) {
				return s, fmt.Errorf("invalid FEN: pawn on rank %d", Height-y)
			}
			if p.Is(King) {
				kings[p>>7]++
			}
			s.Squares[PosFromXY(x, y)] = p
			x++
		}
		if x != Width {
			return s, fmt.Errorf("invalid FEN: rank %d has %d files", Height-y, x)
		}
	}
	if kings[0] != 1 || kings[1] != 1 {
		return s, fmt.Errorf("invalid FEN: expected exactly one king per color")
	}

	// 2. side to move
	switch parts[1] {
	case "w":
		s.WhiteMove = true
	case "b":
		s.WhiteMove = false
	default:
		return s, fmt.Errorf("invalid FEN: side to move %q", parts[1])
	}

	// 3. castling rights (validated against king and rook squares)
	if parts[2] != "-" {
		for i := 0; i < len(parts[2]); i++ {
			var right Castling
			var kingSq, rookSq Pos
			var king, rook Piece
			switch parts[2][i] {
			case 'K':
				right, kingSq, rookSq, king, rook = WhiteKingside, 60, 63, WhiteKing, WhiteRook
			case 'Q':
				right, kingSq, rookSq, king, rook = WhiteQueenside, 60, 56, WhiteKing, WhiteRook
			case 'k':
				right, kingSq, rookSq, king, rook = BlackKingside, 4, 7, BlackKing, BlackRook
			case 'q':
				right, kingSq, rookSq, king, rook = BlackQueenside, 4, 0, BlackKing, BlackRook
			default:
				return s, fmt.Errorf("invalid FEN: castling %q", parts[2])
			}
			if s.Squares[kingSq] != king || s.Squares[rookSq] != rook {
				return s, fmt.Errorf("invalid FEN: castling right %c without king/rook on home squares", parts[2][i])
			}
			s.Castling |= right
		}
	}

	// 4. en passant
	if parts[3] != "-" {
		ep := PosFromString(parts[3])
		if !ep.Valid() {
			return s, fmt.Errorf("invalid FEN: en passant %q", parts[3])
		}
		wantY, pawnDir := 2, Pos(Width) // white to move: ep on rank 6, the black pawn stands below it
		if !s.WhiteMove {
			wantY, pawnDir = 5, -Width
		}
		pawnSq := ep + pawnDir
		if ep.Y() != wantY || s.Squares[ep] != None || s.Squares[pawnSq] != Opponent(s.SideToMove())|Pawn {
			return s, fmt.Errorf("invalid FEN: en passant %q does not match the pawns", parts[3])
		}
		if s.pawnCanCaptureEnPassant(pawnSq) {
			s.EnPassant = ep
		}
	}

	// 5. halfmove clock
	if len(parts) > 4 {
		n, err := strconv.Atoi(parts[4])
		if err != nil || n < 0 || n > 9999 {
			return s, fmt.Errorf("invalid FEN: halfmove clock %q", parts[4])
		}
		s.HalfmoveClock = uint16(n)
	}

	// 6. move number
	if len(parts) > 5 {
		n, err := strconv.Atoi(parts[5])
		if err != nil || n < 1 || n > 9999 {
			return s, fmt.Errorf("invalid FEN: move number %q", parts[5])
		}
		s.MoveNumber = uint16(n)
	}

	return s, nil
}

// pawnCanCaptureEnPassant reports whether an enemy pawn stands directly left or
// right of the pawn on pawnSq.
func (s *Setup) pawnCanCaptureEnPassant(pawnSq Pos) bool {
	enemyPawn := Opponent(s.Squares[pawnSq].Color()) | Pawn
	return (pawnSq.X() > 0 && s.Squares[pawnSq-1] == enemyPawn) ||
		(pawnSq.X() < Width-1 && s.Squares[pawnSq+1] == enemyPawn)
}

// FEN returns the position as FEN string.
func (s *Setup) FEN() string {
	var sb strings.Builder
	for y := 0; y < Height; y++ {
		if y > 0 {
			sb.WriteByte('/')
		}
		empty := 0
		for x := 0; x < Width; x++ {
			p := s.Squares[PosFromXY(x, y)]
			if p == None {
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

	if s.WhiteMove {
		sb.WriteString(" w ")
	} else {
		sb.WriteString(" b ")
	}

	if s.Castling == 0 {
		sb.WriteByte('-')
	} else {
		for i, c := range []byte("KQkq") {
			if s.Castling&(1<<i) != 0 {
				sb.WriteByte(c)
			}
		}
	}

	sb.WriteByte(' ')
	sb.WriteString(s.EnPassant.String())
	sb.WriteString(" " + strconv.Itoa(int(s.HalfmoveClock)) + " " + strconv.Itoa(int(s.MoveNumber)))
	return sb.String()
}

// String renders an ASCII diagram followed by the FEN.
func (s *Setup) String() string {
	var sb strings.Builder
	for y := 0; y < Height; y++ {
		sb.WriteString("    ")
		for x := 0; x < Width; x++ {
			sb.WriteByte(s.Squares[PosFromXY(x, y)].Char())
		}
		sb.WriteByte('\n')
	}
	sb.WriteString("\nFEN: ")
	sb.WriteString(s.FEN())
	return sb.String()
}
