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
// At most 32 pieces = 16 nibble bytes, so the start position (30 bytes) is the
// maximum; 6 pieces take 17 bytes.
const MaxPackedBytes = 8 + maxPieces/2 + flagBytes

const maxPieces = 32

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

// PackedFixedBytes is the packed encoding in a constant 32-byte record: 8 bytes
// occupancy, 16 nibble bytes (zero padded), 6 flag bytes, 2 bytes padding.
// Two records per 64-byte cache line; records can be addressed by index.
const PackedFixedBytes = 32

const fixedFlagsOffset = 8 + maxPieces/2

// AppendPackedFixed writes one PackedFixedBytes record directly.
func (b Board) AppendPackedFixed(dst []byte) []byte {
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
	return append(dst, rec[:]...)
}

// DecodePackedFixed reads one PackedFixedBytes record.
func DecodePackedFixed(src []byte) (Board, int) {
	b, _ := DecodePacked(src)
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

func (b *Board) readFlags(src []byte) int {
	b.WhiteMove = src[0]&1 != 0
	b.Castling = chess.Castling(src[0]>>1) & chess.AllCastling
	b.EnPassant = chess.Pos(int8(src[1]))
	b.HalfmoveClock = uint16(src[2]) | uint16(src[3])<<8
	b.MoveNumber = uint16(src[4]) | uint16(src[5])<<8
	return flagBytes
}
