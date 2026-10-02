// Package chess holds the small, generator-independent vocabulary shared by all
// board representations: pieces, squares and perft reference data.
package chess

// Piece encodes color and type in one byte. The encoding is identical to the
// author's earlier yacboard and old C# code, so positions and hashes stay comparable.
type Piece uint8

const (
	None Piece = 0x00

	King     Piece = 0x01
	Queen    Piece = 0x02
	Rook     Piece = 0x04
	Bishop   Piece = 0x08
	Knight   Piece = 0x10
	Pawn     Piece = 0x20
	TypeMask Piece = 0x3f

	White     Piece = 0x40
	Black     Piece = 0x80
	ColorMask Piece = White | Black

	// Blocked marks an invalid piece (e.g. unknown FEN character).
	Blocked Piece = ColorMask

	WhiteKing   = White | King
	WhiteQueen  = White | Queen
	WhiteRook   = White | Rook
	WhiteBishop = White | Bishop
	WhiteKnight = White | Knight
	WhitePawn   = White | Pawn
	BlackKing   = Black | King
	BlackQueen  = Black | Queen
	BlackRook   = Black | Rook
	BlackBishop = Black | Bishop
	BlackKnight = Black | Knight
	BlackPawn   = Black | Pawn
)

// Color returns White, Black or None.
func (p Piece) Color() Piece { return p & ColorMask }

// Type returns the piece type without color.
func (p Piece) Type() Piece { return p & TypeMask }

// Is reports whether any of the given type bits is set.
func (p Piece) Is(t Piece) bool { return p&t != 0 }

// Opponent returns the opposite color of a color value.
func Opponent(color Piece) Piece { return color ^ ColorMask }

// PieceFromChar converts a FEN character to a piece. Unknown characters return Blocked.
func PieceFromChar(c byte) Piece {
	switch c {
	case 'K':
		return WhiteKing
	case 'Q':
		return WhiteQueen
	case 'R':
		return WhiteRook
	case 'B':
		return WhiteBishop
	case 'N':
		return WhiteKnight
	case 'P':
		return WhitePawn
	case 'k':
		return BlackKing
	case 'q':
		return BlackQueen
	case 'r':
		return BlackRook
	case 'b':
		return BlackBishop
	case 'n':
		return BlackKnight
	case 'p':
		return BlackPawn
	}
	return Blocked
}

// Char returns the FEN character of the piece, '.' for an empty square and '?' for invalid values.
func (p Piece) Char() byte {
	var c byte
	switch p.Type() {
	case King:
		c = 'k'
	case Queen:
		c = 'q'
	case Rook:
		c = 'r'
	case Bishop:
		c = 'b'
	case Knight:
		c = 'n'
	case Pawn:
		c = 'p'
	case None:
		return '.'
	default:
		return '?'
	}
	if p&White != 0 {
		c -= 'a' - 'A'
	}
	return c
}

func (p Piece) String() string { return string(p.Char()) }
