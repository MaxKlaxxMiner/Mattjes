use std::fmt;

use super::mv::*;
use super::{Piece, Pos, FIELD_COUNT, HEIGHT, WIDTH};

pub const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// A validated, representation-independent position. Every board type converts
/// from and to it, so FEN parsing and validation exist only once.
///
/// Invariants guaranteed after `parse` (generators may rely on them):
///   - exactly one king per color, no pawns on rank 1 or 8
///   - castling rights only with king and rook on their home squares
///   - `en_passant` is set only if a pawn of the side to move can actually capture there
#[derive(Clone, Copy, Debug)]
pub struct Setup {
    pub squares: [Piece; FIELD_COUNT],
    pub white_move: bool,
    pub castling: Castling,
    pub en_passant: Pos,
    pub halfmove_clock: u16,
    pub move_number: u16,
}

impl Setup {
    pub fn side_to_move(&self) -> Piece {
        if self.white_move {
            Piece::WHITE
        } else {
            Piece::BLACK
        }
    }

    /// Parses and validates a FEN string. Halfmove clock and move number are optional.
    pub fn parse(fen: &str) -> Result<Setup, String> {
        let mut s = Setup {
            squares: [Piece::NONE; FIELD_COUNT],
            white_move: true,
            castling: 0,
            en_passant: Pos::NONE,
            halfmove_clock: 0,
            move_number: 1,
        };

        let parts: Vec<&str> = fen.split_whitespace().collect();
        if parts.len() < 4 || parts.len() > 6 {
            return Err(format!("invalid FEN: expected 4 to 6 fields, got {}", parts.len()));
        }

        // 1. pieces
        let ranks: Vec<&str> = parts[0].split('/').collect();
        if ranks.len() != HEIGHT as usize {
            return Err(format!("invalid FEN: expected {} ranks, got {}", HEIGHT, ranks.len()));
        }
        let mut kings = [0u8; 2];
        for (y, rank) in ranks.iter().enumerate() {
            let mut x = 0i32;
            for &c in rank.as_bytes() {
                if (b'1'..=b'8').contains(&c) {
                    x += (c - b'0') as i32;
                    continue;
                }
                let p = Piece::from_char(c);
                if p == Piece::BLOCKED {
                    return Err(format!("invalid FEN: unknown piece {:?}", c as char));
                }
                if x >= WIDTH as i32 {
                    return Err(format!("invalid FEN: rank {} too long", HEIGHT as usize - y));
                }
                if p.is(Piece::PAWN) && (y == 0 || y == HEIGHT as usize - 1) {
                    return Err(format!("invalid FEN: pawn on rank {}", HEIGHT as usize - y));
                }
                if p.is(Piece::KING) {
                    kings[(p.0 >> 7) as usize] += 1;
                }
                s.squares[Pos::from_xy(x, y as i32).idx()] = p;
                x += 1;
            }
            if x != WIDTH as i32 {
                return Err(format!("invalid FEN: rank {} has {} files", HEIGHT as usize - y, x));
            }
        }
        if kings != [1, 1] {
            return Err("invalid FEN: expected exactly one king per color".to_string());
        }

        // 2. side to move
        s.white_move = match parts[1] {
            "w" => true,
            "b" => false,
            other => return Err(format!("invalid FEN: side to move {:?}", other)),
        };

        // 3. castling rights (validated against king and rook squares)
        if parts[2] != "-" {
            for c in parts[2].bytes() {
                let (right, king_sq, rook_sq, king, rook) = match c {
                    b'K' => (WHITE_KINGSIDE, Pos(60), Pos(63), Piece::WHITE_KING, Piece::WHITE_ROOK),
                    b'Q' => (WHITE_QUEENSIDE, Pos(60), Pos(56), Piece::WHITE_KING, Piece::WHITE_ROOK),
                    b'k' => (BLACK_KINGSIDE, Pos(4), Pos(7), Piece::BLACK_KING, Piece::BLACK_ROOK),
                    b'q' => (BLACK_QUEENSIDE, Pos(4), Pos(0), Piece::BLACK_KING, Piece::BLACK_ROOK),
                    _ => return Err(format!("invalid FEN: castling {:?}", parts[2])),
                };
                if s.squares[king_sq.idx()] != king || s.squares[rook_sq.idx()] != rook {
                    return Err(format!("invalid FEN: castling right {} without king/rook on home squares", c as char));
                }
                s.castling |= right;
            }
        }

        // 4. en passant
        if parts[3] != "-" {
            let ep = Pos::parse(parts[3]);
            if !ep.valid() {
                return Err(format!("invalid FEN: en passant {:?}", parts[3]));
            }
            // white to move: ep square on rank 6, the black pawn stands below it
            let (want_y, pawn_dir) = if s.white_move { (2, WIDTH) } else { (5, -WIDTH) };
            let pawn_sq = ep + pawn_dir;
            let enemy_pawn = s.side_to_move().opponent() | Piece::PAWN;
            if ep.y() != want_y || s.squares[ep.idx()] != Piece::NONE || s.squares[pawn_sq.idx()] != enemy_pawn {
                return Err(format!("invalid FEN: en passant {:?} does not match the pawns", parts[3]));
            }
            if s.pawn_can_capture_en_passant(pawn_sq) {
                s.en_passant = ep;
            }
        }

        // 5. halfmove clock
        if let Some(v) = parts.get(4) {
            s.halfmove_clock = match v.parse::<u16>() {
                Ok(n) if n <= 9999 => n,
                _ => return Err(format!("invalid FEN: halfmove clock {:?}", v)),
            };
        }

        // 6. move number
        if let Some(v) = parts.get(5) {
            s.move_number = match v.parse::<u16>() {
                Ok(n) if (1..=9999).contains(&n) => n,
                _ => return Err(format!("invalid FEN: move number {:?}", v)),
            };
        }

        Ok(s)
    }

    /// Reports whether an enemy pawn stands directly left or right of the pawn on `pawn_sq`.
    fn pawn_can_capture_en_passant(&self, pawn_sq: Pos) -> bool {
        let enemy_pawn = self.squares[pawn_sq.idx()].color().opponent() | Piece::PAWN;
        (pawn_sq.x() > 0 && self.squares[(pawn_sq - 1).idx()] == enemy_pawn)
            || (pawn_sq.x() < WIDTH - 1 && self.squares[(pawn_sq + 1).idx()] == enemy_pawn)
    }

    /// The position as FEN string.
    pub fn fen(&self) -> String {
        let mut s = String::with_capacity(90);
        for y in 0..HEIGHT as i32 {
            if y > 0 {
                s.push('/');
            }
            let mut empty = 0u8;
            for x in 0..WIDTH as i32 {
                let p = self.squares[Pos::from_xy(x, y).idx()];
                if p == Piece::NONE {
                    empty += 1;
                    continue;
                }
                if empty > 0 {
                    s.push((b'0' + empty) as char);
                    empty = 0;
                }
                s.push(p.to_char() as char);
            }
            if empty > 0 {
                s.push((b'0' + empty) as char);
            }
        }

        s.push_str(if self.white_move { " w " } else { " b " });

        if self.castling == 0 {
            s.push('-');
        } else {
            for (i, c) in "KQkq".chars().enumerate() {
                if self.castling & (1 << i) != 0 {
                    s.push(c);
                }
            }
        }

        s.push_str(&format!(" {} {} {}", self.en_passant, self.halfmove_clock, self.move_number));
        s
    }
}

/// ASCII diagram followed by the FEN.
impl fmt::Display for Setup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for y in 0..HEIGHT as i32 {
            write!(f, "    ")?;
            for x in 0..WIDTH as i32 {
                write!(f, "{}", self.squares[Pos::from_xy(x, y).idx()])?;
            }
            writeln!(f)?;
        }
        write!(f, "\nFEN: {}", self.fen())
    }
}
