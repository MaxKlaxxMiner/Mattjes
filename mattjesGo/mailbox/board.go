// Package mailbox is a minimalistic 8x8 mailbox board with a legal move
// generator, modelled after the author's yacboard. It is the first, simplest
// generator in Mattjes and serves as the correctness reference for others.
package mailbox

import (
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// Board is a complete position. It is a plain value and can be copied freely.
type Board struct {
	Fields        [chess.FieldCount]chess.Piece
	WhiteKing     chess.Pos
	BlackKing     chess.Pos
	EnPassant     chess.Pos // target square of a possible en passant capture, or NoPos
	Castling      chess.Castling
	WhiteMove     bool
	HalfmoveClock uint16
	MoveNumber    uint16
}

// State is the irreversible part of a position that UndoMove cannot reconstruct
// from the move alone: en passant square, castling rights and halfmove clock.
type State uint32

func (b *Board) State() State {
	return State(uint8(b.EnPassant)) | State(b.Castling)<<8 | State(b.HalfmoveClock)<<16
}

func (b *Board) restoreState(s State) {
	b.EnPassant = chess.Pos(int8(uint8(s)))
	b.Castling = chess.Castling(s>>8) & chess.AllCastling
	b.HalfmoveClock = uint16(s >> 16)
}

// New returns the start position.
func New() Board {
	b, _ := FromFEN(chess.StartFEN)
	return b
}

// FromFEN parses a FEN string via chess.ParseFEN.
func FromFEN(fen string) (Board, error) {
	s, err := chess.ParseFEN(fen)
	if err != nil {
		return Board{}, err
	}
	return FromSetup(&s), nil
}

// FromSetup builds a board from a validated setup.
func FromSetup(s *chess.Setup) Board {
	var b Board
	b.Clear()
	for pos := chess.Pos(0); pos < chess.FieldCount; pos++ {
		if p := s.Squares[pos]; p != chess.None {
			b.SetField(pos, p)
		}
	}
	b.WhiteMove = s.WhiteMove
	b.Castling = s.Castling
	b.EnPassant = s.EnPassant
	b.HalfmoveClock = s.HalfmoveClock
	b.MoveNumber = s.MoveNumber
	return b
}

// Setup converts the board back to the representation-independent form.
func (b *Board) Setup() chess.Setup {
	return chess.Setup{
		Squares:       b.Fields,
		WhiteMove:     b.WhiteMove,
		Castling:      b.Castling,
		EnPassant:     b.EnPassant,
		HalfmoveClock: b.HalfmoveClock,
		MoveNumber:    b.MoveNumber,
	}
}

// FEN returns the position as FEN string.
func (b *Board) FEN() string {
	s := b.Setup()
	return s.FEN()
}

// String renders the board as ASCII diagram followed by the FEN.
func (b *Board) String() string {
	s := b.Setup()
	return s.String()
}

// Clear empties the board. White to move, no castling, no en passant.
func (b *Board) Clear() {
	*b = Board{WhiteKing: chess.NoPos, BlackKing: chess.NoPos, EnPassant: chess.NoPos, WhiteMove: true, MoveNumber: 1}
}

// SetField places a piece and keeps the king positions in sync.
func (b *Board) SetField(pos chess.Pos, p chess.Piece) {
	b.Fields[pos] = p
	switch p {
	case chess.WhiteKing:
		b.WhiteKing = pos
	case chess.BlackKing:
		b.BlackKing = pos
	}
}

// SideToMove returns the color to move.
func (b *Board) SideToMove() chess.Piece {
	if b.WhiteMove {
		return chess.White
	}
	return chess.Black
}

// KingPos returns the king square of the given color.
func (b *Board) KingPos(color chess.Piece) chess.Pos {
	if color == chess.White {
		return b.WhiteKing
	}
	return b.BlackKing
}
