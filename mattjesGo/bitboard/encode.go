package bitboard

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// Compact position encoding for storing many positions (position lists, later
// hash/persistence experiments): the 8-byte occupancy followed by one nibble
// (color*6 + kind, 0..11) per occupied square in bit order, then 6 flag bytes
// (side+castling, en passant, halfmove clock, move number).
//
// Packed is variable-length (19 bytes for 6 pieces, 30 for the start position,
// which is the maximum because at most 32 pieces = 16 nibble bytes exist).
// PackedFixed pads the same data to a constant 32-byte record for index-based
// access. Milestone 1 measured variable as ~10 % faster for sequential streams.

const flagBytes = 6
const maxPieces = 32

// MaxPackedBytes is the largest Packed record (start position).
const MaxPackedBytes = 8 + maxPieces/2 + flagBytes

// PackedFixedBytes is the constant PackedFixed record size: two per 64-byte cache line.
const PackedFixedBytes = 32

const fixedFlagsOffset = 8 + maxPieces/2

var nibblePiece = [12]chess.Piece{
	chess.WhiteKing, chess.WhiteQueen, chess.WhiteRook, chess.WhiteBishop, chess.WhiteKnight, chess.WhitePawn,
	chess.BlackKing, chess.BlackQueen, chess.BlackRook, chess.BlackBishop, chess.BlackKnight, chess.BlackPawn,
}

// AppendPacked appends the variable-length encoding of b to dst.
//
// Value receiver on purpose: the Append functions are called through Codec
// function values (indirect calls), and a *Board passed to an indirect call
// escapes to the heap in Go's escape analysis. Passing 184 bytes by value is
// cheaper than a heap allocation per position.
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

// decodePackedPieces reads occupancy and nibbles; returns the board (flags not yet set)
// and the number of bytes consumed.
func decodePackedPieces(src []byte) (Board, int) {
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
	return b, i
}

// DecodePacked reads one position from src and returns it with the number of bytes consumed.
func DecodePacked(src []byte) (Board, int) {
	b, i := decodePackedPieces(src)
	return b, i + b.readFlags(src[i:])
}

// PackedFixedRecord returns the constant 32-byte record of b.
func (b Board) PackedFixedRecord() [PackedFixedBytes]byte {
	var rec [PackedFixedBytes]byte
	occ := b.occupied()
	rec[0], rec[1], rec[2], rec[3] = byte(occ), byte(occ>>8), byte(occ>>16), byte(occ>>24)
	rec[4], rec[5], rec[6], rec[7] = byte(occ>>32), byte(occ>>40), byte(occ>>48), byte(occ>>56)
	i := 8
	for bb := occ; bb != 0; i++ {
		p := b.Squares[popLSB(&bb)]
		code := byte(colorIdx(p)*6 + kindIdx(p))
		if bb != 0 {
			q := b.Squares[popLSB(&bb)]
			code |= byte(colorIdx(q)*6+kindIdx(q)) << 4
		}
		rec[i] = code
	}
	f := rec[fixedFlagsOffset:]
	f[0] = byte(b.Castling) << 1
	if b.WhiteMove {
		f[0] |= 1
	}
	f[1], f[2], f[3], f[4], f[5] = byte(b.EnPassant), byte(b.HalfmoveClock), byte(b.HalfmoveClock>>8), byte(b.MoveNumber), byte(b.MoveNumber>>8)
	return rec
}

// AppendPackedFixed appends one PackedFixedBytes record.
func (b Board) AppendPackedFixed(dst []byte) []byte {
	rec := b.PackedFixedRecord()
	return append(dst, rec[:]...)
}

// DecodePackedFixed reads one PackedFixedBytes record.
func DecodePackedFixed(src []byte) (Board, int) {
	b, _ := decodePackedPieces(src)
	b.readFlags(src[fixedFlagsOffset:])
	return b, PackedFixedBytes
}

func (b Board) appendFlags(dst []byte) []byte {
	flags := byte(b.Castling) << 1
	if b.WhiteMove {
		flags |= 1
	}
	return append(dst, flags, byte(b.EnPassant), byte(b.HalfmoveClock), byte(b.HalfmoveClock>>8), byte(b.MoveNumber), byte(b.MoveNumber>>8))
}

// readFlags completes a decoded board: flags and the state part of the Zobrist key.
func (b *Board) readFlags(src []byte) int {
	b.WhiteMove = src[0]&1 != 0
	b.Castling = chess.Castling(src[0]>>1) & chess.AllCastling
	b.EnPassant = chess.Pos(int8(src[1]))
	b.HalfmoveClock = uint16(src[2]) | uint16(src[3])<<8
	b.MoveNumber = uint16(src[4]) | uint16(src[5])<<8
	b.finishKey()
	return flagBytes
}
