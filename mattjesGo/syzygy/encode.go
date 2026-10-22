package syzygy

// The position index of a Syzygy table: a port of the encoding in Fathom
// (tbprobe.c, MIT, Ronald de Man / basil00 / Jon Dart). Squares are Syzygy
// squares (a1 = 0, b1 = 1, ..., h8 = 63), pieces are listed in the order
// the table header prescribes (same kinds consecutive), and the index is a
// mixed-radix number: the leading group (the two kings, or the three
// leading pieces, or the pawns of one colour) by its own table, every
// further group of like pieces by a binomial placement count over the
// squares the earlier groups left free.

import "github.com/MaxKlaxxMiner/Mattjes/mattjesGo/pairs"

const (
	encPiece = iota
	encFile
	encRank
)

const maxPieces = 7

var offDiag = [64]int8{
	0, -1, -1, -1, -1, -1, -1, -1,
	1, 0, -1, -1, -1, -1, -1, -1,
	1, 1, 0, -1, -1, -1, -1, -1,
	1, 1, 1, 0, -1, -1, -1, -1,
	1, 1, 1, 1, 0, -1, -1, -1,
	1, 1, 1, 1, 1, 0, -1, -1,
	1, 1, 1, 1, 1, 1, 0, -1,
	1, 1, 1, 1, 1, 1, 1, 0,
}

var triangle = [64]uint8{
	6, 0, 1, 2, 2, 1, 0, 6,
	0, 7, 3, 4, 4, 3, 7, 0,
	1, 3, 8, 5, 5, 8, 3, 1,
	2, 4, 5, 9, 9, 5, 4, 2,
	2, 4, 5, 9, 9, 5, 4, 2,
	1, 3, 8, 5, 5, 8, 3, 1,
	0, 7, 3, 4, 4, 3, 7, 0,
	6, 0, 1, 2, 2, 1, 0, 6,
}

var flipDiag = [64]uint8{
	0, 8, 16, 24, 32, 40, 48, 56,
	1, 9, 17, 25, 33, 41, 49, 57,
	2, 10, 18, 26, 34, 42, 50, 58,
	3, 11, 19, 27, 35, 43, 51, 59,
	4, 12, 20, 28, 36, 44, 52, 60,
	5, 13, 21, 29, 37, 45, 53, 61,
	6, 14, 22, 30, 38, 46, 54, 62,
	7, 15, 23, 31, 39, 47, 55, 63,
}

var lower = [64]uint8{
	28, 0, 1, 2, 3, 4, 5, 6,
	0, 29, 7, 8, 9, 10, 11, 12,
	1, 7, 30, 13, 14, 15, 16, 17,
	2, 8, 13, 31, 18, 19, 20, 21,
	3, 9, 14, 18, 32, 22, 23, 24,
	4, 10, 15, 19, 22, 33, 25, 26,
	5, 11, 16, 20, 23, 25, 34, 27,
	6, 12, 17, 21, 24, 26, 27, 35,
}

var diag = [64]uint8{
	0, 0, 0, 0, 0, 0, 0, 8,
	0, 1, 0, 0, 0, 0, 9, 0,
	0, 0, 2, 0, 0, 10, 0, 0,
	0, 0, 0, 3, 11, 0, 0, 0,
	0, 0, 0, 12, 4, 0, 0, 0,
	0, 0, 13, 0, 0, 5, 0, 0,
	0, 14, 0, 0, 0, 0, 6, 0,
	15, 0, 0, 0, 0, 0, 0, 7,
}

var flap = [2][64]uint8{
	{
		0, 0, 0, 0, 0, 0, 0, 0,
		0, 6, 12, 18, 18, 12, 6, 0,
		1, 7, 13, 19, 19, 13, 7, 1,
		2, 8, 14, 20, 20, 14, 8, 2,
		3, 9, 15, 21, 21, 15, 9, 3,
		4, 10, 16, 22, 22, 16, 10, 4,
		5, 11, 17, 23, 23, 17, 11, 5,
		0, 0, 0, 0, 0, 0, 0, 0,
	},
	{
		0, 0, 0, 0, 0, 0, 0, 0,
		0, 1, 2, 3, 3, 2, 1, 0,
		4, 5, 6, 7, 7, 6, 5, 4,
		8, 9, 10, 11, 11, 10, 9, 8,
		12, 13, 14, 15, 15, 14, 13, 12,
		16, 17, 18, 19, 19, 18, 17, 16,
		20, 21, 22, 23, 23, 22, 21, 20,
		0, 0, 0, 0, 0, 0, 0, 0,
	},
}

var pawnTwist = [2][64]uint8{
	{
		0, 0, 0, 0, 0, 0, 0, 0,
		47, 35, 23, 11, 10, 22, 34, 46,
		45, 33, 21, 9, 8, 20, 32, 44,
		43, 31, 19, 7, 6, 18, 30, 42,
		41, 29, 17, 5, 4, 16, 28, 40,
		39, 27, 15, 3, 2, 14, 26, 38,
		37, 25, 13, 1, 0, 12, 24, 36,
		0, 0, 0, 0, 0, 0, 0, 0,
	},
	{
		0, 0, 0, 0, 0, 0, 0, 0,
		47, 45, 43, 41, 40, 42, 44, 46,
		39, 37, 35, 33, 32, 34, 36, 38,
		31, 29, 27, 25, 24, 26, 28, 30,
		23, 21, 19, 17, 16, 18, 20, 22,
		15, 13, 11, 9, 8, 10, 12, 14,
		7, 5, 3, 1, 0, 2, 4, 6,
		0, 0, 0, 0, 0, 0, 0, 0,
	},
}

var kkIdx = [10][64]int16{
	{-1, -1, -1, 0, 1, 2, 3, 4,
		-1, -1, -1, 5, 6, 7, 8, 9,
		10, 11, 12, 13, 14, 15, 16, 17,
		18, 19, 20, 21, 22, 23, 24, 25,
		26, 27, 28, 29, 30, 31, 32, 33,
		34, 35, 36, 37, 38, 39, 40, 41,
		42, 43, 44, 45, 46, 47, 48, 49,
		50, 51, 52, 53, 54, 55, 56, 57},
	{58, -1, -1, -1, 59, 60, 61, 62,
		63, -1, -1, -1, 64, 65, 66, 67,
		68, 69, 70, 71, 72, 73, 74, 75,
		76, 77, 78, 79, 80, 81, 82, 83,
		84, 85, 86, 87, 88, 89, 90, 91,
		92, 93, 94, 95, 96, 97, 98, 99,
		100, 101, 102, 103, 104, 105, 106, 107,
		108, 109, 110, 111, 112, 113, 114, 115},
	{116, 117, -1, -1, -1, 118, 119, 120,
		121, 122, -1, -1, -1, 123, 124, 125,
		126, 127, 128, 129, 130, 131, 132, 133,
		134, 135, 136, 137, 138, 139, 140, 141,
		142, 143, 144, 145, 146, 147, 148, 149,
		150, 151, 152, 153, 154, 155, 156, 157,
		158, 159, 160, 161, 162, 163, 164, 165,
		166, 167, 168, 169, 170, 171, 172, 173},
	{174, -1, -1, -1, 175, 176, 177, 178,
		179, -1, -1, -1, 180, 181, 182, 183,
		184, -1, -1, -1, 185, 186, 187, 188,
		189, 190, 191, 192, 193, 194, 195, 196,
		197, 198, 199, 200, 201, 202, 203, 204,
		205, 206, 207, 208, 209, 210, 211, 212,
		213, 214, 215, 216, 217, 218, 219, 220,
		221, 222, 223, 224, 225, 226, 227, 228},
	{229, 230, -1, -1, -1, 231, 232, 233,
		234, 235, -1, -1, -1, 236, 237, 238,
		239, 240, -1, -1, -1, 241, 242, 243,
		244, 245, 246, 247, 248, 249, 250, 251,
		252, 253, 254, 255, 256, 257, 258, 259,
		260, 261, 262, 263, 264, 265, 266, 267,
		268, 269, 270, 271, 272, 273, 274, 275,
		276, 277, 278, 279, 280, 281, 282, 283},
	{284, 285, 286, 287, 288, 289, 290, 291,
		292, 293, -1, -1, -1, 294, 295, 296,
		297, 298, -1, -1, -1, 299, 300, 301,
		302, 303, -1, -1, -1, 304, 305, 306,
		307, 308, 309, 310, 311, 312, 313, 314,
		315, 316, 317, 318, 319, 320, 321, 322,
		323, 324, 325, 326, 327, 328, 329, 330,
		331, 332, 333, 334, 335, 336, 337, 338},
	{-1, -1, 339, 340, 341, 342, 343, 344,
		-1, -1, 345, 346, 347, 348, 349, 350,
		-1, -1, 441, 351, 352, 353, 354, 355,
		-1, -1, -1, 442, 356, 357, 358, 359,
		-1, -1, -1, -1, 443, 360, 361, 362,
		-1, -1, -1, -1, -1, 444, 363, 364,
		-1, -1, -1, -1, -1, -1, 445, 365,
		-1, -1, -1, -1, -1, -1, -1, 446},
	{-1, -1, -1, 366, 367, 368, 369, 370,
		-1, -1, -1, 371, 372, 373, 374, 375,
		-1, -1, -1, 376, 377, 378, 379, 380,
		-1, -1, -1, 447, 381, 382, 383, 384,
		-1, -1, -1, -1, 448, 385, 386, 387,
		-1, -1, -1, -1, -1, 449, 388, 389,
		-1, -1, -1, -1, -1, -1, 450, 390,
		-1, -1, -1, -1, -1, -1, -1, 451},
	{452, 391, 392, 393, 394, 395, 396, 397,
		-1, -1, -1, -1, 398, 399, 400, 401,
		-1, -1, -1, -1, 402, 403, 404, 405,
		-1, -1, -1, -1, 406, 407, 408, 409,
		-1, -1, -1, -1, 453, 410, 411, 412,
		-1, -1, -1, -1, -1, 454, 413, 414,
		-1, -1, -1, -1, -1, -1, 455, 415,
		-1, -1, -1, -1, -1, -1, -1, 456},
	{457, 416, 417, 418, 419, 420, 421, 422,
		-1, 458, 423, 424, 425, 426, 427, 428,
		-1, -1, -1, -1, -1, 429, 430, 431,
		-1, -1, -1, -1, -1, 432, 433, 434,
		-1, -1, -1, -1, -1, 435, 436, 437,
		-1, -1, -1, -1, -1, 459, 438, 439,
		-1, -1, -1, -1, -1, -1, 460, 440,
		-1, -1, -1, -1, -1, -1, -1, 461},
}

var fileToFile = [8]uint8{0, 1, 2, 3, 3, 2, 1, 0}

var (
	binomial       [7][64]uint64
	pawnIdx        [2][6][24]uint64
	pawnFactorFile [6][4]uint64
	pawnFactorRank [6][6]uint64
)

func init() {
	// binomial[k][n] = C(n, k)
	for i := 0; i < 7; i++ {
		for j := 0; j < 64; j++ {
			f, l := uint64(1), uint64(1)
			for k := 0; k < i; k++ {
				f *= uint64(j - k)
				l *= uint64(k + 1)
			}
			binomial[i][j] = f / l
		}
	}
	for i := 0; i < 6; i++ {
		s := uint64(0)
		for j := 0; j < 24; j++ {
			pawnIdx[0][i][j] = s
			s += binomial[i][pawnTwist[0][(1+(j%6))*8+(j/6)]]
			if (j+1)%6 == 0 {
				pawnFactorFile[i][j/6] = s
				s = 0
			}
		}
	}
	for i := 0; i < 6; i++ {
		s := uint64(0)
		for j := 0; j < 24; j++ {
			pawnIdx[1][i][j] = s
			s += binomial[i][pawnTwist[1][(1+(j/4))*8+(j%4)]]
			if (j+1)%4 == 0 {
				pawnFactorRank[i][j/4] = s
				s = 0
			}
		}
	}
}

// encInfo describes one encoding of a table: the piece order of the header
// (Syzygy piece codes 1..14, like kinds consecutive), the group sizes
// (norm) and the mixed-radix factors.
type encInfo struct {
	precomp *pairsData   // a Syzygy file's stream
	own     *pairs.Table // or our own (write.go)
	factor  [maxPieces]uint64
	pieces  [maxPieces]uint8
	norm    [maxPieces]uint8
}

// leadingPawn moves the leading pawn (the one with the smallest flap
// value) to the front and returns the file (file encoding) or rank
// (rank encoding) class of the table to use.
func leadingPawn(p []int, be *baseEntry, enc int) int {
	for i := 1; i < int(be.pawns[0]); i++ {
		if flap[enc-1][p[0]] > flap[enc-1][p[i]] {
			p[0], p[i] = p[i], p[0]
		}
	}
	if enc == encFile {
		return int(fileToFile[p[0]&7])
	}
	return (p[0] - 8) >> 3
}

// encode computes the index of the squares p (Syzygy squares, in the
// table's piece order) for the encoding enc.
func encode(p []int, ei *encInfo, be *baseEntry, enc int) uint64 {
	n := int(be.num)
	var idx uint64
	var k int

	if p[0]&0x04 != 0 {
		for i := 0; i < n; i++ {
			p[i] ^= 0x07
		}
	}
	if enc == encPiece {
		if p[0]&0x20 != 0 {
			for i := 0; i < n; i++ {
				p[i] ^= 0x38
			}
		}
		for i := 0; i < n; i++ {
			if offDiag[p[i]] != 0 {
				lim := 3
				if be.kkEnc {
					lim = 2
				}
				if offDiag[p[i]] > 0 && i < lim {
					for j := 0; j < n; j++ {
						p[j] = int(flipDiag[p[j]])
					}
				}
				break
			}
		}
		if be.kkEnc {
			idx = uint64(kkIdx[triangle[p[0]]][p[1]])
			k = 2
		} else {
			s1 := 0
			if p[1] > p[0] {
				s1 = 1
			}
			s2 := 0
			if p[2] > p[0] {
				s2++
			}
			if p[2] > p[1] {
				s2++
			}
			switch {
			case offDiag[p[0]] != 0:
				idx = uint64(triangle[p[0]])*63*62 + uint64(p[1]-s1)*62 + uint64(p[2]-s2)
			case offDiag[p[1]] != 0:
				idx = 6*63*62 + uint64(diag[p[0]])*28*62 + uint64(lower[p[1]])*62 + uint64(p[2]-s2)
			case offDiag[p[2]] != 0:
				idx = 6*63*62 + 4*28*62 + uint64(diag[p[0]])*7*28 + uint64(int(diag[p[1]])-s1)*28 + uint64(lower[p[2]])
			default:
				idx = 6*63*62 + 4*28*62 + 4*7*28 + uint64(diag[p[0]])*7*6 + uint64(int(diag[p[1]])-s1)*6 + uint64(int(diag[p[2]])-s2)
			}
			k = 3
		}
		idx *= ei.factor[0]
	} else {
		for i := 1; i < int(be.pawns[0]); i++ {
			for j := i + 1; j < int(be.pawns[0]); j++ {
				if pawnTwist[enc-1][p[i]] < pawnTwist[enc-1][p[j]] {
					p[i], p[j] = p[j], p[i]
				}
			}
		}
		k = int(be.pawns[0])
		idx = pawnIdx[enc-1][k-1][flap[enc-1][p[0]]]
		for i := 1; i < k; i++ {
			idx += binomial[k-i][pawnTwist[enc-1][p[i]]]
		}
		idx *= ei.factor[0]
		// pawns of the other colour
		if be.pawns[1] != 0 {
			t := k + int(be.pawns[1])
			for i := k; i < t; i++ {
				for j := i + 1; j < t; j++ {
					if p[i] > p[j] {
						p[i], p[j] = p[j], p[i]
					}
				}
			}
			s := uint64(0)
			for i := k; i < t; i++ {
				sq := p[i]
				skips := 0
				for j := 0; j < k; j++ {
					if sq > p[j] {
						skips++
					}
				}
				s += binomial[i-k+1][sq-skips-8]
			}
			idx += s * ei.factor[k]
			k = t
		}
	}

	for k < n {
		t := k + int(ei.norm[k])
		for i := k; i < t; i++ {
			for j := i + 1; j < t; j++ {
				if p[i] > p[j] {
					p[i], p[j] = p[j], p[i]
				}
			}
		}
		s := uint64(0)
		for i := k; i < t; i++ {
			sq := p[i]
			skips := 0
			for j := 0; j < k; j++ {
				if sq > p[j] {
					skips++
				}
			}
			s += binomial[i-k+1][sq-skips]
		}
		idx += s * ei.factor[k]
		k = t
	}
	return idx
}

// subfactor counts the placements of k like pieces on n squares.
func subfactor(k, n uint64) uint64 {
	f, l := n, uint64(1)
	for i := uint64(1); i < k; i++ {
		f *= n - i
		l *= i + 1
	}
	return f / l
}

// initEncInfo reads the piece order and the group order of one encoding
// from the header bytes tb (nibble shift 0 or 4 for the two sides of a
// split table) and computes the factors; returns the table size.
func initEncInfo(ei *encInfo, be *baseEntry, tb []byte, shift uint, t int, enc int) uint64 {
	morePawns := enc != encPiece && be.pawns[1] > 0
	mp := 0
	if morePawns {
		mp = 1
	}
	for i := 0; i < int(be.num); i++ {
		ei.pieces[i] = (tb[i+1+mp] >> shift) & 0x0f
	}
	order := int((tb[0] >> shift) & 0x0f)
	order2 := 0x0f
	if morePawns {
		order2 = int((tb[1] >> shift) & 0x0f)
	}
	return ei.setup(be, order, order2, t, enc)
}

// setup computes the groups (norm) and the factors of an encoding from its
// piece order (ei.pieces), the chain position of the leading group (order)
// and of the other colour's pawns (order2, 0x0f when there are none); t is
// the file or rank class of a pawn table. Returns the table size.
func (ei *encInfo) setup(be *baseEntry, order, order2 int, t int, enc int) uint64 {
	for i := 0; i < int(be.num); i++ {
		ei.norm[i] = 0
		ei.factor[i] = 0
	}
	morePawns := enc != encPiece && be.pawns[1] > 0
	var k int
	switch {
	case enc != encPiece:
		k = int(be.pawns[0])
	case be.kkEnc:
		k = 2
	default:
		k = 3
	}
	ei.norm[0] = uint8(k)
	if morePawns {
		ei.norm[k] = be.pawns[1]
		k += int(ei.norm[k])
	}
	for i := k; i < int(be.num); i += int(ei.norm[i]) {
		for j := i; j < int(be.num) && ei.pieces[j] == ei.pieces[i]; j++ {
			ei.norm[i]++
		}
	}
	n := uint64(64 - k)
	f := uint64(1)
	for i := 0; k < int(be.num) || i == order || i == order2; i++ {
		switch i {
		case order:
			ei.factor[0] = f
			switch {
			case enc == encFile:
				f *= pawnFactorFile[ei.norm[0]-1][t]
			case enc == encRank:
				f *= pawnFactorRank[ei.norm[0]-1][t]
			case be.kkEnc:
				f *= 462
			default:
				f *= 31332
			}
		case order2:
			ei.factor[ei.norm[0]] = f
			f *= subfactor(uint64(ei.norm[ei.norm[0]]), uint64(48-ei.norm[0]))
		default:
			ei.factor[k] = f
			f *= subfactor(uint64(ei.norm[k]), n)
			n -= uint64(ei.norm[k])
			k += int(ei.norm[k])
		}
	}
	return f
}
