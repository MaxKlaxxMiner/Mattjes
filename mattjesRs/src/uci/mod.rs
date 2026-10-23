//! The Universal Chess Interface (milestone 6), direct port of
//! `mattjesGo/uci`: positions covered by the endgame tables are answered
//! straight from them, with the exact distance to mate and the complete
//! winning line; every root move gets its own value, so MultiPV lists them
//! all. Positions the tables do not settle go to one of the mate searches
//! (option Search: mateab, matepn, matelist) in its own thread, with the
//! tables as oracle (search.rs).
//!
//! Tables beyond the four-piece base are loaded from the cache directory when
//! a file exists. When the options allow it, a missing table is generated in
//! the background, but only while a "go infinite" is running: "stop" pauses
//! the generation, the next "go infinite" resumes it, and once the table is
//! complete the held-back answer is recomputed with it. Commands come from a
//! reader thread, the finished generation and the finished search from their
//! own threads, all over one channel, so the command loop never blocks on
//! any of them.
//!
//! Ownership: the search thread needs the tables as oracle, so the `Set` is
//! moved into it for the duration of the search and comes back with the
//! result (the Go version shares the pointer and simply does not mutate the
//! set meanwhile). While it is away, `set` is `None` and `searching` is true.
//!
//! Protocol subset: uci, isready, setoption, ucinewgame, position, go
//! (infinite, movetime, wtime/btime/winc/binc, mate, depth), stop, quit, d
//! (debug: print the FEN).

mod search;
mod sweep;
mod wdl;

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use crate::bitboard::Board;
use crate::chess::{new_buffer, Move, Piece};
use crate::egtb::{self, Control, Material, Set, Value, MAX_PLIES};
use crate::mateab::Verdict;
use search::{GoParams, SearchEnd, SearchTable, DEFAULT_ALGO, DEFAULT_GOAL, DEFAULT_HASH_MB, DEFAULT_MOBILITY, MAX_SEARCH_PLIES, MIN_HASH_MB};

/// Reported in "id name"; it follows the milestone that is being worked on.
pub const VERSION: &str = "0.6";

/// The banner line printed at startup (unprompted, as most engines do).
pub const ABOUT: &str = "Mattjes 0.6 by Max Klaxx Miner, a mate and draw search engine with its own endgame tables, GPLv3";

/// Limits the per-level progress of a background generation and the node
/// counts of a running search to one line every few seconds.
const PROGRESS_INTERVAL: Duration = Duration::from_secs(5);

enum Event {
    Line(String),
    Eof,
    Done(Set),
    /// The search thread has put its result into the search channel.
    Searched,
    /// The search's reporter knocks every few seconds: time to look at the
    /// demand for tables.
    Tick,
}

/// The state between commands.
pub struct Engine<W: Write + Send + 'static> {
    /// The generation and search threads report through the same writer.
    out: Arc<Mutex<W>>,
    set: Option<Set>,
    board: Board,
    multi_pv: usize,
    workers: usize,
    name: String,
    // options
    write_cache: bool, // EgtbWriteCache: write generated tables to the cache directory
    force_gen: bool,   // EgtbForceGen: go infinite generates every material the options allow, one after the other (sweep.rs)
    generate5: bool,   // EgtbGenerate5: build missing five-piece tables during go infinite
    generate6: bool,   // EgtbGenerate6: the same for six pieces
    cache_dir: PathBuf, // EgtbPath
    syzygy: Option<Arc<crate::syzygy::Tables>>, // the WDL tables of the search oracle (SyzygyPath plus our own files), shared with the search thread
    syzygy_path: String,
    algo: String,      // Search: the algorithm for positions the tables do not settle
    hash_mb: usize,    // Hash: the transposition table of the search
    mobility: String,  // MatePnMobility: on (leaves start with their move count), off (1/1), lazy (move count when entered)
    goal: String,      // MatePnGoal: mate (the shortest mate, depth by depth) or win (any won table position proves, the mate length is an upper bound)
    /// The search's transposition table, allocated on the first search,
    /// dropped when a generation starts (the generator needs the memory).
    tables: Option<SearchTable>,
    table_mb: usize,
    /// The algorithm whose entries the table holds (their value layouts differ).
    table_algo: String,
    // a "go infinite" holds the best move back until "stop"
    infinite: bool,
    pending_best: String,
    /// `pending_best` has a table value (a search without result keeps it).
    table_best: bool,
    /// The root moves the table stage knows exactly, handed to the search's root driver.
    root_table: Vec<RootMove>,
    /// The parameters of the last "go": a search is repeated with them when a
    /// generation finishes beside it.
    last_go: GoParams,
    /// When the running timed search ends (the repeat gets the rest).
    deadline: Option<Instant>,
    /// The running search is stopped to be repeated: no result lines, no bestmove.
    restart: bool,
    /// A fork taken before a search for a generation that the search's demand
    /// starts meanwhile (None = none wanted).
    spare: Option<Set>,
    /// The demand counters of the set away in the search thread.
    demand: Option<Arc<Mutex<std::collections::HashMap<egtb::Signature, u64>>>>,
    job: Option<Job>,
    tx: mpsc::Sender<Event>,
    // the running search, if any
    searching: bool,
    stop: Option<Arc<AtomicBool>>,
    search_tx: mpsc::Sender<SearchEnd>,
    search_rx: mpsc::Receiver<SearchEnd>,
    /// EgtbWriteCache was switched on while the set was away in a search.
    save_pending: bool,
}

/// A background generation of one material (with its dependencies) in a
/// forked set.
struct Job {
    mat: Material,
    ctrl: Arc<Control>,
    sweep: bool, // EgtbForceGen: every material the options allow, one after the other (sweep.rs); mat is the first
}

/// Reads commands from `input` until quit or EOF. `name` is the engine name
/// reported to the GUI (the Go and Rust binaries differ).
pub fn run<R: Read + Send + 'static, W: Write + Send + 'static>(input: R, out: W, name: &str, workers: usize) {
    let (tx, rx) = mpsc::channel();
    let (search_tx, search_rx) = mpsc::channel();
    let mut e = Engine {
        out: Arc::new(Mutex::new(out)),
        set: None,
        board: Board::new(),
        multi_pv: 1,
        workers,
        name: name.to_string(),
        write_cache: false,
        force_gen: false,
        generate5: false,
        generate6: false,
        cache_dir: egtb::default_cache_dir(),
        syzygy: None,
        syzygy_path: String::new(),
        algo: DEFAULT_ALGO.to_string(),
        hash_mb: DEFAULT_HASH_MB,
        mobility: DEFAULT_MOBILITY.to_string(),
        goal: DEFAULT_GOAL.to_string(),
        tables: None,
        table_mb: 0,
        table_algo: String::new(),
        infinite: false,
        pending_best: String::new(),
        table_best: false,
        root_table: Vec::new(),
        last_go: GoParams { infinite: false, max_plies: MAX_SEARCH_PLIES, budget: None },
        deadline: None,
        restart: false,
        spare: None,
        demand: None,
        job: None,
        tx: tx.clone(),
        searching: false,
        stop: None,
        search_tx,
        search_rx,
        save_pending: false,
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
            Event::Searched => {
                // nothing to take when stop_search already waited for the result
                if let Ok(r) = e.search_rx.try_recv() {
                    e.finish_search(r);
                }
            }
            Event::Tick => e.on_demand_tick(),
        }
        e.flush();
    }
    if let Some(j) = &e.job {
        j.ctrl.abort();
    }
    if let Some(s) = &e.stop {
        s.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    e.flush();
}

/// One root move with its value from the side to move's view.
#[derive(Clone)]
struct RootMove {
    mv: Move,
    value: Value,
    /// The own tables know the distance (value and pv).
    known: bool,
    pv: Vec<Move>,
    /// Set when only the Syzygy WDL tables know the move (the outcome from
    /// the root side's view, no distance, no line).
    wdl: Verdict,
}

impl RootMove {
    /// The move is known by the WDL tables only (no distance).
    fn wdl_only(&self) -> bool {
        !self.known && self.wdl != Verdict::Unknown
    }
}

/// Renders a WDL-only outcome as the WDL search does.
fn wdl_score(v: Verdict) -> &'static str {
    match v {
        Verdict::Win => "cp 9999",
        Verdict::Loss => "cp -9999",
        _ => "cp 0",
    }
}

impl<W: Write + Send + 'static> Engine<W> {
    fn send(&self, line: &str) {
        let mut o = self.out.lock().unwrap();
        let _ = writeln!(o, "{}", line);
    }

    fn flush(&self) {
        let _ = self.out.lock().unwrap().flush();
    }

    fn set(&self) -> &Set {
        self.set.as_ref().expect("tables are loaded and not away in a search")
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
                self.send(&format!("option name Hash type spin default {} min {} max 65536", DEFAULT_HASH_MB, MIN_HASH_MB));
                self.send(&format!("option name Search type combo default {} var none var mateab var matepn var matelist var fastwdl", DEFAULT_ALGO));
                self.send(&format!("option name MatePnMobility type combo default {} var on var off var lazy", DEFAULT_MOBILITY));
                self.send(&format!("option name MatePnGoal type combo default {} var mate var win", DEFAULT_GOAL));
                self.send("option name EgtbWriteCache type check default false");
                self.send("option name EgtbGenerate5 type check default false");
                self.send("option name EgtbGenerate6 type check default false");
                self.send("option name EgtbForceGen type check default false");
                self.send(&format!("option name EgtbPath type string default {}", self.cache_dir.display()));
                self.send("option name SyzygyPath type string default <empty>");
                self.send("uciok");
            }
            // Winboard probes from a GUI's auto-detection: stay silent, so that only UCI answers are seen
            "xboard" | "protover" | "new" | "force" => {}
            "isready" => {
                self.ensure_tables();
                self.send("readyok");
            }
            "setoption" => self.set_option(&words[1..]),
            "ucinewgame" => {
                self.clear_tables();
                if let Some(s) = &self.set {
                    s.reset_missing(); // the demand for tables belongs to the old game
                }
            }
            "position" => self.position(&words[1..]),
            "go" => {
                self.ensure_tables();
                self.stop_search(); // a GUI sends stop first; if not, the old search ends here
                let p = self.parse_go(&words[1..]);
                self.infinite = p.infinite;
                self.last_go = p;
                let settled = self.search();
                self.spare = None;
                if self.force_gen && p.infinite && (self.generate5 || self.generate6) && self.start_sweep() {
                    // EgtbForceGen: go infinite runs the sweep over every
                    // material the options allow instead of a search (every
                    // thread and the hash table's memory are its), stop pauses
                    // it, the next go infinite resumes it; with every file in
                    // place the go runs as usual
                    return true;
                }
                // a missing table of the root material is generated in the
                // background beside the search (every "go", not only infinite);
                // when it is ready, finish_job answers again with it. A position
                // the tables settle needs none (a paused job stays paused)
                if !settled && !self.maybe_generate() && (self.generate5 || self.generate6) && piece_count(&self.board) > 4 {
                    // nothing to generate yet: the search's demand may name a
                    // table meanwhile (on_demand_tick), and that job needs a
                    // copy of the set taken while the main loop still owns it
                    self.spare = Some(self.set().fork());
                }
                if !settled && self.algo != "none" {
                    self.start_search(p);
                    return true;
                }
                if !self.infinite {
                    let best = self.pending_best.clone();
                    self.send(&format!("bestmove {}", best));
                }
            }
            "stop" => {
                let mut sweep = false;
                if let Some(j) = &self.job {
                    sweep = j.sweep;
                    if !j.ctrl.paused() {
                        j.ctrl.pause();
                        if j.sweep {
                            self.send("info string egtb: sweep paused, the next go infinite with EgtbForceGen resumes it");
                        } else {
                            let name = j.mat.name();
                            self.send(&format!("info string egtb: generation of {} paused, the next go resumes it", name));
                        }
                    }
                }
                self.stop_search();
                if sweep && self.set.is_some() {
                    self.attach_wdl(); // the files written so far serve the searches meanwhile (after the search stopped: it reads the tables)
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
        } else if name.eq_ignore_ascii_case("Hash") && number.is_ok() {
            self.hash_mb = number.unwrap_or(MIN_HASH_MB).max(MIN_HASH_MB); // applied by the next search
        } else if name.eq_ignore_ascii_case("Search") && search::is_algo(&value) {
            self.algo = value.to_ascii_lowercase();
        } else if name.eq_ignore_ascii_case("MatePnMobility") && search::is_mobility(&value) {
            self.mobility = value.to_ascii_lowercase();
        } else if name.eq_ignore_ascii_case("MatePnGoal") && search::is_goal(&value) {
            self.goal = value.to_ascii_lowercase(); // the next search clears the table when the goal changed (table key)
        } else if name.eq_ignore_ascii_case("EgtbWriteCache") {
            self.write_cache = flag;
            if flag {
                if self.set.is_some() {
                    self.save_tables();
                } else if self.searching {
                    self.save_pending = true;
                }
            }
        } else if name.eq_ignore_ascii_case("EgtbGenerate5") {
            self.generate5 = flag;
        } else if name.eq_ignore_ascii_case("EgtbGenerate6") {
            self.generate6 = flag;
        } else if name.eq_ignore_ascii_case("EgtbForceGen") {
            self.force_gen = flag;
        } else if name.eq_ignore_ascii_case("EgtbPath") {
            self.cache_dir = PathBuf::from(&value);
            if let Some(set) = self.set.as_mut() {
                set.cache_dir = self.cache_dir.clone();
                self.attach_wdl();
            }
        } else if name.eq_ignore_ascii_case("SyzygyPath") {
            // the WDL tables become part of the search oracle (Syzygy answers
            // win, draw or loss without a distance, the fifty-move rule is
            // ignored); the files are mapped on first use
            self.syzygy_path = value;
            self.attach_wdl();
        } else {
            self.send(&format!("info string unknown option {} or bad value {:?}", name, value));
        }
    }

    /// Loads the four-piece base once (generating it on the first start, which
    /// takes about 17 s and is reported as info strings; written to the cache
    /// only with EgtbWriteCache).
    fn ensure_tables(&mut self) {
        if self.set.is_some() || self.searching {
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
        self.attach_wdl();
    }

    /// Builds the WDL tables of the search oracle: the Syzygy files of
    /// SyzygyPath, when set, and our own files in the cache folder for every
    /// other material (the originals win); the base answers captures into
    /// materials without a file (probe_wdl_with). Called once the base is
    /// loaded and whenever SyzygyPath or the cache folder changes, and after
    /// a job wrote new files.
    fn attach_wdl(&mut self) {
        let Some(set) = self.set.as_ref() else { return }; // before the base is loaded: ensure_tables calls again
        let mut tb = None;
        if !self.syzygy_path.is_empty() && self.syzygy_path != "<empty>" {
            match crate::syzygy::Tables::open(std::path::Path::new(&self.syzygy_path)) {
                Ok(t) => {
                    self.send(&format!("info string syzygy: {} WDL tables in {}, up to {} pieces", t.count(), self.syzygy_path, t.largest()));
                    tb = Some(t);
                }
                Err(e) => self.send(&format!("info string syzygy: {}", e)),
            }
        }
        let mut tb = tb.unwrap_or_else(crate::syzygy::Tables::new);
        if let Ok(n) = tb.add_own(&set.wdl_path()) {
            if n > 0 {
                self.send(&format!("info string wdl: {} own tables in {}", n, set.wdl_path().display()));
            }
        }
        if let Ok(n) = tb.add_dtm(&set.dtm_path()) {
            if n > 0 {
                self.send(&format!("info string dtm: {} own tables in {} (step {}, the search resolves exact distances)", n, set.dtm_path().display(), egtb::FILE_STEP));
            }
        }
        self.syzygy = Some(Arc::new(tb));
    }

    /// Writes every table that lives in RAM only (generated while
    /// EgtbWriteCache was off) to the cache directory, the base included.
    fn save_tables(&mut self) {
        let set = self.set();
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
    /// Returns whether the tables settle the position: no moves, or every
    /// move known. Otherwise the search is asked, with the known moves as the
    /// root driver's starting point (a known win alone proves nothing about
    /// the other moves: a quiet move could mate faster).
    fn search(&mut self) -> bool {
        let mut buf = new_buffer();
        let n = self.board.gen_moves(&mut buf);
        if n == 0 {
            self.pending_best = "0000".to_string();
            return true;
        }
        let root = self.board;
        self.load_material(&root);
        if self.algo != "none" {
            self.load_dependencies(&root);
        }
        let mut moves: Vec<RootMove> = Vec::with_capacity(n);
        let mut all_known = true;
        for &m in &buf[..n] {
            let mut child = root;
            child.do_move(m);
            self.load_material(&child);
            let mut r = RootMove { mv: m, value: Value::DRAW, known: false, pv: Vec::new(), wdl: Verdict::Unknown };
            if let Some(cv) = self.eval(&child, 1) {
                r.known = true;
                r.value = parent_value(cv);
                r.pv.push(m);
                let rest = self.line(child, cv);
                r.pv.extend(rest);
            } else {
                // the Syzygy WDL tables know the outcome without a distance
                // (v is for the opponent, to move in the child): the WDL
                // search has nothing left to do for such a move, the mate
                // searches skip a known draw or loss and still search a
                // known win for its distance
                if let Some(v) = self.syzygy.as_ref().and_then(|t| t.probe_wdl_with(&child, self.set.as_ref())) {
                    r.wdl = if v >= crate::syzygy::CURSED_WIN {
                        Verdict::Loss
                    } else if v <= crate::syzygy::BLESSED_LOSS {
                        Verdict::Win
                    } else {
                        Verdict::Draw
                    };
                }
                if !(r.wdl != Verdict::Unknown && (self.algo == "fastwdl" || r.wdl != Verdict::Win)) {
                    all_known = false;
                }
            }
            moves.push(r);
        }
        moves.sort_by_key(rank);
        if !moves[0].known && !moves[0].wdl_only() {
            self.send(&format!("info string no endgame table for this material ({} pieces)", piece_count(&root)));
        }
        // one depth for the whole block (the longest line), as the search's
        // blocks have it: GUIs order MultiPV lines by depth or drop the ones
        // that look older, so a draw with its one-move line must not say depth 1
        let depth = moves.iter().take(self.multi_pv).map(|r| r.pv.len()).max().unwrap_or(1).max(1);
        for (i, r) in moves.iter().enumerate().take(self.multi_pv) {
            if r.wdl_only() {
                self.send(&format!("info depth {} multipv {} score {} pv {}", depth, i + 1, wdl_score(r.wdl), r.mv.uci()));
                continue;
            }
            if !r.known {
                self.send(&format!("info depth {} multipv {} score cp 0 pv {}", depth, i + 1, r.mv.uci()));
                continue;
            }
            let pv: Vec<String> = r.pv.iter().map(|m| m.uci()).collect();
            self.send(&format!("info depth {} multipv {} score {} pv {}", depth, i + 1, score(r.value), pv.join(" ")));
        }
        self.pending_best = moves[0].mv.uci();
        self.table_best = moves[0].known || moves[0].wdl_only();
        self.root_table = moves.iter().filter(|r| r.known || r.wdl_only()).cloned().collect();
        all_known
    }

    /// Starts (or resumes) the background generation of the root material's
    /// table on every "go", when the options allow it; it runs beside the
    /// search with `background_workers` threads and `finish_job` answers again
    /// when it is ready. Returns whether a generation is running afterwards.
    fn maybe_generate(&mut self) -> bool {
        if self.job.as_ref().is_some_and(|j| j.sweep) {
            return true; // a paused sweep stays as it is (its files serve the searches), nothing else starts
        }
        let Some(m) = self.generation_target() else {
            return false; // nothing missing that the options allow: the search runs alone, quietly
        };
        if let Some(j) = &self.job {
            if j.mat.name() == m.name() {
                if j.ctrl.paused() {
                    j.ctrl.resume();
                    self.send(&format!("info string egtb: generation of {} resumed", m.name()));
                }
                return true;
            }
            j.ctrl.abort();
            let name = j.mat.name();
            self.send(&format!("info string egtb: generation of {} dropped", name));
            self.job = None;
        }
        self.start_job(m)
    }

    /// Picks the material worth generating: the root material when its table
    /// is missing and the options allow its piece count, else the material
    /// without a table the searches asked for most (`Set::missing_tables`,
    /// "Erzeugung nach Bedarf" in docs/m6-uci.md: a six-piece root without
    /// EgtbGenerate6 still gets the five-piece tables its captures lead into).
    fn generation_target(&self) -> Option<Material> {
        let missing = |m: &Material| -> bool {
            if m.pieces() <= 4 || !(m.pieces() == 5 && self.generate5 || m.pieces() == 6 && self.generate6) {
                return false;
            }
            let no_table = match self.set().find(&m.name()) {
                Some(i) => !self.set().tables[i].is_available(),
                None => true,
            };
            no_table && !self.has_product(m)
        };
        if let Some(m) = Material::of_board(&self.board) {
            if self.board.castling == 0 && missing(&m) {
                return Some(m);
            }
        }
        for (m, probes) in self.set().missing_tables(0) {
            if missing(&m) {
                self.send(&format!("info string egtb: the search asked {} times for {}, generating it", egtb::group(probes as usize), m.name()));
                return Some(m);
            }
        }
        None
    }

    /// Runs in the main loop every few seconds of a search (the reporter
    /// knocks): when no generation runs and a spare copy of the set is at
    /// hand, the material the search asked for most, within the options'
    /// piece limits, is generated beside it ("Erzeugung nach Bedarf").
    fn on_demand_tick(&mut self) {
        if !self.searching || self.restart || self.job.is_some() || self.spare.is_none() {
            return;
        }
        let Some(demand) = self.demand.clone() else { return };
        let list = egtb::missing_tables_of(&demand.lock().unwrap(), 0);
        for (m, probes) in list {
            if (m.pieces() == 5 && self.generate5 || m.pieces() == 6 && self.generate6) && !self.has_product(&m) {
                self.send(&format!("info string egtb: the search asked {} times for {}, generating it", egtb::group(probes as usize), m.name()));
                let fork = self.spare.take().expect("checked above");
                self.start_job_from(m, fork);
                return;
            }
        }
    }

    fn start_job(&mut self, m: Material) -> bool {
        let fork = self.set().fork();
        self.start_job_from(m, fork)
    }

    /// Generates m in the background on `fork`, a copy of the loaded tables
    /// (taken by the caller while the main loop owned them).
    fn start_job_from(&mut self, m: Material, mut fork: Set) -> bool {
        // decided: the next demand decision looks at the rest
        match (&self.demand, &self.set) {
            (Some(d), _) => {
                d.lock().unwrap().remove(&m.signature());
            }
            (None, Some(s)) => s.forget_missing(&m),
            _ => {}
        }
        let ti = fork.add_material(m.clone());
        let size = fork.tables[ti].size;
        let (need, measured) = fork.estimate_bytes(ti);
        let how = if measured { "measured" } else { "estimated" };
        match egtb::available_memory() {
            Some((total, avail)) => {
                if need as u64 > total {
                    self.send(&format!("info string egtb: {} needs about {} ({}), the machine has {}: not started", m.name(), egtb::format_bytes(need as u64), how, egtb::format_bytes(total)));
                    return false;
                }
                self.send(&format!(
                    "info string egtb: generating {} ({} MB table, about {} RAM {}, {} of {} free), stop pauses it",
                    m.name(),
                    size >> 20,
                    egtb::format_bytes(need as u64),
                    how,
                    egtb::format_bytes(avail),
                    egtb::format_bytes(total)
                ));
                if need as u64 > avail {
                    self.send("info string egtb: WARNING: more than the free memory, expect swapping");
                }
            }
            None => self.send(&format!("info string egtb: generating {} ({} MB table, about {} RAM {}), stop pauses it", m.name(), size >> 20, egtb::format_bytes(need as u64), how)),
        }
        // the search keeps its hash table unless the generation would not fit
        // beside it (the estimate against the free memory, which the table
        // already took from)
        if let Some((_, avail)) = egtb::available_memory() {
            if need as u64 > avail && self.table_mb > 0 {
                self.send(&format!("info string egtb: the search's hash table ({} MB) is released for the generation", self.table_mb));
                self.drop_tables();
            }
        }
        let ctrl = Arc::new(Control::new());
        self.job = Some(Job { mat: m.clone(), ctrl: ctrl.clone(), sweep: false });
        let workers = self.background_workers();
        self.send(&format!("info string egtb: {} workers beside the search thread ({} cores)", workers, egtb::physical_cores()));
        let (out, tx, write) = (self.out.clone(), self.tx.clone(), self.write_cache);
        let syz = self.syzygy.clone(); // the job thread only asks it which materials the Syzygy files cover
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
            // every computed table gets its log line; a dependency the job
            // had to compute (or found as an exact file) is written right
            // away, both exactly (raw_path) and quantised, so that a job
            // interrupted before the requested table loses nothing and a
            // later generation loads the exact file instead of computing it
            // again (the quantised file cannot serve as a dependency); the
            // requested table is written below
            let out_done = out.clone();
            let mut done = move |set: &mut Set, gi: usize, st: &egtb::Stats| {
                let say = |line: &str| {
                    let mut o = out_done.lock().unwrap();
                    let _ = writeln!(o, "info string egtb: {}", line.strip_prefix("egtb: ").unwrap_or(line));
                    let _ = o.flush();
                };
                if !st.loaded {
                    if let Err(e) = egtb::append_log(&set.cache_dir, &set.log_line(gi, st, workers, "uci")) {
                        say(&e.to_string());
                    }
                }
                if write {
                    // from the exact values, before anything quantises them: the
                    // WDL file, then the distance buckets (which replace the quantised .bin)
                    write_wdl_file(set, gi, syz.as_deref(), workers, &say);
                    write_dtm_file(set, gi, workers, &say);
                }
                if gi == ti || !write {
                    return;
                }
                if let Err(e) = set.save_both(gi, &mut |line| say(line)) {
                    say(&e.to_string());
                }
            };
            let st = fork.generate_notify(ti, workers, Some(&ctrl), &mut progress, Some(&mut done));
            if st.aborted {
                return;
            }
            for gi in fork.base().len()..fork.tables.len() {
                if !fork.tables[gi].is_generated() {
                    continue;
                }
                // the quantised .bin is no longer written (2026-10-22): the WDL
                // and DTM files of the done callback replace it, existing .bin
                // files still load
                let _ = write;
                fork.tables[gi].fill(); // drops the generator's bitset
            }
            let _ = tx.send(Event::Done(fork));
        });
        true
    }

    /// Takes the generated tables over and, during a "go infinite", answers
    /// again with them (searching on if the tables still do not settle the
    /// position).
    /// Takes the generated tables over and, when a search was running beside
    /// the generation (or a "go infinite" is open), answers again with them:
    /// the table stage, and the search once more with the rest of the time
    /// budget where the tables still do not settle the position. The running
    /// search has to stop first, because it owns the set; its partial result
    /// is discarded.
    fn finish_job(&mut self, set: Set) {
        let Some(j) = self.job.take() else { return };
        let again = self.searching || self.infinite;
        self.restart = self.searching;
        self.stop_search();
        self.restart = false;
        let name = if j.sweep { String::from("sweep") } else { j.mat.name() };
        self.set.as_mut().expect("tables are loaded before a job runs").adopt(set);
        self.attach_wdl(); // the files the job wrote
        match egtb::peak_memory() {
            Some((committed, working)) => self.send(&format!("info string egtb: {} ready, peak memory {} committed / {} working set", name, egtb::format_bytes(committed), egtb::format_bytes(working))),
            None => self.send(&format!("info string egtb: {} ready", name)),
        }
        if !again {
            return;
        }
        // the next table the searches asked for, if any, starts right away
        // (the restarted search pauses it when the new table answers everything)
        self.maybe_generate();
        if !self.search() && self.algo != "none" {
            let mut p = self.last_go;
            if !p.infinite && p.budget.is_some() {
                let rest = self.deadline.map_or(Duration::ZERO, |d| d.saturating_duration_since(Instant::now()));
                p.budget = Some(rest.max(Duration::from_millis(50)));
            }
            self.start_search(p);
        } else if !self.infinite {
            let best = self.pending_best.clone();
            self.send(&format!("bestmove {}", best));
        }
    }

    /// The thread count of a generation beside a running search: the search
    /// needs a core of its own, and an SMT sibling would share one with it, so
    /// physical cores minus one, bounded by Threads, at least one.
    fn background_workers(&self) -> usize {
        let mut cores = egtb::physical_cores();
        if cores == 0 {
            cores = self.workers;
        }
        self.workers.min(cores).saturating_sub(1).max(1)
    }

    /// Registers the material of a position beyond the base and loads its
    /// table from the cache directory if a file exists (never generates here).
    fn load_material(&mut self, b: &Board) {
        if piece_count(b) <= 4 || b.castling != 0 {
            return;
        }
        let Some(m) = Material::of_board(b) else { return };
        self.load_table(m);
    }

    /// Registers the material and loads its cache file, with one line per
    /// table loaded.
    fn load_table(&mut self, m: Material) {
        let (ti, loaded) = self.open_table(m);
        if !loaded {
            return;
        }
        let set = self.set.as_ref().expect("tables are loaded before searching");
        let t = &set.tables[ti];
        let path = set.table_path(t);
        let mut o = self.out.lock().unwrap();
        if t.is_exact() {
            let _ = writeln!(o, "info string egtb: loaded {}", path.display());
        } else {
            // cache files beyond the base hold buckets of FILE_STEP moves: the
            // table stage does not report them, the search resolves them
            let _ = writeln!(o, "info string egtb: loaded {} (step {}, the search resolves exact distances)", path.display(), t.step());
        }
        let _ = o.flush();
    }

    /// Registers the material (if new) and loads its cache file unless the
    /// table is already available or no file exists. A table registered
    /// earlier without a file is retried, the file may have appeared since.
    /// Returns the table index and whether a file was loaded now.
    fn open_table(&mut self, m: Material) -> (usize, bool) {
        let set = self.set.as_mut().expect("tables are loaded before searching");
        let ti = match set.find(&m.name()) {
            Some(ti) if set.tables[ti].is_available() => return (ti, false),
            Some(ti) => ti,
            None => set.add_material(m),
        };
        // the quantised file first, else the exact twin of an interrupted
        // job: with EgtbWriteCache the quantised file is made from it now
        // (loaded whole, written, dropped), otherwise it is read block by
        // block like any cache file, only with step 1
        let write_cache = self.write_cache;
        let set = self.set.as_mut().expect("tables are loaded before searching");
        let mut path = set.table_path(&set.tables[ti]);
        if !path.exists() {
            let raw = set.raw_path(&set.tables[ti]);
            if !raw.exists() {
                return (ti, false);
            }
            if write_cache {
                if let Err(e) = set.load_raw_table(ti, &raw) {
                    self.send(&format!("info string {}: {}", raw.display(), e));
                    return (ti, false);
                }
                if let Err(e) = set.save_table(ti, &path) {
                    self.send(&format!("info string {}: {}", path.display(), e));
                    return (ti, false);
                }
                self.send(&format!("info string egtb: written to {} (from {})", path.display(), raw.display()));
            } else {
                path = raw;
            }
        }
        let set = self.set.as_mut().expect("tables are loaded before searching");
        match set.load_table(ti, &path) {
            Ok(()) => (ti, true),
            Err(e) => {
                self.send(&format!("info string {}: {}", path.display(), e));
                (ti, false)
            }
        }
    }

    /// Loads every cached table beyond the base that a search below this
    /// position can reach by captures and promotions, whatever the size of
    /// the root: an eleven-piece pawn ending reaches the five-piece tables
    /// too. The candidates are the files in the cache directory (one listing,
    /// sorted by name), filtered by material, so an empty directory costs
    /// nothing and a table is loaded only when its file exists (header and
    /// block index only, the blocks follow on demand). One summary line
    /// instead of one per table. The search thread owns the set meanwhile and
    /// cannot load, so this happens before it starts.
    fn load_dependencies(&mut self, b: &Board) {
        let Some((white, black)) = Material::pieces_of(b) else { return };
        if 2 + white.len() + black.len() <= 4 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(self.set().dtm_path()) else { return };
        let mut names: Vec<String> = entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        let (mut loaded, mut step) = (0, 1);
        for name in names {
            let Some(m) = Set::parse_file_name(&name) else { continue };
            if !m.reachable_from(&white, &black) {
                continue;
            }
            let (ti, ok) = self.open_table(m);
            if ok {
                loaded += 1;
                step = step.max(self.set().tables[ti].step());
            }
        }
        if loaded == 0 {
            return;
        }
        if step <= 1 {
            self.send(&format!("info string egtb: loaded {} cached tables the search can reach", loaded));
        } else {
            self.send(&format!("info string egtb: loaded {} cached tables the search can reach (step {}, the search resolves exact distances)", loaded, step));
        }
        self.flush();
    }

    /// Values a position from the side to move's view: from the table, or, for
    /// positions the tables leave out (an en passant right), from its children
    /// up to `depth` plies deep. None when no table covers it.
    fn eval(&self, b: &Board, depth: u32) -> Option<Value> {
        if let Some((v, lo, hi)) = self.set().lookup_bounds(b) {
            // a table that only bounds the distance (quantised) does not count:
            // the table stage reports exact values only, such positions go to
            // the search, which has the bounds as its oracle
            return if lo == hi { Some(v) } else { None };
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

    /// Frees the search's transposition table (before a generation).
    fn drop_tables(&mut self) {
        self.tables = None;
    }

    /// Empties the search's transposition table (ucinewgame).
    fn clear_tables(&mut self) {
        if let Some(t) = self.tables.as_mut() {
            t.clear();
        }
    }
}

/// Orders root moves: wins by shortest distance, then draws, then moves
/// without a table, then losses by longest distance.
fn rank(r: &RootMove) -> u32 {
    if r.wdl_only() {
        // a WDL win after the wins with a distance, a WDL loss before the
        // losses with one (its distance is unknown, so it may be the longest)
        match r.wdl {
            Verdict::Win => (1 << 10) - 1,
            Verdict::Loss => (3 << 10) - 512,
            _ => 1 << 10,
        }
    } else if !r.known {
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
/// Writes the own WDL table of an exact table to the cache folder when the
/// file is missing: the candidate orders are judged on samples and the
/// finalists packed (seconds for the base, minutes for five pieces). A
/// material the Syzygy files cover gets no own file: the prober would never
/// read it (the DTM file is written regardless, it is read after either).
fn write_wdl_file(set: &Set, gi: usize, tb: Option<&crate::syzygy::Tables>, workers: usize, say: &dyn Fn(&str)) {
    let t = &set.tables[gi];
    let path = set.wdl_file(&t.mat);
    if path.exists() || !t.is_generated() || !t.is_exact() {
        return;
    }
    if tb.is_some_and(|tb| tb.has_original(&t.mat)) {
        say(&format!("WDL of {} not written, Syzygy has it", t.mat.name()));
        return;
    }
    let start = Instant::now();
    match crate::syzygy::write::write_wdl(set, t, &path, crate::pairs::Options { workers, ..Default::default() }, crate::syzygy::write::BuildOptions::default(), workers, None) {
        Ok(b) => say(&format!("WDL written to {} ({} bytes, {} candidate orders, {:?})", path.display(), b.bytes(), b.candidates, Duration::from_secs(start.elapsed().as_secs()))),
        Err(e) => say(&format!("WDL: {}", e)),
    }
}

/// Writes the own DTM table of an exact table to the cache folder when the
/// file is missing (the distance buckets; read after the WDL verdict, so the
/// WDL file comes first).
fn write_dtm_file(set: &Set, gi: usize, workers: usize, say: &dyn Fn(&str)) {
    let t = &set.tables[gi];
    let path = set.dtm_file(&t.mat);
    if path.exists() || !t.is_generated() || !t.is_exact() {
        return;
    }
    let start = Instant::now();
    match crate::syzygy::write::write_dtm(set, t, &path, crate::pairs::Options { workers, ..Default::default() }, crate::syzygy::write::BuildOptions::default(), workers, None) {
        Ok(b) => say(&format!("DTM written to {} ({} bytes, {} candidate orders, {:?})", path.display(), b.bytes(), b.candidates, Duration::from_secs(start.elapsed().as_secs()))),
        Err(e) => say(&format!("DTM: {}", e)),
    }
}

fn compact(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn piece_count(b: &Board) -> usize {
    b.squares.iter().filter(|&&p| p != Piece::NONE).count()
}
