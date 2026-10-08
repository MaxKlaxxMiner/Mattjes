//! The Universal Chess Interface (milestone 6, stages a and b), direct port of
//! `mattjesGo/uci`: positions covered by the endgame tables are answered
//! straight from them, with the exact distance to mate and the complete
//! winning line; every root move gets its own value, so MultiPV lists them
//! all. Positions without a table get a legal move and an "info string" for
//! now; the search algorithms come in a later stage.
//!
//! Tables beyond the four-piece base are loaded from the cache directory when
//! a file exists. When the options allow it, a missing table is generated in
//! the background, but only while a "go infinite" is running: "stop" pauses
//! the generation, the next "go infinite" resumes it, and once the table is
//! complete the held-back answer is recomputed with it. Commands come from a
//! reader thread and the finished generation from its own thread, both over
//! one channel, so the command loop never blocks on either.
//!
//! Protocol subset: uci, isready, setoption, ucinewgame, position, go (only
//! "infinite" is looked at, the answer is immediate), stop, quit, d (debug:
//! print the FEN).

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use crate::bitboard::Board;
use crate::chess::{new_buffer, Move, Piece};
use crate::egtb::{self, Control, Material, Set, Value, MAX_PLIES};

/// Reported in "id name"; it follows the milestone that is being worked on.
pub const VERSION: &str = "0.6";

/// The banner line printed at startup (unprompted, as most engines do).
pub const ABOUT: &str = "Mattjes 0.6 by Max Klaxx Miner, a mate and draw search engine with its own endgame tables, GPLv3";

/// Limits the per-level progress of a background generation to one info
/// string every few seconds (a table has up to 254 levels).
const PROGRESS_INTERVAL: Duration = Duration::from_secs(5);

enum Event {
    Line(String),
    Eof,
    Done(Set),
}

/// The state between commands.
pub struct Engine<W: Write + Send + 'static> {
    /// The generation thread reports through the same writer.
    out: Arc<Mutex<W>>,
    set: Option<Set>,
    board: Board,
    multi_pv: usize,
    workers: usize,
    name: String,
    // options
    write_cache: bool, // EgtbWriteCache: write generated tables to the cache directory
    generate5: bool,   // EgtbGenerate5: build missing five-piece tables during go infinite
    generate6: bool,   // EgtbGenerate6: the same for six pieces
    cache_dir: PathBuf, // EgtbPath
    // a "go infinite" holds the best move back until "stop"
    infinite: bool,
    pending_best: String,
    job: Option<Job>,
    tx: mpsc::Sender<Event>,
}

/// A background generation of one material (with its dependencies) in a
/// forked set.
struct Job {
    mat: Material,
    ctrl: Arc<Control>,
}

/// Reads commands from `input` until quit or EOF. `name` is the engine name
/// reported to the GUI (the Go and Rust binaries differ).
pub fn run<R: Read + Send + 'static, W: Write + Send + 'static>(input: R, out: W, name: &str, workers: usize) {
    let (tx, rx) = mpsc::channel();
    let mut e = Engine {
        out: Arc::new(Mutex::new(out)),
        set: None,
        board: Board::new(),
        multi_pv: 1,
        workers,
        name: name.to_string(),
        write_cache: false,
        generate5: false,
        generate6: false,
        cache_dir: egtb::default_cache_dir(),
        infinite: false,
        pending_best: String::new(),
        job: None,
        tx: tx.clone(),
    };
    e.send(ABOUT);
    e.flush();
    std::thread::spawn(move || {
        for line in BufReader::new(input).lines() {
            let Ok(line) = line else { break };
            if tx.send(Event::Line(line)).is_err() {
                return;
            }
        }
        let _ = tx.send(Event::Eof);
    });
    while let Ok(event) = rx.recv() {
        match event {
            Event::Line(line) => {
                let words: Vec<&str> = line.split_whitespace().collect();
                if !e.command(&words) {
                    break;
                }
            }
            Event::Eof => break,
            Event::Done(set) => e.finish_job(set),
        }
        e.flush();
    }
    if let Some(j) = &e.job {
        j.ctrl.abort();
    }
    e.flush();
}

/// One root move with its value from the side to move's view.
struct RootMove {
    mv: Move,
    value: Value,
    known: bool,
    pv: Vec<Move>,
}

impl<W: Write + Send + 'static> Engine<W> {
    fn send(&self, line: &str) {
        let mut o = self.out.lock().unwrap();
        let _ = writeln!(o, "{}", line);
    }

    fn flush(&self) {
        let _ = self.out.lock().unwrap().flush();
    }

    /// Handles one line; false means quit.
    fn command(&mut self, words: &[&str]) -> bool {
        let Some(&cmd) = words.first() else { return true };
        match cmd {
            "uci" => {
                self.send(&format!("id name {}", self.name));
                self.send("id author Max Klaxx Miner");
                self.send("option name MultiPV type spin default 1 min 1 max 256");
                self.send(&format!("option name Threads type spin default {} min 1 max 64", self.workers));
                self.send("option name EgtbWriteCache type check default false");
                self.send("option name EgtbGenerate5 type check default false");
                self.send("option name EgtbGenerate6 type check default false");
                self.send(&format!("option name EgtbPath type string default {}", self.cache_dir.display()));
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
                self.infinite = words.contains(&"infinite");
                self.search();
                if self.infinite {
                    self.maybe_generate();
                } else {
                    let best = self.pending_best.clone();
                    self.send(&format!("bestmove {}", best));
                }
            }
            "stop" => {
                if let Some(j) = &self.job {
                    if !j.ctrl.paused() {
                        j.ctrl.pause();
                        let name = j.mat.name();
                        self.send(&format!("info string egtb: generation of {} paused, the next go infinite resumes it", name));
                    }
                }
                if self.infinite {
                    let best = self.pending_best.clone();
                    self.send(&format!("bestmove {}", best));
                    self.infinite = false;
                }
            }
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
        // setoption name <id> [value <x>]; the value may contain spaces (a path)
        let (mut name, mut value) = ("", String::new());
        let mut i = 0;
        while i < words.len() {
            match words[i] {
                "name" => name = words.get(i + 1).copied().unwrap_or(""),
                "value" => {
                    value = words[i + 1..].join(" ");
                    break;
                }
                _ => {}
            }
            i += 1;
        }
        let number = value.parse::<usize>();
        let flag = value.eq_ignore_ascii_case("true");
        if name.eq_ignore_ascii_case("MultiPV") && number.is_ok() {
            self.multi_pv = number.unwrap_or(1).max(1);
        } else if name.eq_ignore_ascii_case("Threads") && number.is_ok() {
            self.workers = number.unwrap_or(1).max(1);
        } else if name.eq_ignore_ascii_case("EgtbWriteCache") {
            self.write_cache = flag;
            if flag && self.set.is_some() {
                self.save_tables();
            }
        } else if name.eq_ignore_ascii_case("EgtbGenerate5") {
            self.generate5 = flag;
        } else if name.eq_ignore_ascii_case("EgtbGenerate6") {
            self.generate6 = flag;
        } else if name.eq_ignore_ascii_case("EgtbPath") {
            self.cache_dir = PathBuf::from(&value);
            if let Some(set) = self.set.as_mut() {
                set.cache_dir = self.cache_dir.clone();
            }
        } else {
            self.send(&format!("info string unknown option {} or bad value {:?}", name, value));
        }
    }

    /// Loads the four-piece base once (generating it on the first start, which
    /// takes about 17 s and is reported as info strings; written to the cache
    /// only with EgtbWriteCache).
    fn ensure_tables(&mut self) {
        if self.set.is_some() {
            return;
        }
        let out = self.out.clone();
        let path = self.cache_dir.join(egtb::BASE_FILE_NAME);
        let set = Set::load_or_generate(&path, self.workers, self.write_cache, &mut |line| {
            let mut o = out.lock().unwrap();
            let _ = writeln!(o, "info string {}", compact(line));
            let _ = o.flush();
        });
        self.set = Some(set);
    }

    /// Writes every table that lives in RAM only (generated while
    /// EgtbWriteCache was off) to the cache directory, the base included.
    fn save_tables(&mut self) {
        let set = self.set.as_ref().expect("checked by the caller");
        let mut results = Vec::new();
        if set.base()[0].is_generated() {
            let base = set.base_path();
            if !base.exists() {
                results.push((base.clone(), set.save(&base)));
            }
        }
        for ti in set.base().len()..set.tables.len() {
            if set.tables[ti].is_generated() {
                let path = set.table_path(&set.tables[ti]);
                if !path.exists() {
                    results.push((path.clone(), set.save_table(ti, &path)));
                }
            }
        }
        for (path, result) in results {
            match result {
                Ok(()) => self.send(&format!("info string egtb: written to {}", path.display())),
                Err(e) => self.send(&format!("info string egtb: {}", e)),
            }
            self.flush();
        }
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
    /// MultiPV lines are printed with their complete lines to mate. The best
    /// move is kept in `pending_best`; the caller decides when to send it.
    fn search(&mut self) {
        let mut buf = new_buffer();
        let n = self.board.gen_moves(&mut buf);
        if n == 0 {
            self.pending_best = "0000".to_string();
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
                self.send(&format!("info depth 1 multipv {} score cp 0 pv {}", i + 1, r.mv.uci()));
                continue;
            }
            let pv: Vec<String> = r.pv.iter().map(|m| m.uci()).collect();
            self.send(&format!("info depth {} multipv {} score {} pv {}", r.pv.len(), i + 1, score(r.value), pv.join(" ")));
        }
        self.pending_best = moves[0].mv.uci();
    }

    /// Starts (or resumes) the background generation of the root material's
    /// table during a "go infinite", when the options allow it.
    fn maybe_generate(&mut self) {
        let Some(m) = Material::of_board(&self.board) else { return };
        if m.pieces() <= 4 || self.board.castling != 0 {
            return;
        }
        let set = self.set.as_ref().expect("tables are loaded before searching");
        if let Some(i) = set.find(&m.name()) {
            if set.tables[i].is_generated() {
                return;
            }
        }
        if m.pieces() == 5 && !self.generate5 || m.pieces() == 6 && !self.generate6 {
            self.send(&format!("info string egtb: no table for {} (EgtbGenerate{} would build it during go infinite)", m.name(), m.pieces()));
            return;
        }
        if let Some(j) = &self.job {
            if j.mat.name() == m.name() {
                if j.ctrl.paused() {
                    j.ctrl.resume();
                    self.send(&format!("info string egtb: generation of {} resumed", m.name()));
                }
                return;
            }
            j.ctrl.abort();
            let name = j.mat.name();
            self.send(&format!("info string egtb: generation of {} dropped", name));
            self.job = None;
        }
        self.start_job(m);
    }

    fn start_job(&mut self, m: Material) {
        let ctrl = Arc::new(Control::new());
        self.job = Some(Job { mat: m.clone(), ctrl: ctrl.clone() });
        let mut fork = self.set.as_ref().expect("tables are loaded before searching").fork();
        let ti = fork.add_material(m.clone());
        let (size, bytes) = (fork.tables[ti].size, fork.generation_bytes(ti));
        self.send(&format!("info string egtb: generating {} ({} MB table, about {} MB RAM), stop pauses it", m.name(), size >> 20, bytes >> 20));
        let (out, tx, workers, write) = (self.out.clone(), self.tx.clone(), self.workers, self.write_cache);
        std::thread::spawn(move || {
            // level lines come up to 254 times per table: at most one every few seconds
            let mut last_level: Option<Instant> = None;
            let mut progress = |line: &str| {
                if line.starts_with(' ') {
                    if last_level.is_some_and(|t| t.elapsed() < PROGRESS_INTERVAL) {
                        return;
                    }
                    last_level = Some(Instant::now());
                }
                let mut o = out.lock().unwrap();
                let _ = writeln!(o, "info string egtb: {}", compact(line));
                let _ = o.flush();
            };
            let st = fork.generate_controlled(ti, workers, Some(&ctrl), &mut progress);
            if st.aborted {
                return;
            }
            for gi in fork.base().len()..fork.tables.len() {
                if !fork.tables[gi].is_generated() {
                    continue;
                }
                if write {
                    let path = fork.table_path(&fork.tables[gi]);
                    if !path.exists() {
                        match fork.save_table(gi, &path) {
                            Ok(()) => progress(&format!("written to {}", path.display())),
                            Err(e) => progress(&e.to_string()),
                        }
                    }
                }
                fork.tables[gi].fill(); // drops the generator's bitset
            }
            let _ = tx.send(Event::Done(fork));
        });
    }

    /// Takes the generated tables over and, during a "go infinite", answers
    /// again with them.
    fn finish_job(&mut self, set: Set) {
        let Some(j) = self.job.take() else { return };
        let name = j.mat.name();
        self.set.as_mut().expect("tables are loaded before a job runs").adopt(set);
        self.send(&format!("info string egtb: {} ready", name));
        if self.infinite {
            self.search();
        }
    }

    /// Registers the material of a position beyond the base and loads its
    /// table from the cache directory if a file exists (never generates here).
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
                let mut o = self.out.lock().unwrap();
                let _ = writeln!(o, "info string egtb: loaded {}", path.display());
                let _ = o.flush();
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

/// Joins the words of a console status line with single spaces: the column
/// alignment of the generator is noise in a GUI's proportional font.
fn compact(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn piece_count(b: &Board) -> usize {
    b.squares.iter().filter(|&&p| p != Piece::NONE).count()
}
