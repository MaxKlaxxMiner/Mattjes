use std::fmt;
use std::ops::{BitAnd, BitOr};

/// A piece encodes color and type in one byte. The encoding is identical to the
/// Go version, yacboard and the old C# code, so positions and hashes stay comparable.
///
/// This is a "newtype": a struct wrapping a plain `u8`. Unlike a type alias it is a
/// distinct type, so a `Piece` cannot be mixed up with an ordinary number.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Piece(pub u8);

impl Piece {
    pub const NONE: Piece = Piece(0x00);

    pub const KING: Piece = Piece(0x01);
    pub const QUEEN: Piece = Piece(0x02);
    pub const ROOK: Piece = Piece(0x04);
    pub const BISHOP: Piece = Piece(0x08);
    pub const KNIGHT: Piece = Piece(0x10);
    pub const PAWN: Piece = Piece(0x20);
    pub const TYPE_MASK: Piece = Piece(0x3f);

    pub const WHITE: Piece = Piece(0x40);
    pub const BLACK: Piece = Piece(0x80);
    pub const COLOR_MASK: Piece = Piece(0xc0);

    /// Marks an invalid piece (e.g. unknown FEN character).
    pub const BLOCKED: Piece = Piece(0xc0);

    pub const WHITE_KING: Piece = Piece(0x40 | 0x01);
    pub const WHITE_QUEEN: Piece = Piece(0x40 | 0x02);
    pub const WHITE_ROOK: Piece = Piece(0x40 | 0x04);
    pub const WHITE_BISHOP: Piece = Piece(0x40 | 0x08);
    pub const WHITE_KNIGHT: Piece = Piece(0x40 | 0x10);
    pub const WHITE_PAWN: Piece = Piece(0x40 | 0x20);
    pub const BLACK_KING: Piece = Piece(0x80 | 0x01);
    pub const BLACK_QUEEN: Piece = Piece(0x80 | 0x02);
    pub const BLACK_ROOK: Piece = Piece(0x80 | 0x04);
    pub const BLACK_BISHOP: Piece = Piece(0x80 | 0x08);
    pub const BLACK_KNIGHT: Piece = Piece(0x80 | 0x10);
    pub const BLACK_PAWN: Piece = Piece(0x80 | 0x20);

    /// Returns WHITE, BLACK or NONE.
    pub const fn color(self) -> Piece {
        Piece(self.0 & Self::COLOR_MASK.0)
    }

    /// Returns the piece type without color.
    pub const fn kind(self) -> Piece {
        Piece(self.0 & Self::TYPE_MASK.0)
    }

    /// Reports whether any of the given bits is set.
    pub const fn is(self, t: Piece) -> bool {
        self.0 & t.0 != 0
    }

    /// Returns the opposite color of a color value.
    pub const fn opponent(self) -> Piece {
        Piece(self.0 ^ Self::COLOR_MASK.0)
    }

    /// Converts a FEN character to a piece. Unknown characters return BLOCKED.
    pub fn from_char(c: u8) -> Piece {
        match c {
            b'K' => Self::WHITE_KING,
            b'Q' => Self::WHITE_QUEEN,
            b'R' => Self::WHITE_ROOK,
            b'B' => Self::WHITE_BISHOP,
            b'N' => Self::WHITE_KNIGHT,
            b'P' => Self::WHITE_PAWN,
            b'k' => Self::BLACK_KING,
            b'q' => Self::BLACK_QUEEN,
            b'r' => Self::BLACK_ROOK,
            b'b' => Self::BLACK_BISHOP,
            b'n' => Self::BLACK_KNIGHT,
            b'p' => Self::BLACK_PAWN,
            _ => Self::BLOCKED,
        }
    }

    /// Returns the FEN character, '.' for an empty square and '?' for invalid values.
    pub fn to_char(self) -> u8 {
        let c = match self.kind() {
            Self::KING => b'k',
            Self::QUEEN => b'q',
            Self::ROOK => b'r',
            Self::BISHOP => b'b',
            Self::KNIGHT => b'n',
            Self::PAWN => b'p',
            Self::NONE => return b'.',
            _ => return b'?',
        };
        if self.is(Self::WHITE) {
            c.to_ascii_uppercase()
        } else {
            c
        }
    }
}

impl BitOr for Piece {
    type Output = Piece;
    fn bitor(self, rhs: Piece) -> Piece {
        Piece(self.0 | rhs.0)
    }
}

impl BitAnd for Piece {
    type Output = Piece;
    fn bitand(self, rhs: Piece) -> Piece {
        Piece(self.0 & rhs.0)
    }
}

impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_char() as char)
    }
}
