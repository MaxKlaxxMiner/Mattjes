use std::fmt;
use std::ops::{Add, Sub};

pub const WIDTH: i8 = 8;
pub const HEIGHT: i8 = 8;
pub const FIELD_COUNT: usize = (WIDTH * HEIGHT) as usize;

/// A square index 0..63 with a8 = 0, h8 = 7, a1 = 56, h1 = 63.
/// Rank 8 is at the top (y = 0), so white pawns move towards smaller indices.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Debug)]
pub struct Pos(pub i8);

impl Pos {
    /// Marks "no square" (e.g. no en passant square).
    pub const NONE: Pos = Pos(-1);

    pub const fn from_xy(x: i32, y: i32) -> Pos {
        if x < 0 || x >= WIDTH as i32 || y < 0 || y >= HEIGHT as i32 {
            return Pos::NONE;
        }
        Pos((x + y * WIDTH as i32) as i8)
    }

    /// Parses algebraic notation like "e4". Invalid input returns NONE.
    pub fn parse(s: &str) -> Pos {
        let b = s.as_bytes();
        if b.len() != 2 {
            return Pos::NONE;
        }
        let x = b[0] as i32 - 'a' as i32;
        let y = HEIGHT as i32 - (b[1] as i32 - '0' as i32);
        Pos::from_xy(x, y)
    }

    pub const fn valid(self) -> bool {
        (self.0 as u8) < FIELD_COUNT as u8
    }

    /// The array index. Only call on valid squares.
    #[inline(always)]
    pub const fn idx(self) -> usize {
        self.0 as usize
    }

    pub const fn x(self) -> i8 {
        self.0 % WIDTH
    }

    pub const fn y(self) -> i8 {
        self.0 / WIDTH
    }

    /// The chess rank 1..8.
    pub const fn rank(self) -> i8 {
        HEIGHT - self.y()
    }
}

/// `pos + delta` steps across the board. No bounds check, callers use the edge tables.
impl Add<i8> for Pos {
    type Output = Pos;
    #[inline(always)]
    fn add(self, d: i8) -> Pos {
        Pos(self.0 + d)
    }
}

impl Sub<i8> for Pos {
    type Output = Pos;
    #[inline(always)]
    fn sub(self, d: i8) -> Pos {
        Pos(self.0 - d)
    }
}

/// `to - from` is the signed distance between two squares.
impl Sub<Pos> for Pos {
    type Output = i8;
    #[inline(always)]
    fn sub(self, other: Pos) -> i8 {
        self.0 - other.0
    }
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.valid() {
            return write!(f, "-");
        }
        write!(f, "{}{}", (b'a' + self.x() as u8) as char, self.rank())
    }
}
