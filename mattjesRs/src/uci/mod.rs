//! The Universal Chess Interface (milestone 6, stage a), direct port of
//! `mattjesGo/uci`: positions covered by the endgame tables are answered
//! straight from them, with the exact distance to mate and the complete
//! winning line; every root move gets its own value, so MultiPV lists them
//! all. Positions without a table get a legal move and an "info string" for
//! now; the search algorithms come in later stages.
//!
//! Protocol subset: uci, isready, setoption (MultiPV, Threads), ucinewgame,
//! position, go (all parameters ignored, the answer is immediate), stop, quit,
//! d (debug: print the FEN).

use std::io::{BufRead, Write};

use crate::bitboard::Board;
use crate::chess::{new_buffer, Move};
use crate::egtb::{self, Material, Set, Value, MAX_PLIES};

/// Reported in "id name"; it follows the milestone that is being worked on.
pub const VERSION: &str = "0.6";

/// The UCI_EngineAbout option, shown by GUIs as the engine description.
pub const ABOUT: &str = "Mattjes 0.6 by Max Klaxx Miner, a mate and draw search engine with its own endgame tables, GPLv3, https://github.com/MaxKlaxxMiner/Mattjes";

/// The state between commands.
pub struct Engine<W: Write> {
    out: W,
    set: Option<Set>,
    board: Board,
    multi_pv: usize,
    workers: usize,
    name: String,
}

/// Reads commands from `input` until quit or EOF. `name` is the engine name
/// reported to the GUI (the Go and Rust binaries differ).
pub fn run<R: BufRead, W: Write>(input: R, out: W, name: &str, workers: usize) {
    let mut e = Engine { out, set: None, board: Board::new(), multi_pv: 1, workers, name: name.to_string() };
    for line in input.lines() {
        let Ok(line) = line else { break };
        let words: Vec<&str> = line.split_whitespace().collect();
        if !e.command(&words) {
            break;
        }
        let _ = e.out.flush();
    }
    let _ = e.out.flush();
}

/// One root move with its value from the side to move's view.
struct RootMove {
    mv: Move,
    value: Value,
    known: bool,
    pv: Vec<Move>,
}

impl<W: Write> Engine<W> {
    fn send(&mut self, line: &str) {
        let _ = writeln!(self.out, "{}", line);
    }

    /// Handles one line; false means quit.
    fn command(&mut self, words: &[&str]) -> bool {
        let Some(&cmd) = words.first() else { return true };
        match cmd {
            "uci" => {
                let name = self.name.clone();
                self.send(&format!("id name {}", name));
                self.send("id author Max Klaxx Miner");
                self.send("option name MultiPV type spin default 1 min 1 max 256");
                self.send(&format!("option name Threads type spin default {} min 1 max 64", self.workers));
                self.send(&format!("option name UCI_EngineAbout type string default {}", ABOUT));
                self.send("uciok");
            }
            // Winboard probes from a GUI's auto-detection: stay silent, so that only UCI answers are seen
            "xboard" | "protover" | "new" | "force" => {}
            "isready" => {
                self.ensure_tables();
                self.send("readyok");
            }
            "setoption" => self.set_option(&words[1..]),
            "ucinewgame" => {}
            "position" => self.position(&words[1..]),
            "go" => {
                self.ensure_tables();
                self.search();
            }
            "stop" => {} // the answer is always sent at once, nothing runs in the background yet
            "d" => {
                let fen = self.board.fen();
                self.send(&fen);
            }
            "quit" => return false,
            other => self.send(&format!("info string unknown command {}", other)),
        }
        true
    }

    fn set_option(&mut self, words: &[&str]) {
        // setoption name <id> [value <x>]
        let (mut name, mut value) = ("", "");
        for i in 0..words.len() {
            match words[i] {
                "name" => name = words.get(i + 1).copied().unwrap_or(""),
                "value" => value = words.get(i + 1).copied().unwrap_or(""),
                _ => {}
            }
        }
        if name.eq_ignore_ascii_case("UCI_EngineAbout") {
            return;
        }
        match value.parse::<usize>() {
            Err(_) => self.send(&format!("info string option {}: bad value {:?}", name, value)),
            Ok(n) if name.eq_ignore_ascii_case("MultiPV") => self.multi_pv = n.max(1),
            Ok(n) if name.eq_ignore_ascii_case("Threads") => self.workers = n.max(1),
            Ok(_) => self.send(&format!("info string unknown option {}", name)),
        }
    }

    /// Loads the four-piece base once (generating it on the first start, which
    /// takes about 17 s and is reported as info strings).
    fn ensure_tables(&mut self) {
        if self.set.is_some() {
            return;
        }
        let out = &mut self.out;
        let set = Set::load_or_generate(&egtb::default_path(), self.workers, &mut |line| {
            let _ = writeln!(out, "info string {}", line);
            let _ = out.flush();
        });
        self.set = Some(set);
    }

    /// position [startpos | fen <6 fields>] [moves <uci>...]
    fn position(&mut self, words: &[&str]) {
        let mut i = 0;
        if words.first() == Some(&"startpos") {
            self.board = Board::new();
            i = 1;
        } else if words.first() == Some(&"fen") {
            let j = words.iter().position(|&w| w == "moves").unwrap_or(words.len());
            match Board::from_fen(&words[1..j].join(" ")) {
                Ok(b) => self.board = b,
                Err(e) => {
                    self.send(&format!("info string bad fen: {}", e));
                    return;
                }
            }
            i = j;
        }
        if words.get(i) == Some(&"moves") {
            for &s in &words[i + 1..] {
                match self.find_move(s) {
                    Some(m) => self.board.do_move(m),
                    None => {
                        let fen = self.board.fen();
                        self.send(&format!("info string illegal move {} in {}", s, fen));
                        return;
                    }
                }
            }
        }
    }

    fn find_move(&self, s: &str) -> Option<Move> {
        let mut buf = new_buffer();
        let n = self.board.gen_moves(&mut buf);
        buf[..n].iter().copied().find(|m| m.uci() == s)
    }

    /// Answers the current position from the tables: every root move is
    /// valued, sorted (shortest win, draw, unknown, longest loss) and the first
    /// MultiPV lines are printed with their complete lines to mate.
    fn search(&mut self) {
        let mut buf = new_buffer();
        let n = self.board.gen_moves(&mut buf);
        if n == 0 {
            self.send("bestmove 0000");
            return;
        }
        let root = self.board;
        self.load_material(&root);
        let mut moves: Vec<RootMove> = Vec::with_capacity(n);
        for &m in &buf[..n] {
            let mut child = root;
            child.do_move(m);
            self.load_material(&child);
            let mut r = RootMove { mv: m, value: Value::DRAW, known: false, pv: Vec::new() };
            if let Some(cv) = self.eval(&child, 1) {
                r.known = true;
                r.value = parent_value(cv);
                r.pv.push(m);
                let rest = self.line(child, cv);
                r.pv.extend(rest);
            }
            moves.push(r);
        }
        moves.sort_by_key(rank);
        if !moves[0].known {
            self.send(&format!("info string no endgame table for this material ({} pieces)", piece_count(&root)));
        }
        for (i, r) in moves.iter().enumerate().take(self.multi_pv) {
            if !r.known {
                let _ = writeln!(self.out, "info depth 1 multipv {} score cp 0 pv {}", i + 1, r.mv.uci());
                continue;
            }
            let pv: Vec<String> = r.pv.iter().map(|m| m.uci()).collect();
            let _ = writeln!(self.out, "info depth {} multipv {} score {} pv {}", r.pv.len(), i + 1, score(r.value), pv.join(" "));
        }
        let best = moves[0].mv.uci();
        self.send(&format!("bestmove {}", best));
    }

    /// Registers the material of a position beyond the base and loads its
    /// table from the cache directory if a file exists (never generates).
    fn load_material(&mut self, b: &Board) {
        if piece_count(b) <= 4 || b.castling != 0 {
            return;
        }
        let Some(m) = Material::of_board(b) else { return };
        let set = self.set.as_mut().expect("tables are loaded before searching");
        if set.find(&m.name()).is_some() {
            return;
        }
        let ti = set.add_material(m);
        let path = set.table_path(&set.tables[ti]);
        if !path.exists() {
            return;
        }
        match set.load_table(ti, &path) {
            Ok(()) => {
                let _ = writeln!(self.out, "info string egtb: loaded {}", path.display());
                let _ = self.out.flush();
            }
            Err(e) => self.send(&format!("info string {}: {}", path.display(), e)),
        }
    }

    /// Values a position from the side to move's view: from the table, or, for
    /// positions the tables leave out (an en passant right), from its children
    /// up to `depth` plies deep. None when no table covers it.
    fn eval(&self, b: &Board, depth: u32) -> Option<Value> {
        let set = self.set.as_ref().expect("tables are loaded before searching");
        if let Some(v) = set.lookup(b) {
            return Some(v);
        }
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        if n == 0 {
            return Some(if b.in_check() { Value::loss_in(0) } else { Value::DRAW });
        }
        if depth == 0 {
            return None;
        }
        let (mut min_loss, mut max_win, mut all_win) = (u32::MAX, 0u32, true);
        for &m in &buf[..n] {
            let mut child = *b;
            child.do_move(m);
            let cv = self.eval(&child, depth - 1)?;
            if cv.is_loss() {
                min_loss = min_loss.min(cv.plies());
                all_win = false;
            } else if cv.is_win() {
                max_win = max_win.max(cv.plies());
            } else {
                all_win = false;
            }
        }
        Some(if min_loss < u32::MAX {
            Value::win_in(min_loss + 1)
        } else if all_win {
            Value::loss_in(max_win + 1)
        } else {
            Value::DRAW
        })
    }

    /// Follows the optimal play from a position with value v: the winner takes
    /// a child lost in v-1, the loser a child won in v-1 (the longest defence).
    /// Draws give an empty line. Stops at mate or where the tables end.
    fn line(&mut self, mut b: Board, mut v: Value) -> Vec<Move> {
        let mut line = Vec::new();
        while (v.is_win() || v.is_loss()) && v.plies() > 0 && line.len() < MAX_PLIES as usize {
            let mut buf = new_buffer();
            let n = b.gen_moves(&mut buf);
            let mut found = None;
            for &m in &buf[..n] {
                let mut child = b;
                child.do_move(m);
                self.load_material(&child);
                let Some(cv) = self.eval(&child, 1) else { continue };
                if (v.is_win() && cv.is_loss() || v.is_loss() && cv.is_win()) && cv.plies() + 1 == v.plies() {
                    found = Some((m, cv));
                    break;
                }
            }
            let Some((best, best_val)) = found else { break };
            line.push(best);
            b.do_move(best);
            v = best_val;
        }
        line
    }
}

/// Orders root moves: wins by shortest distance, then draws, then moves
/// without a table, then losses by longest distance.
fn rank(r: &RootMove) -> u32 {
    if !r.known {
        2 << 10
    } else if r.value.is_win() {
        r.value.plies()
    } else if r.value.is_loss() {
        (3 << 10) - r.value.plies()
    } else {
        1 << 10
    }
}

/// Renders a value as UCI score from the root side's view.
fn score(v: Value) -> String {
    if v.is_win() {
        format!("mate {}", v.plies().div_ceil(2))
    } else if v.is_loss() {
        format!("mate -{}", v.plies() / 2)
    } else {
        "cp 0".to_string()
    }
}

/// Converts a child's value (its side to move) to the parent's.
fn parent_value(cv: Value) -> Value {
    if cv.is_loss() {
        Value::win_in(cv.plies() + 1)
    } else if cv.is_win() {
        Value::loss_in(cv.plies() + 1)
    } else {
        Value::DRAW
    }
}

fn piece_count(b: &Board) -> usize {
    b.squares.iter().filter(|&&p| p != crate::chess::Piece::NONE).count()
}
