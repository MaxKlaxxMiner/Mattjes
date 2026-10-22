//! The position index of a Syzygy table: a port of the encoding in Fathom
//! (tbprobe.c, MIT, Ronald de Man / basil00 / Jon Dart), see the Go
//! package `syzygy/encode.go`. Squares are Syzygy squares (a1 = 0, h8 =
//! 63), pieces are listed in the order the table header prescribes.

use std::sync::LazyLock;

pub const ENC_PIECE: usize = 0;
pub const ENC_FILE: usize = 1;
pub const ENC_RANK: usize = 2;

pub const MAX_PIECES: usize = 7;

pub(super) const OFF_DIAG: [i8; 64] = [
    0, -1, -1, -1, -1, -1, -1, -1, 1, 0, -1, -1, -1, -1, -1, -1, 1, 1, 0, -1, -1, -1, -1, -1, 1, 1, 1, 0, -1, -1, -1, -1, 1, 1, 1, 1, 0, -1, -1, -1, 1, 1, 1, 1, 1, 0, -1, -1, 1, 1, 1, 1, 1, 1, 0, -1, 1, 1, 1, 1, 1, 1, 1, 0,
];

const TRIANGLE: [u8; 64] = [
    6, 0, 1, 2, 2, 1, 0, 6, 0, 7, 3, 4, 4, 3, 7, 0, 1, 3, 8, 5, 5, 8, 3, 1, 2, 4, 5, 9, 9, 5, 4, 2, 2, 4, 5, 9, 9, 5, 4, 2, 1, 3, 8, 5, 5, 8, 3, 1, 0, 7, 3, 4, 4, 3, 7, 0, 6, 0, 1, 2, 2, 1, 0, 6,
];

pub(super) const FLIP_DIAG: [u8; 64] = [
    0, 8, 16, 24, 32, 40, 48, 56, 1, 9, 17, 25, 33, 41, 49, 57, 2, 10, 18, 26, 34, 42, 50, 58, 3, 11, 19, 27, 35, 43, 51, 59, 4, 12, 20, 28, 36, 44, 52, 60, 5, 13, 21, 29, 37, 45, 53, 61, 6, 14, 22, 30, 38, 46, 54, 62, 7, 15, 23, 31, 39, 47, 55, 63,
];

const LOWER: [u8; 64] = [
    28, 0, 1, 2, 3, 4, 5, 6, 0, 29, 7, 8, 9, 10, 11, 12, 1, 7, 30, 13, 14, 15, 16, 17, 2, 8, 13, 31, 18, 19, 20, 21, 3, 9, 14, 18, 32, 22, 23, 24, 4, 10, 15, 19, 22, 33, 25, 26, 5, 11, 16, 20, 23, 25, 34, 27, 6, 12, 17, 21, 24, 26, 27, 35,
];

const DIAG: [u8; 64] = [
    0, 0, 0, 0, 0, 0, 0, 8, 0, 1, 0, 0, 0, 0, 9, 0, 0, 0, 2, 0, 0, 10, 0, 0, 0, 0, 0, 3, 11, 0, 0, 0, 0, 0, 0, 12, 4, 0, 0, 0, 0, 0, 13, 0, 0, 5, 0, 0, 0, 14, 0, 0, 0, 0, 6, 0, 15, 0, 0, 0, 0, 0, 0, 7,
];

const FLAP: [[u8; 64]; 2] = [
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 12, 18, 18, 12, 6, 0, 1, 7, 13, 19, 19, 13, 7, 1, 2, 8, 14, 20, 20, 14, 8, 2, 3, 9, 15, 21, 21, 15, 9, 3, 4, 10, 16, 22, 22, 16, 10, 4, 5, 11, 17, 23, 23, 17, 11, 5, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 3, 2, 1, 0, 4, 5, 6, 7, 7, 6, 5, 4, 8, 9, 10, 11, 11, 10, 9, 8, 12, 13, 14, 15, 15, 14, 13, 12, 16, 17, 18, 19, 19, 18, 17, 16, 20, 21, 22, 23, 23, 22, 21, 20, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
];

const PAWN_TWIST: [[u8; 64]; 2] = [
    [
        0, 0, 0, 0, 0, 0, 0, 0, 47, 35, 23, 11, 10, 22, 34, 46, 45, 33, 21, 9, 8, 20, 32, 44, 43, 31, 19, 7, 6, 18, 30, 42, 41, 29, 17, 5, 4, 16, 28, 40, 39, 27, 15, 3, 2, 14, 26, 38, 37, 25, 13, 1, 0, 12, 24, 36, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    [
        0, 0, 0, 0, 0, 0, 0, 0, 47, 45, 43, 41, 40, 42, 44, 46, 39, 37, 35, 33, 32, 34, 36, 38, 31, 29, 27, 25, 24, 26, 28, 30, 23, 21, 19, 17, 16, 18, 20, 22, 15, 13, 11, 9, 8, 10, 12, 14, 7, 5, 3, 1, 0, 2, 4, 6, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
];

const KK_IDX: [[i16; 64]; 10] = [
    [-1, -1, -1, 0, 1, 2, 3, 4, -1, -1, -1, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57],
    [58, -1, -1, -1, 59, 60, 61, 62, 63, -1, -1, -1, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115],
    [116, 117, -1, -1, -1, 118, 119, 120, 121, 122, -1, -1, -1, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159, 160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173],
    [174, -1, -1, -1, 175, 176, 177, 178, 179, -1, -1, -1, 180, 181, 182, 183, 184, -1, -1, -1, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228],
    [229, 230, -1, -1, -1, 231, 232, 233, 234, 235, -1, -1, -1, 236, 237, 238, 239, 240, -1, -1, -1, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255, 256, 257, 258, 259, 260, 261, 262, 263, 264, 265, 266, 267, 268, 269, 270, 271, 272, 273, 274, 275, 276, 277, 278, 279, 280, 281, 282, 283],
    [284, 285, 286, 287, 288, 289, 290, 291, 292, 293, -1, -1, -1, 294, 295, 296, 297, 298, -1, -1, -1, 299, 300, 301, 302, 303, -1, -1, -1, 304, 305, 306, 307, 308, 309, 310, 311, 312, 313, 314, 315, 316, 317, 318, 319, 320, 321, 322, 323, 324, 325, 326, 327, 328, 329, 330, 331, 332, 333, 334, 335, 336, 337, 338],
    [-1, -1, 339, 340, 341, 342, 343, 344, -1, -1, 345, 346, 347, 348, 349, 350, -1, -1, 441, 351, 352, 353, 354, 355, -1, -1, -1, 442, 356, 357, 358, 359, -1, -1, -1, -1, 443, 360, 361, 362, -1, -1, -1, -1, -1, 444, 363, 364, -1, -1, -1, -1, -1, -1, 445, 365, -1, -1, -1, -1, -1, -1, -1, 446],
    [-1, -1, -1, 366, 367, 368, 369, 370, -1, -1, -1, 371, 372, 373, 374, 375, -1, -1, -1, 376, 377, 378, 379, 380, -1, -1, -1, 447, 381, 382, 383, 384, -1, -1, -1, -1, 448, 385, 386, 387, -1, -1, -1, -1, -1, 449, 388, 389, -1, -1, -1, -1, -1, -1, 450, 390, -1, -1, -1, -1, -1, -1, -1, 451],
    [452, 391, 392, 393, 394, 395, 396, 397, -1, -1, -1, -1, 398, 399, 400, 401, -1, -1, -1, -1, 402, 403, 404, 405, -1, -1, -1, -1, 406, 407, 408, 409, -1, -1, -1, -1, 453, 410, 411, 412, -1, -1, -1, -1, -1, 454, 413, 414, -1, -1, -1, -1, -1, -1, 455, 415, -1, -1, -1, -1, -1, -1, -1, 456],
    [457, 416, 417, 418, 419, 420, 421, 422, -1, 458, 423, 424, 425, 426, 427, 428, -1, -1, -1, -1, -1, 429, 430, 431, -1, -1, -1, -1, -1, 432, 433, 434, -1, -1, -1, -1, -1, 435, 436, 437, -1, -1, -1, -1, -1, 459, 438, 439, -1, -1, -1, -1, -1, -1, 460, 440, -1, -1, -1, -1, -1, -1, -1, 461],
];

const FILE_TO_FILE: [u8; 8] = [0, 1, 2, 3, 3, 2, 1, 0];

/// The binomial and pawn index tables, computed once.
struct Indices {
    binomial: [[u64; 64]; 7],
    pawn_idx: [[[u64; 24]; 6]; 2],
    pawn_factor_file: [[u64; 4]; 6],
    pawn_factor_rank: [[u64; 6]; 6],
}

static INDICES: LazyLock<Indices> = LazyLock::new(|| {
    let mut ix = Indices { binomial: [[0; 64]; 7], pawn_idx: [[[0; 24]; 6]; 2], pawn_factor_file: [[0; 4]; 6], pawn_factor_rank: [[0; 6]; 6] };
    for i in 0..7 {
        for j in 0..64 {
            let (mut f, mut l) = (1u64, 1u64);
            for k in 0..i {
                f *= (j - k) as u64;
                l *= (k + 1) as u64;
            }
            ix.binomial[i][j] = f / l;
        }
    }
    for i in 0..6 {
        let mut s = 0u64;
        for j in 0..24 {
            ix.pawn_idx[0][i][j] = s;
            s += ix.binomial[i][PAWN_TWIST[0][(1 + (j % 6)) * 8 + (j / 6)] as usize];
            if (j + 1) % 6 == 0 {
                ix.pawn_factor_file[i][j / 6] = s;
                s = 0;
            }
        }
    }
    for i in 0..6 {
        let mut s = 0u64;
        for j in 0..24 {
            ix.pawn_idx[1][i][j] = s;
            s += ix.binomial[i][PAWN_TWIST[1][(1 + (j / 4)) * 8 + (j % 4)] as usize];
            if (j + 1) % 4 == 0 {
                ix.pawn_factor_rank[i][j / 4] = s;
                s = 0;
            }
        }
    }
    ix
});

/// One encoding of a table: the piece order of the header (Syzygy piece
/// codes 1..14, like kinds consecutive), the group sizes and the
/// mixed-radix factors.
#[derive(Clone, Default)]
pub struct EncInfo {
    /// A Syzygy file's stream, or our own (write.rs; the mapping's bytes go
    /// to every lookup).
    pub precomp: Option<super::pairs::PairsData>,
    pub own: Option<(std::sync::Arc<crate::pairs::Table>, usize, usize)>, // the container and its offset and length in the mapping
    pub factor: [u64; MAX_PIECES],
    pub pieces: [u8; MAX_PIECES],
    pub norm: [u8; MAX_PIECES],
}

/// The shape of a table as the encoding needs it.
pub struct Shape {
    pub num: usize,
    pub kk_enc: bool,
    pub pawns: [u8; 2],
}

/// Moves the leading pawn (the smallest flap value) to the front and
/// returns the file (file encoding) or rank (rank encoding) class.
pub fn leading_pawn(p: &mut [usize], shape: &Shape, enc: usize) -> usize {
    for i in 1..shape.pawns[0] as usize {
        if FLAP[enc - 1][p[0]] > FLAP[enc - 1][p[i]] {
            p.swap(0, i);
        }
    }
    if enc == ENC_FILE {
        FILE_TO_FILE[p[0] & 7] as usize
    } else {
        (p[0] - 8) >> 3
    }
}

/// The index of the squares p (Syzygy squares, in the table's piece order).
pub fn encode(p: &mut [usize], ei: &EncInfo, shape: &Shape, enc: usize) -> u64 {
    let ix = &*INDICES;
    let n = shape.num;
    let mut idx: u64;
    let mut k: usize;
    if p[0] & 0x04 != 0 {
        for v in p.iter_mut().take(n) {
            *v ^= 0x07;
        }
    }
    if enc == ENC_PIECE {
        if p[0] & 0x20 != 0 {
            for v in p.iter_mut().take(n) {
                *v ^= 0x38;
            }
        }
        for i in 0..n {
            if OFF_DIAG[p[i]] != 0 {
                let lim = if shape.kk_enc { 2 } else { 3 };
                if OFF_DIAG[p[i]] > 0 && i < lim {
                    for v in p.iter_mut().take(n) {
                        *v = FLIP_DIAG[*v] as usize;
                    }
                }
                break;
            }
        }
        if shape.kk_enc {
            idx = KK_IDX[TRIANGLE[p[0]] as usize][p[1]] as u64;
            k = 2;
        } else {
            let s1 = (p[1] > p[0]) as usize;
            let s2 = (p[2] > p[0]) as usize + (p[2] > p[1]) as usize;
            idx = if OFF_DIAG[p[0]] != 0 {
                TRIANGLE[p[0]] as u64 * 63 * 62 + (p[1] - s1) as u64 * 62 + (p[2] - s2) as u64
            } else if OFF_DIAG[p[1]] != 0 {
                6 * 63 * 62 + DIAG[p[0]] as u64 * 28 * 62 + LOWER[p[1]] as u64 * 62 + (p[2] - s2) as u64
            } else if OFF_DIAG[p[2]] != 0 {
                6 * 63 * 62 + 4 * 28 * 62 + DIAG[p[0]] as u64 * 7 * 28 + (DIAG[p[1]] as usize - s1) as u64 * 28 + LOWER[p[2]] as u64
            } else {
                6 * 63 * 62 + 4 * 28 * 62 + 4 * 7 * 28 + DIAG[p[0]] as u64 * 7 * 6 + (DIAG[p[1]] as usize - s1) as u64 * 6 + (DIAG[p[2]] as usize - s2) as u64
            };
            k = 3;
        }
        idx *= ei.factor[0];
    } else {
        let np = shape.pawns[0] as usize;
        for i in 1..np {
            for j in i + 1..np {
                if PAWN_TWIST[enc - 1][p[i]] < PAWN_TWIST[enc - 1][p[j]] {
                    p.swap(i, j);
                }
            }
        }
        k = np;
        idx = ix.pawn_idx[enc - 1][k - 1][FLAP[enc - 1][p[0]] as usize];
        for i in 1..k {
            idx += ix.binomial[k - i][PAWN_TWIST[enc - 1][p[i]] as usize];
        }
        idx *= ei.factor[0];
        // pawns of the other colour
        if shape.pawns[1] != 0 {
            let t = k + shape.pawns[1] as usize;
            for i in k..t {
                for j in i + 1..t {
                    if p[i] > p[j] {
                        p.swap(i, j);
                    }
                }
            }
            let mut s = 0u64;
            for i in k..t {
                let sq = p[i];
                let skips = (0..k).filter(|&j| sq > p[j]).count();
                s += ix.binomial[i - k + 1][sq - skips - 8];
            }
            idx += s * ei.factor[k];
            k = t;
        }
    }
    while k < n {
        let t = k + ei.norm[k] as usize;
        for i in k..t {
            for j in i + 1..t {
                if p[i] > p[j] {
                    p.swap(i, j);
                }
            }
        }
        let mut s = 0u64;
        for i in k..t {
            let sq = p[i];
            let skips = (0..k).filter(|&j| sq > p[j]).count();
            s += ix.binomial[i - k + 1][sq - skips];
        }
        idx += s * ei.factor[k];
        k = t;
    }
    idx
}

/// Counts the placements of k like pieces on n squares.
fn subfactor(k: u64, n: u64) -> u64 {
    let (mut f, mut l) = (n, 1u64);
    for i in 1..k {
        f *= n - i;
        l *= i + 1;
    }
    f / l
}

/// Reads the piece order and the group order of one encoding from the
/// header bytes tb (nibble shift 0 or 4 for the two sides of a split
/// table) and computes the factors; returns the table size.
pub fn init_enc_info(ei: &mut EncInfo, shape: &Shape, tb: &[u8], shift: u32, t: usize, enc: usize) -> u64 {
    let more_pawns = enc != ENC_PIECE && shape.pawns[1] > 0;
    let mp = more_pawns as usize;
    for i in 0..shape.num {
        ei.pieces[i] = (tb[i + 1 + mp] >> shift) & 0x0f;
    }
    let order = ((tb[0] >> shift) & 0x0f) as usize;
    let order2 = if more_pawns { ((tb[1] >> shift) & 0x0f) as usize } else { 0x0f };
    setup(ei, shape, order, order2, t, enc)
}

/// Computes the groups (norm) and the factors of an encoding from its piece
/// order (ei.pieces), the chain position of the leading group (order) and
/// of the other colour's pawns (order2, 0x0f when there are none); t is the
/// file or rank class of a pawn table. Returns the table size.
pub fn setup(ei: &mut EncInfo, shape: &Shape, order: usize, order2: usize, t: usize, enc: usize) -> u64 {
    let ix = &*INDICES;
    let more_pawns = enc != ENC_PIECE && shape.pawns[1] > 0;
    for i in 0..shape.num {
        ei.norm[i] = 0;
        ei.factor[i] = 0;
    }
    let mut k = if enc != ENC_PIECE {
        shape.pawns[0] as usize
    } else if shape.kk_enc {
        2
    } else {
        3
    };
    ei.norm[0] = k as u8;
    if more_pawns {
        ei.norm[k] = shape.pawns[1];
        k += ei.norm[k] as usize;
    }
    let mut i = k;
    while i < shape.num {
        let mut j = i;
        while j < shape.num && ei.pieces[j] == ei.pieces[i] {
            ei.norm[i] += 1;
            j += 1;
        }
        i += ei.norm[i] as usize;
    }
    let mut n = (64 - k) as u64;
    let mut f = 1u64;
    let mut i = 0usize;
    while k < shape.num || i == order || i == order2 {
        if i == order {
            ei.factor[0] = f;
            f *= if enc == ENC_FILE {
                ix.pawn_factor_file[ei.norm[0] as usize - 1][t]
            } else if enc == ENC_RANK {
                ix.pawn_factor_rank[ei.norm[0] as usize - 1][t]
            } else if shape.kk_enc {
                462
            } else {
                31332
            };
        } else if i == order2 {
            ei.factor[ei.norm[0] as usize] = f;
            f *= subfactor(ei.norm[ei.norm[0] as usize] as u64, 48 - ei.norm[0] as u64);
        } else {
            ei.factor[k] = f;
            f *= subfactor(ei.norm[k] as u64, n);
            n -= ei.norm[k] as u64;
            k += ei.norm[k] as usize;
        }
        i += 1;
    }
    f
}
