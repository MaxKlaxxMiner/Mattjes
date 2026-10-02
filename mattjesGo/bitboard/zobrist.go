package bitboard

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"

// Zobrist hashing: every (piece, square) pair, every castling-rights combination,
// every en passant file and the side to move get random 64-bit numbers. The key
// of a position is the XOR of all applicable numbers, which makes it incremental:
// moving a piece XORs out its old square and XORs in the new one.
//
// The key is 128 bit (two independent 64-bit sets). Milestone 2 showed that
// 64 bit is collision-free for millions of positions, but a mate proof must
// never rely on a false hit, so the wider key is the standard. It costs about
// 2 % in Go and nothing measurable in Rust compared with 64 bit.
const KeyWords = 2

// Key is the Zobrist key of a position (pieces, side to move, castling, en passant;
// halfmove clock and move number are not part of the identity).
type Key [KeyWords]uint64

var zPiece [12][chess.FieldCount]Key // [colorIdx*6+kindIdx][sq]
var zCastle [16]Key
var zEnPassant [9]Key // 0 = none, 1..8 = file a..h
var zSide Key         // XORed in when black is to move

func init() {
	r := rng{s: 0x6A09E667F3BCC908}
	fill := func(k *Key) {
		for i := range k {
			k[i] = r.next()
		}
	}
	for p := range zPiece {
		for sq := range zPiece[p] {
			fill(&zPiece[p][sq])
		}
	}
	for c := range zCastle {
		fill(&zCastle[c])
	}
	for f := range zEnPassant {
		fill(&zEnPassant[f])
	}
	fill(&zSide)
}

func epIdx(ep chess.Pos) int {
	if !ep.Valid() {
		return 0
	}
	return ep.X() + 1
}

// xorPiece is called by put and remove.
func (b *Board) xorPiece(p chess.Piece, sq chess.Pos) {
	z := &zPiece[colorIdx(p)*6+kindIdx(p)][sq]
	for i := range b.Key {
		b.Key[i] ^= z[i]
	}
}

// xorState flips castling rights and en passant between the current and the given
// values and toggles the side to move. XOR is symmetric, so DoMove and UndoMove
// both call it with the "other" state.
func (b *Board) xorState(otherCastling chess.Castling, otherEP chess.Pos) {
	zc1, zc2 := &zCastle[b.Castling], &zCastle[otherCastling]
	ze1, ze2 := &zEnPassant[epIdx(b.EnPassant)], &zEnPassant[epIdx(otherEP)]
	for i := range b.Key {
		b.Key[i] ^= zc1[i] ^ zc2[i] ^ ze1[i] ^ ze2[i] ^ zSide[i]
	}
}

// finishKey adds the non-piece components (castling, en passant, side to move) to a
// key that so far only contains the pieces (after FromSetup or a stream decode).
func (b *Board) finishKey() {
	for i := range b.Key {
		b.Key[i] ^= zCastle[b.Castling][i] ^ zEnPassant[epIdx(b.EnPassant)][i]
		if !b.WhiteMove {
			b.Key[i] ^= zSide[i]
		}
	}
}

// ZobristFingerprint identifies the Zobrist constants. Persistent tables store it
// so that a file written with other constants (different seed or layout) is
// rejected instead of producing silent false hits. Go and Rust compute the same
// value, which makes their table files interchangeable.
func ZobristFingerprint() uint64 {
	var f uint64
	mix := func(k *Key) {
		for _, w := range k {
			f = (f ^ w) * 0x9E3779B97F4A7C15
			f ^= f >> 32
		}
	}
	for p := range zPiece {
		for sq := range zPiece[p] {
			mix(&zPiece[p][sq])
		}
	}
	for c := range zCastle {
		mix(&zCastle[c])
	}
	for e := range zEnPassant {
		mix(&zEnPassant[e])
	}
	mix(&zSide)
	return f
}

// ZobristFull recomputes the key from scratch. Used to verify the incremental updates.
func (b *Board) ZobristFull() Key {
	var k Key
	for sq := chess.Pos(0); sq < chess.FieldCount; sq++ {
		if p := b.Squares[sq]; p != chess.None {
			z := &zPiece[colorIdx(p)*6+kindIdx(p)][sq]
			for i := range k {
				k[i] ^= z[i]
			}
		}
	}
	for i := range k {
		k[i] ^= zCastle[b.Castling][i] ^ zEnPassant[epIdx(b.EnPassant)][i]
		if !b.WhiteMove {
			k[i] ^= zSide[i]
		}
	}
	return k
}
