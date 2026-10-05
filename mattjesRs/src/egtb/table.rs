//! Tables, values and the lookup, see `mattjesGo/egtb/table.go`.

use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};

use super::index::{KkTable, PAWN_OFFSET, PAWN_SQUARES, PIECE_SQUARES, SYMMETRY};
use super::{board_signature, Material, SIGNATURE_SPACE};
use crate::bitboard::Board;
use crate::chess::{Piece, Pos};

/// The table entry of a position from the point of view of the side to move:
/// 0 draw, 1..127 mates in n plies, 128 invalid position, 129..255 gets mated in
/// n-129 plies (129 = is mated right now).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(transparent)]
pub struct Value(pub u8);

const LOSS_BASE: u8 = 129;
/// The longest distance either side can express.
pub const MAX_PLIES: u32 = 126;

impl Value {
    pub const DRAW: Value = Value(0);
    pub const INVALID: Value = Value(128);

    pub fn win_in(plies: u32) -> Value {
        Value(plies as u8)
    }

    pub fn loss_in(plies: u32) -> Value {
        Value(LOSS_BASE + plies as u8)
    }

    pub fn is_win(self) -> bool {
        self.0 >= 1 && self.0 < Value::INVALID.0
    }

    pub fn is_loss(self) -> bool {
        self.0 > Value::INVALID.0
    }

    /// The distance to mate for a win or a loss, 0 otherwise.
    pub fn plies(self) -> u32 {
        if self.is_win() {
            self.0 as u32
        } else if self.is_loss() {
            (self.0 - LOSS_BASE) as u32
        } else {
            0
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Value::DRAW {
            write!(f, "draw")
        } else if *self == Value::INVALID {
            write!(f, "invalid")
        } else if self.is_win() {
            write!(f, "win in {}", self.plies())
        } else {
            write!(f, "loss in {}", self.plies())
        }
    }
}

/// The endgame table of one material: a dense array indexed by the position.
/// Squares are listed as white king, black king, then the slots of the
/// material in order. The values are atomic bytes so that the generator's
/// workers can write distinct entries without locks; reads are plain loads.
pub struct Table {
    pub mat: Material,
    /// Colored pieces after the kings.
    pub slots: Vec<Piece>,
    /// Index digits: 64 per piece, 48 per pawn.
    sizes: Vec<usize>,
    /// Slot holds the same piece as the previous slot: squares are kept ascending.
    equal_prev: Vec<bool>,
    pawns: bool,
    pub size: usize,
    /// Empty until generated or loaded.
    pub values: Vec<AtomicU8>,
}

impl Table {
    fn new(mat: Material) -> Table {
        let slots = mat.slots();
        let equal_prev = (0..slots.len()).map(|i| i > 0 && slots[i - 1] == slots[i]).collect();
        let pawns = mat.pawns() > 0;
        let mut size = 2 * if pawns { SYMMETRY.pawns.squares.len() } else { SYMMETRY.pawnless.squares.len() };
        let sizes: Vec<usize> = slots.iter().map(|p| if p.is(Piece::PAWN) { PAWN_SQUARES } else { PIECE_SQUARES }).collect();
        for s in &sizes {
            size *= s;
        }
        Table { mat, slots, sizes, equal_prev, pawns, size, values: Vec::new() }
    }

    #[inline(always)]
    fn kk(&self) -> &'static KkTable {
        if self.pawns {
            &SYMMETRY.pawns
        } else {
            &SYMMETRY.pawnless
        }
    }

    pub fn is_generated(&self) -> bool {
        !self.values.is_empty()
    }

    #[inline(always)]
    pub fn value(&self, idx: usize) -> Value {
        Value(self.values[idx].load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub fn set_value(&self, idx: usize, v: Value) {
        self.values[idx].store(v.0, Ordering::Relaxed)
    }

    /// The position index; -1 if the kings are adjacent or equal. A square
    /// occupied twice still gives an index (marked `INVALID` in the table). Two
    /// equal pieces are ordered by square, so both orders give the same index;
    /// the indices with the other order are dead (`decode` does not know that,
    /// the generator marks them `INVALID`).
    pub fn index(&self, white_move: bool, sq: &[Pos]) -> i64 {
        let kk = self.kk();
        let (k, tf) = kk.at(sq[0], sq[1]);
        if k < 0 {
            return -1;
        }
        let mut base = k as usize;
        if !white_move {
            base += kk.squares.len();
        }
        let mut idx = self.digits(base, tf, sq);
        if kk.diagonal[k as usize] {
            // both kings on the diagonal: the kings do not fix the transposition,
            // so the smaller of the two indices is the canonical one (independent
            // of the order in which equal pieces are listed)
            idx = idx.min(self.digits(base, tf | 4, sq));
        }
        idx as i64
    }

    /// Appends the piece squares, transformed by tf, to the king pair index.
    fn digits(&self, mut idx: usize, tf: usize, sq: &[Pos]) -> usize {
        let xf = &SYMMETRY.xform[tf];
        let mut s = [0usize; 4];
        for i in 0..self.sizes.len() {
            s[i] = xf[sq[2 + i].idx()].idx();
            if self.equal_prev[i] && s[i] < s[i - 1] {
                s.swap(i, i - 1);
            }
        }
        for (i, &size) in self.sizes.iter().enumerate() {
            let mut d = s[i];
            if size == PAWN_SQUARES {
                d -= PAWN_OFFSET;
            }
            idx = idx * size + d;
        }
        idx
    }

    /// The inverse of `index`: fills sq (len 2 + slots) and returns the side to
    /// move. Decoded positions are always in canonical form, but an index with two
    /// equal pieces in descending square order (or the transposed twin when both
    /// kings are on the diagonal) is dead: index(decode(idx)) then differs from
    /// idx, which is how the generator recognizes it.
    pub fn decode(&self, mut idx: usize, sq: &mut [Pos]) -> bool {
        for i in (0..self.sizes.len()).rev() {
            let size = self.sizes[i];
            let mut s = idx % size;
            idx /= size;
            if size == PAWN_SQUARES {
                s += PAWN_OFFSET;
            }
            sq[2 + i] = Pos(s as i8);
        }
        let kk = self.kk();
        let n = kk.squares.len();
        let white_move = idx < n;
        if !white_move {
            idx -= n;
        }
        sq[0] = kk.squares[idx][0];
        sq[1] = kk.squares[idx][1];
        white_move
    }

    /// The position of an index (a convenience for tests and output; the
    /// generator does the same inline).
    pub fn board(&self, idx: usize) -> Board {
        let mut sq = vec![Pos::NONE; 2 + self.slots.len()];
        let white_move = self.decode(idx, &mut sq);
        let mut pieces = vec![Piece::WHITE_KING, Piece::BLACK_KING];
        pieces.extend_from_slice(&self.slots);
        Board::from_pieces(white_move, &sq, &pieces)
    }

    /// Extracts the squares of the board in slot order, with the colors swapped
    /// and the board mirrored vertically when flip is set. Returns the count.
    fn squares(&self, b: &Board, flip: bool, sq: &mut [Pos]) -> usize {
        let m: i8 = if flip { 56 } else { 0 };
        let (us, them) = if flip { (1, 0) } else { (0, 1) };
        sq[0] = Pos(b.king_pos(us).0 ^ m);
        sq[1] = Pos(b.king_pos(them).0 ^ m);
        let mut taken = 0u64;
        for (i, &p) in self.slots.iter().enumerate() {
            let p = if flip { p.opponent() } else { p }; // xor of the color bits
            let bb = b.piece_bb(p) & !taken;
            let s = bb.trailing_zeros() as i8;
            taken |= 1 << s;
            sq[2 + i] = Pos(s ^ m);
        }
        2 + self.slots.len()
    }

    /// The values as plain bytes. Only call while no worker writes (the
    /// generator has finished): `AtomicU8` is `repr(transparent)` over `u8`.
    pub fn bytes(&self) -> &[u8] {
        // SAFETY: AtomicU8 has the same size and alignment as u8, and the slice
        // is only read while the table is not being generated.
        unsafe { std::slice::from_raw_parts(self.values.as_ptr() as *const u8, self.values.len()) }
    }
}

/// The complete collection of tables for all materials up to four pieces.
pub struct Set {
    pub tables: Vec<Table>,
    /// signature -> table index, -1 none
    by_sig: Vec<i8>,
}

impl Set {
    /// Allocates the tables without values; `generate_all` or `load` fills them.
    pub fn new() -> Set {
        let mut s = Set { tables: Vec::new(), by_sig: vec![-1; SIGNATURE_SPACE] };
        for (i, m) in Material::all().into_iter().enumerate() {
            s.by_sig[m.signature() as usize] = i as i8;
            s.tables.push(Table::new(m));
        }
        s
    }

    /// The table of a material by name, e.g. "KBNK".
    pub fn find(&self, name: &str) -> Option<usize> {
        self.tables.iter().position(|t| t.mat.name() == name)
    }

    /// The number of bytes of all tables.
    pub fn total_size(&self) -> usize {
        self.tables.iter().map(|t| t.size).sum()
    }

    /// The value of a position with up to four pieces, without castling rights
    /// and without an en passant square. `None` when the position is not covered:
    /// more pieces, castling or en passant (the search plays one more ply) or a
    /// table that is not computed yet. King against king is a draw without a
    /// table.
    pub fn lookup(&self, b: &Board) -> Option<Value> {
        let occ = b.by_color[0] | b.by_color[1];
        if occ.count_ones() == 2 && b.castling == 0 {
            return Some(Value::DRAW);
        }
        let (t, idx) = self.locate(b)?;
        if !t.is_generated() {
            return None;
        }
        if idx < 0 {
            return Some(Value::INVALID);
        }
        Some(t.value(idx as usize))
    }

    /// The table and the index of a position with three or four pieces, no
    /// castling rights and no en passant square. The index is -1 for adjacent
    /// kings. `None` when no table covers the position.
    pub fn locate(&self, b: &Board) -> Option<(&Table, i64)> {
        if b.castling != 0 || b.en_passant.valid() {
            return None;
        }
        let occ = b.by_color[0] | b.by_color[1];
        let n = occ.count_ones();
        if !(3..=4).contains(&n) {
            return None;
        }
        let (sig, flipped) = board_signature(b);
        let mut flip = false;
        let mut ti = self.by_sig[sig as usize];
        if ti < 0 {
            ti = self.by_sig[flipped as usize];
            flip = true;
        }
        if ti < 0 {
            return None;
        }
        let t = &self.tables[ti as usize];
        let mut sq = [Pos::NONE; 6];
        let n = t.squares(b, flip, &mut sq);
        Some((t, t.index(b.white_move != flip, &sq[..n])))
    }
}

impl Default for Set {
    fn default() -> Set {
        Set::new()
    }
}
