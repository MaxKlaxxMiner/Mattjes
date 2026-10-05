//! Board symmetry and the king pair tables, see `mattjesGo/egtb/index.go`.

use std::sync::LazyLock;

use crate::chess::{Pos, FIELD_COUNT, HEIGHT, WIDTH};

/// A transform id has three bits: 1 = mirror files (a<->h), 2 = mirror ranks
/// (1<->8), 4 = transpose (a1-h8 diagonal). Without pawns all eight are
/// available, with pawns only the file mirror.
pub fn transform_square(t: usize, sq: Pos) -> Pos {
    let (mut f, mut r) = (sq.x() as i32, (HEIGHT - 1 - sq.y()) as i32); // file and rank, a1 = (0,0)
    if t & 1 != 0 {
        f = WIDTH as i32 - 1 - f;
    }
    if t & 2 != 0 {
        r = HEIGHT as i32 - 1 - r;
    }
    if t & 4 != 0 {
        std::mem::swap(&mut f, &mut r);
    }
    Pos::from_xy(f, HEIGHT as i32 - 1 - r)
}

/// White king in the triangle a1-d1-d4, black king below or on the diagonal
/// when white's is on it.
pub const KK_PAWNLESS: usize = 462;
/// White king on the files a-d.
pub const KK_PAWNS: usize = 1806;

/// Pawns live on the ranks 2 to 7: 48 squares, index = square - 8.
pub const PAWN_SQUARES: usize = 48;
pub const PIECE_SQUARES: usize = FIELD_COUNT;
pub const PAWN_OFFSET: usize = WIDTH as usize;

pub struct KkTable {
    /// [white king][black king] -> dense index, -1 illegal.
    pub index: Vec<i16>,
    /// Transform that produces the canonical pair.
    pub xform: Vec<u8>,
    /// Dense index -> canonical (white king, black king).
    pub squares: Vec<[Pos; 2]>,
    /// Pairs with both kings on the a1-h8 diagonal: the kings do not fix the
    /// transposition, the first other piece off the diagonal does (it is moved
    /// below). Without that rule a position and its transposed twin would get
    /// two indices whose children coincide, and the un-moves from a child would
    /// reach only one of the twins.
    pub diagonal: Vec<bool>,
}

impl KkTable {
    #[inline(always)]
    pub fn at(&self, wk: Pos, bk: Pos) -> (i32, usize) {
        let i = wk.idx() * FIELD_COUNT + bk.idx();
        (self.index[i] as i32, self.xform[i] as usize)
    }
}

pub struct Symmetry {
    pub xform: [[Pos; FIELD_COUNT]; 8],
    pub pawnless: KkTable,
    pub pawns: KkTable,
}

pub static SYMMETRY: LazyLock<Symmetry> = LazyLock::new(Symmetry::build);

fn kings_adjacent(a: Pos, b: Pos) -> bool {
    let (dx, dy) = (a.x() - b.x(), a.y() - b.y());
    (-1..=1).contains(&dx) && (-1..=1).contains(&dy)
}

/// One of the ten squares a1, b1, b2, c1, c2, c3, d1, d2, d3, d4 (file <= 3, rank <= file).
fn in_triangle(sq: Pos) -> bool {
    let (f, r) = (sq.x(), HEIGHT - 1 - sq.y());
    f <= 3 && r <= f
}

pub fn below_or_on_diagonal(sq: Pos) -> bool {
    let (f, r) = (sq.x(), HEIGHT - 1 - sq.y());
    r <= f
}

pub fn on_diagonal(sq: Pos) -> bool {
    sq.x() == HEIGHT - 1 - sq.y()
}

impl Symmetry {
    fn build() -> Symmetry {
        let mut xform = [[Pos::NONE; FIELD_COUNT]; 8];
        for (t, row) in xform.iter_mut().enumerate() {
            for (i, cell) in row.iter_mut().enumerate() {
                *cell = transform_square(t, Pos(i as i8));
            }
        }
        let pawnless = build_kk(&xform, 8, &|wk, bk| {
            if !in_triangle(wk) {
                return false;
            }
            if on_diagonal(wk) {
                return below_or_on_diagonal(bk); // white king on the diagonal: the black king decides
            }
            true
        });
        let pawns = build_kk(&xform, 2, &|wk, _| wk.x() <= 3);
        assert_eq!(pawnless.squares.len(), KK_PAWNLESS, "pawnless king pairs");
        assert_eq!(pawns.squares.len(), KK_PAWNS, "king pairs with pawns");
        Symmetry { xform, pawnless, pawns }
    }
}

/// Fills a king pair table. `canonical` reports whether (wk, bk) is in canonical
/// form; the transform with the lowest id that makes a pair canonical is
/// chosen, so a pair that already is canonical keeps the identity.
fn build_kk(xform: &[[Pos; FIELD_COUNT]; 8], transforms: usize, canonical: &dyn Fn(Pos, Pos) -> bool) -> KkTable {
    let mut t = KkTable { index: vec![-1; FIELD_COUNT * FIELD_COUNT], xform: vec![0; FIELD_COUNT * FIELD_COUNT], squares: Vec::new(), diagonal: Vec::new() };
    for wk in 0..FIELD_COUNT {
        for bk in 0..FIELD_COUNT {
            let (w, b) = (Pos(wk as i8), Pos(bk as i8));
            if w == b || kings_adjacent(w, b) {
                continue;
            }
            for (tf, xf) in xform.iter().enumerate().take(transforms) {
                if !canonical(xf[wk], xf[bk]) {
                    continue;
                }
                t.xform[wk * FIELD_COUNT + bk] = tf as u8;
                if tf == 0 {
                    t.index[wk * FIELD_COUNT + bk] = t.squares.len() as i16;
                    t.squares.push([w, b]);
                    t.diagonal.push(transforms == 8 && on_diagonal(w) && on_diagonal(b));
                }
                break;
            }
        }
    }
    // second pass: non-canonical pairs point to the index of their canonical form
    for wk in 0..FIELD_COUNT {
        for bk in 0..FIELD_COUNT {
            let (w, b) = (Pos(wk as i8), Pos(bk as i8));
            let i = wk * FIELD_COUNT + bk;
            if w == b || kings_adjacent(w, b) || t.index[i] >= 0 {
                continue;
            }
            let tf = t.xform[i] as usize;
            t.index[i] = t.index[xform[tf][wk].idx() * FIELD_COUNT + xform[tf][bk].idx()];
        }
    }
    t
}
