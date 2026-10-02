// Package mailbox is a minimalistic 8x8 mailbox board with a legal move
// generator, modelled after the author's yacboard. It is the first, simplest
// generator in Mattjes and serves as the correctness reference for others.
package mailbox

import (
	"strings"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// Castling rights as a bit set.
type Castling uint8

const (
	WhiteKingside Castling = 1 << iota
	WhiteQueenside
	BlackKingside
	BlackQueenside
	AllCastling = WhiteKingside | WhiteQueenside | BlackKingside | BlackQueenside
)

// Board is a complete position. It is a plain value and can be copied freely.
type Board struct {
	Fields        [chess.FieldCount]chess.Piece
	WhiteKing     chess.Pos
	BlackKing     chess.Pos
	EnPassant     chess.Pos // target square of a possible en passant capture, or NoPos
	Castling      Castling
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
	b.Castling = Castling(s>>8) & AllCastling
	b.HalfmoveClock = uint16(s >> 16)
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

// String renders the board as ASCII diagram followed by the FEN.
func (b *Board) String() string {
	var sb strings.Builder
	for y := 0; y < chess.Height; y++ {
		sb.WriteString("    ")
		for x := 0; x < chess.Width; x++ {
			sb.WriteByte(b.Fields[chess.PosFromXY(x, y)].Char())
		}
		sb.WriteByte('\n')
	}
	sb.WriteString("\nFEN: ")
	sb.WriteString(b.FEN())
	return sb.String()
}
