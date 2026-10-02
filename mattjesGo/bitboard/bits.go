// Package bitboard is the second move generator: 64-bit sets per piece type and
// color, magic bitboards for sliders and fully legal generation via pin and
// check masks (no make/check/unmake per move).
//
// Bit layout: bit i is square chess.Pos(i), so a8 = bit 0 and h1 = bit 63 (the
// numbering of chess.Pos). Consequently "north" (towards rank 8) is a right
// shift, unlike in most engines where a1 is bit 0.
package bitboard

import (
	"math/bits"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

const (
	FileA uint64 = 0x0101010101010101
	FileH uint64 = 0x8080808080808080
	Rank8 uint64 = 0xff       // y = 0
	Rank1 uint64 = 0xff << 56 // y = 7
)

// rankMask returns the bits of row y (y = 0 is rank 8).
func rankMask(y int) uint64 { return 0xff << (8 * uint(y)) }

func bit(sq chess.Pos) uint64 { return 1 << uint(sq) }

// lsb returns the lowest set square. bb must not be 0.
func lsb(bb uint64) chess.Pos { return chess.Pos(bits.TrailingZeros64(bb)) }

// popLSB removes and returns the lowest set square.
func popLSB(bb *uint64) chess.Pos {
	sq := lsb(*bb)
	*bb &= *bb - 1
	return sq
}

func popcount(bb uint64) int { return bits.OnesCount64(bb) }

// Directional shifts. Edge files are masked off before shifting sideways.
func north(bb uint64) uint64     { return bb >> 8 }
func south(bb uint64) uint64     { return bb << 8 }
func west(bb uint64) uint64      { return (bb &^ FileA) >> 1 }
func east(bb uint64) uint64      { return (bb &^ FileH) << 1 }
func northWest(bb uint64) uint64 { return (bb &^ FileA) >> 9 }
func northEast(bb uint64) uint64 { return (bb &^ FileH) >> 7 }
func southWest(bb uint64) uint64 { return (bb &^ FileA) << 7 }
func southEast(bb uint64) uint64 { return (bb &^ FileH) << 9 }

// Piece kind indices into Board.Pieces[color][kind]. They equal the bit
// position of the type flag in chess.Piece, so kindIdx is a single BSF.
const (
	kKing = iota
	kQueen
	kRook
	kBishop
	kKnight
	kPawn
	kindCount
)

var kindPiece = [kindCount]chess.Piece{chess.King, chess.Queen, chess.Rook, chess.Bishop, chess.Knight, chess.Pawn}
var colorPiece = [2]chess.Piece{chess.White, chess.Black}

func kindIdx(p chess.Piece) int { return bits.TrailingZeros8(uint8(p & chess.TypeMask)) }

// colorIdx maps White (0x40) to 0 and Black (0x80) to 1.
func colorIdx(p chess.Piece) int { return int(p >> 7) }
