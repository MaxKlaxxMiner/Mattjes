package bitboard

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// Compact position encodings for storing many positions (position lists,
// later hash/persistence experiments). Both are variable-length and end with
// the same 6 flag bytes: side+castling, en passant, halfmove clock, move number.

// FastFen is the run-length encoding from the old C# code (GetFastFen): one byte
// per piece (always >= 0x41) and one count byte (< 64) per gap of empty squares.
// Start position 39 bytes, typical middlegame ~50, late endgame ~20.
const MaxFastFenBytes = chess.FieldCount + flagBytes

// Packed is the usual bitboard encoding: the 8-byte occupancy followed by one
// nibble (color*6 + kind, 0..11) per occupied square in bit order.
// Start position 30 bytes, 6 pieces 17 bytes.
const MaxPackedBytes = 8 + chess.FieldCount/2 + flagBytes

const flagBytes = 6

// AppendFastFen appends the run-length encoding of b to dst.
//
// Value receiver on purpose: the Append functions are called through Codec
// function values (indirect calls), and a *Board passed to an indirect call
// escapes to the heap in Go's escape analysis. Passing 184 bytes by value is
// cheaper than a heap allocation per position.
func (b Board) AppendFastFen(dst []byte) []byte {
	gap := byte(0)
	for _, p := range b.Squares {
		if p == chess.None {
			gap++
			continue
		}
		if gap > 0 {
			dst = append(dst, gap)
			gap = 0
		}
		dst = append(dst, byte(p))
	}
	if gap > 0 {
		dst = append(dst, gap)
	}
	return b.appendFlags(dst)
}

// DecodeFastFen reads one position from src and returns it with the number of bytes consumed.
func DecodeFastFen(src []byte) (Board, int) {
	var b Board
	i := 0
	for sq := chess.Pos(0); sq < chess.FieldCount; {
		c := src[i]
		i++
		if c < 64 {
			sq += chess.Pos(c)
			continue
		}
		b.put(sq, chess.Piece(c))
		sq++
	}
	return b, i + b.readFlags(src[i:])
}

var nibblePiece = [12]chess.Piece{
	chess.WhiteKing, chess.WhiteQueen, chess.WhiteRook, chess.WhiteBishop, chess.WhiteKnight, chess.WhitePawn,
	chess.BlackKing, chess.BlackQueen, chess.BlackRook, chess.BlackBishop, chess.BlackKnight, chess.BlackPawn,
}

// AppendPacked appends the occupancy + nibble encoding of b to dst (value receiver, see AppendFastFen).
func (b Board) AppendPacked(dst []byte) []byte {
	occ := b.occupied()
	dst = append(dst, byte(occ), byte(occ>>8), byte(occ>>16), byte(occ>>24), byte(occ>>32), byte(occ>>40), byte(occ>>48), byte(occ>>56))
	for bb := occ; bb != 0; {
		p := b.Squares[popLSB(&bb)]
		code := byte(colorIdx(p)*6 + kindIdx(p))
		if bb != 0 {
			q := b.Squares[popLSB(&bb)]
			code |= byte(colorIdx(q)*6+kindIdx(q)) << 4
		}
		dst = append(dst, code)
	}
	return b.appendFlags(dst)
}

// DecodePacked reads one position from src and returns it with the number of bytes consumed.
func DecodePacked(src []byte) (Board, int) {
	var b Board
	occ := uint64(src[0]) | uint64(src[1])<<8 | uint64(src[2])<<16 | uint64(src[3])<<24 |
		uint64(src[4])<<32 | uint64(src[5])<<40 | uint64(src[6])<<48 | uint64(src[7])<<56
	i := 8
	for bb := occ; bb != 0; {
		code := src[i]
		i++
		b.put(popLSB(&bb), nibblePiece[code&15])
		if bb != 0 {
			b.put(popLSB(&bb), nibblePiece[code>>4])
		}
	}
	return b, i + b.readFlags(src[i:])
}

func (b Board) appendFlags(dst []byte) []byte {
	flags := byte(b.Castling) << 1
	if b.WhiteMove {
		flags |= 1
	}
	return append(dst, flags, byte(b.EnPassant), byte(b.HalfmoveClock), byte(b.HalfmoveClock>>8), byte(b.MoveNumber), byte(b.MoveNumber>>8))
}

func (b *Board) readFlags(src []byte) int {
	b.WhiteMove = src[0]&1 != 0
	b.Castling = chess.Castling(src[0]>>1) & chess.AllCastling
	b.EnPassant = chess.Pos(int8(src[1]))
	b.HalfmoveClock = uint16(src[2]) | uint16(src[3])<<8
	b.MoveNumber = uint16(src[4]) | uint16(src[5])<<8
	return flagBytes
}
