use super::board::*;
use crate::chess::{Piece, Pos, HEIGHT, WIDTH};

pub const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

impl Board {
    /// The start position.
    pub fn new() -> Board {
        Board::from_fen(START_FEN).expect("start FEN is valid")
    }

    /// Parses a FEN string. The halfmove clock and move number are optional.
    pub fn from_fen(fen: &str) -> Result<Board, String> {
        let mut b = Board::empty();
        b.set_fen(fen)?;
        Ok(b)
    }

    /// Replaces the position with the given FEN string.
    pub fn set_fen(&mut self, fen: &str) -> Result<(), String> {
        *self = Board::empty();

        let parts: Vec<&str> = fen.split_whitespace().collect();
        if parts.len() < 4 || parts.len() > 6 {
            return Err(format!("invalid FEN: expected 4 to 6 fields, got {}", parts.len()));
        }

        // 1. pieces
        let ranks: Vec<&str> = parts[0].split('/').collect();
        if ranks.len() != HEIGHT as usize {
            return Err(format!("invalid FEN: expected {} ranks, got {}", HEIGHT, ranks.len()));
        }
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
                    // the move generator relies on this
                    return Err(format!("invalid FEN: pawn on rank {}", HEIGHT as usize - y));
                }
                self.set_field(Pos::from_xy(x, y as i32), p);
                x += 1;
            }
            if x != WIDTH as i32 {
                return Err(format!("invalid FEN: rank {} has {} files", HEIGHT as usize - y, x));
            }
        }
        if !self.white_king.valid() || !self.black_king.valid() {
            return Err("invalid FEN: both kings are required".to_string());
        }

        // 2. side to move
        self.white_move = match parts[1] {
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
                if self.fields[king_sq.idx()] != king || self.fields[rook_sq.idx()] != rook {
                    return Err(format!("invalid FEN: castling right {} without king/rook on home squares", c as char));
                }
                self.castling |= right;
            }
        }

        // 4. en passant (kept only if a pawn can actually capture, see do_move)
        if parts[3] != "-" {
            let ep = Pos::parse(parts[3]);
            if !ep.valid() {
                return Err(format!("invalid FEN: en passant {:?}", parts[3]));
            }
            // white to move: ep square on rank 6, the black pawn stands below it
            let (want_y, pawn_dir) = if self.white_move { (2, WIDTH) } else { (5, -WIDTH) };
            let enemy_pawn = self.side_to_move().opponent() | Piece::PAWN;
            if ep.y() != want_y || self.fields[ep.idx()] != Piece::NONE || self.fields[(ep + pawn_dir).idx()] != enemy_pawn {
                return Err(format!("invalid FEN: en passant {:?} does not match the pawns", parts[3]));
            }
            if self.pawn_can_capture_en_passant(ep + pawn_dir) {
                self.en_passant = ep;
            }
        }

        // 5. halfmove clock
        if let Some(s) = parts.get(4) {
            self.halfmove_clock = match s.parse::<u16>() {
                Ok(n) if n <= 9999 => n,
                _ => return Err(format!("invalid FEN: halfmove clock {:?}", s)),
            };
        }

        // 6. move number
        if let Some(s) = parts.get(5) {
            self.move_number = match s.parse::<u16>() {
                Ok(n) if (1..=9999).contains(&n) => n,
                _ => return Err(format!("invalid FEN: move number {:?}", s)),
            };
        }

        Ok(())
    }

    /// Returns the position as FEN string.
    pub fn fen(&self) -> String {
        let mut s = String::with_capacity(90);
        for y in 0..HEIGHT as i32 {
            if y > 0 {
                s.push('/');
            }
            let mut empty = 0u8;
            for x in 0..WIDTH as i32 {
                let p = self.fields[Pos::from_xy(x, y).idx()];
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

impl Default for Board {
    fn default() -> Board {
        Board::new()
    }
}
