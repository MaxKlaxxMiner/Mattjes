package chess

const (
	Width      = 8
	Height     = 8
	FieldCount = Width * Height
)

// Pos is a square index 0..63 with a8 = 0, h8 = 7, a1 = 56, h1 = 63.
// Rank 8 is at the top (y = 0), so white pawns move towards smaller indices.
type Pos int8

// NoPos marks "no square" (e.g. no en passant square).
const NoPos Pos = -1

func PosFromXY(x, y int) Pos {
	if uint(x) >= Width || uint(y) >= Height {
		return NoPos
	}
	return Pos(x + y*Width)
}

// PosFromString parses algebraic notation like "e4". Invalid input returns NoPos.
func PosFromString(s string) Pos {
	if len(s) != 2 {
		return NoPos
	}
	x := int(s[0]) - 'a'
	y := Height - (int(s[1]) - '0')
	return PosFromXY(x, y)
}

func (p Pos) Valid() bool { return uint(p) < FieldCount }
func (p Pos) X() int      { return int(p) % Width }
func (p Pos) Y() int      { return int(p) / Width }

// Rank returns the chess rank 1..8.
func (p Pos) Rank() int { return Height - p.Y() }

func (p Pos) String() string {
	if !p.Valid() {
		return "-"
	}
	return string([]byte{byte('a' + p.X()), byte('0' + p.Rank())})
}
