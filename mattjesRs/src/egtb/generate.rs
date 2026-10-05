//! The generator, see `mattjesGo/egtb/generate.go` and the design document,
//! section 3: level by level, a position is decided by looking at its children
//! with the forward move generator; un-moves only tell which positions are
//! worth looking at (the parents of positions decided in the previous level).

use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::table::{Set, Table, Value, MAX_PLIES};
use super::Material;
use crate::bitboard::{piece_attacks, Board};
use crate::chess::{new_buffer, Move, Piece, Pos, WIDTH};

/// Receives one line per phase.
pub type Progress<'a> = &'a mut dyn FnMut(&str);

/// Describes a generated table.
#[derive(Clone, Copy, Default, Debug)]
pub struct Stats {
    pub legal: usize,
    pub mates: usize,
    pub wins: usize,
    pub losses: usize,
    pub draws: usize,
    /// Longest distances in plies.
    pub max_win: u32,
    pub max_loss: u32,
    pub levels: u32,
    /// Candidate evaluations (forward move generation) over all levels.
    pub evaluations: usize,
    pub duration: Duration,
}

impl Stats {
    /// The longest forced mate in moves (the usual way to quote it).
    pub fn longest_mate(&self) -> u32 {
        self.max_win.div_ceil(2)
    }
}

/// The longest forced mates in moves from the literature, used to verify the
/// generator. Materials not listed are only printed.
pub const KNOWN_MAXIMA: &[(&str, u32)] = &[
    ("KQK", 10),
    ("KRK", 16),
    ("KBBK", 19),
    ("KBNK", 33),
    ("KQKR", 35),
    ("KQKQ", 13),
    ("KRKR", 19),
    ("KQKN", 21),
    ("KQKB", 17),
    ("KRKB", 29),
    ("KRKN", 40),
    ("KPK", 28),
    ("KPKP", 33),
];

impl Material {
    /// The materials a table looks up: captures (one piece of either side
    /// removed), promotions (a pawn becomes a piece) and both at once.
    fn dependencies(&self) -> Vec<Material> {
        let mut deps = Vec::new();
        let mut add = |w: &[Piece], b: &[Piece]| {
            if !w.is_empty() || !b.is_empty() {
                deps.push(Material::canonical(w, b)); // king against king needs no table
            }
        };
        let without = |ps: &[Piece], i: usize| -> Vec<Piece> {
            let mut r = ps.to_vec();
            r.remove(i);
            r
        };
        let promote = |ps: &[Piece], i: usize, to: Piece| -> Vec<Piece> {
            let mut r = ps.to_vec();
            r[i] = to;
            r
        };
        for i in 0..self.white.len() {
            add(&without(&self.white, i), &self.black);
        }
        for i in 0..self.black.len() {
            add(&self.white, &without(&self.black, i));
        }
        for to in [Piece::QUEEN, Piece::ROOK, Piece::BISHOP, Piece::KNIGHT] {
            for (i, &p) in self.white.iter().enumerate() {
                if p == Piece::PAWN {
                    add(&promote(&self.white, i, to), &self.black);
                    for j in 0..self.black.len() {
                        add(&promote(&self.white, i, to), &without(&self.black, j));
                    }
                }
            }
            for (i, &p) in self.black.iter().enumerate() {
                if p == Piece::PAWN {
                    add(&self.white, &promote(&self.black, i, to));
                    for j in 0..self.white.len() {
                        add(&without(&self.white, j), &promote(&self.black, i, to));
                    }
                }
            }
        }
        deps
    }
}

impl Set {
    /// Computes every table that is not filled yet, in order.
    pub fn generate_all(&mut self, workers: usize, progress: Progress) {
        for i in 0..self.tables.len() {
            if !self.tables[i].is_generated() {
                self.generate(i, workers, progress);
            }
        }
    }

    /// Computes one table (and first the tables it depends on, if they are
    /// missing).
    pub fn generate(&mut self, ti: usize, workers: usize, progress: Progress) -> Stats {
        for d in self.tables[ti].mat.dependencies() {
            let di = self.find(&d.name()).unwrap_or_else(|| panic!("egtb: no table for {}", d.name()));
            if !self.tables[di].is_generated() {
                self.generate(di, workers, progress);
            }
        }
        let start = Instant::now();
        self.tables[ti].values = (0..self.tables[ti].size).map(|_| std::sync::atomic::AtomicU8::new(0)).collect();
        let set: &Set = self;
        let t = &set.tables[ti];
        let g = Generator::new(set, t, true);

        let mut st = Stats::default();
        let (legal, mates) = parallel_chunks(workers, t.size, 1 << 16, &|lo, hi| g.init_range(lo, hi));
        st.legal = legal;
        st.mates = mates;
        progress(&format!("{:<5} {:>10} indices, {:>10} legal, {:>8} mates", t.mat.name(), t.size, legal, mates));

        for level in 1..=MAX_PLIES {
            let cur = &g.cand[(level % 3) as usize];
            {
                let mut pending = g.pending.lock().unwrap();
                for &idx in &pending[level as usize] {
                    cur.set(idx as usize);
                }
                pending[level as usize].clear();
            }
            let (changed, evals) = parallel_bits(workers, cur, &|idx| g.scan_candidate(level, idx));
            st.evaluations += evals;
            cur.clear();
            if changed > 0 {
                st.levels = level;
                if level % 2 == 1 {
                    st.wins += changed;
                    st.max_win = level;
                } else {
                    st.losses += changed;
                    st.max_loss = level;
                }
                progress(&format!("      level {:>3}: {:>9} positions, {:>10} evaluated", level, changed, evals));
            }
            if !g.cand[((level + 1) % 3) as usize].any() && !g.cand[((level + 2) % 3) as usize].any() && !g.pending_beyond(level) {
                break;
            }
        }
        st.losses += mates;
        st.draws = legal - st.wins - st.losses;
        st.duration = start.elapsed();
        progress(&format!(
            "{:<5} wins {:>9}, losses {:>9}, draws {:>9}, longest mate {} plies = {} moves, {:.1} s",
            t.mat.name(),
            st.wins,
            st.losses,
            st.draws,
            st.max_win,
            st.longest_mate(),
            st.duration.as_secs_f64()
        ));
        st
    }

    /// Recomputes every position of a finished table from its children with the
    /// forward move generator alone (no un-moves, no levels) and counts the
    /// positions whose stored value disagrees. The first few are reported.
    pub fn verify(&self, ti: usize, workers: usize, report: &(dyn Fn(&str) + Sync)) -> usize {
        let t = &self.tables[ti];
        let g = Generator::new(self, t, false);
        let reported = AtomicU32::new(0);
        let (bad, _) = parallel_chunks(workers, t.size, 1 << 16, &|lo, hi| {
            let mut sq = [Pos::NONE; 6];
            let mut buf = new_buffer();
            let n = g.pieces.len();
            let mut bad = 0;
            for idx in lo..hi {
                let sl = &mut sq[..n];
                let white_move = t.decode(idx, sl);
                let mut want = Value::DRAW;
                if !distinct_squares(sl) || t.index(white_move, sl) != idx as i64 {
                    want = Value::INVALID;
                } else {
                    let b = Board::from_pieces(white_move, sl, &g.pieces);
                    if b.opponent_in_check() {
                        want = Value::INVALID;
                    } else {
                        let m = b.gen_moves(&mut buf);
                        if m == 0 {
                            if b.in_check() {
                                want = Value::loss_in(0);
                            }
                        } else {
                            let (mut min_loss, mut max_win, mut all_win) = (u32::MAX, -1i64, true);
                            for &mv in &buf[..m] {
                                let v = g.child(&b, sl, mv);
                                if v.is_loss() {
                                    min_loss = min_loss.min(v.plies());
                                } else if v.is_win() {
                                    max_win = max_win.max(v.plies() as i64);
                                } else {
                                    all_win = false;
                                }
                            }
                            if min_loss < u32::MAX {
                                want = Value::win_in(min_loss + 1);
                            } else if all_win {
                                want = Value::loss_in(max_win as u32 + 1);
                            }
                        }
                    }
                }
                if t.value(idx) != want {
                    bad += 1;
                    if reported.fetch_add(1, Ordering::Relaxed) < 5 {
                        report(&format!("  {}: index {} {} stored {}, children say {}", t.mat.name(), idx, t.board(idx).fen(), t.value(idx), want));
                    }
                }
            }
            (bad, 0)
        });
        bad
    }
}

struct Generator<'a> {
    set: &'a Set,
    t: &'a Table,
    /// White king, black king, slots.
    pieces: Vec<Piece>,
    /// Pawns of both colors: a double step can create an en passant right.
    en_passant_possible: bool,
    /// `cand[level % 3]` holds the candidates of a level: positions with a child
    /// decided in the previous level (and, with en passant, two levels back).
    cand: [Bitset; 3],
    /// `pending[level]` lists positions whose decision waits for a level that no
    /// in-table child will trigger: their decisive children lie in smaller tables
    /// (captures, promotions) with a larger distance.
    pending: Mutex<Vec<Vec<u32>>>,
}

impl<'a> Generator<'a> {
    fn new(set: &'a Set, t: &'a Table, with_candidates: bool) -> Generator<'a> {
        let mut pieces = vec![Piece::WHITE_KING, Piece::BLACK_KING];
        pieces.extend_from_slice(&t.slots);
        let white_pawns = t.slots.contains(&Piece::WHITE_PAWN);
        let black_pawns = t.slots.contains(&Piece::BLACK_PAWN);
        let n = if with_candidates { t.size } else { 0 };
        Generator {
            set,
            t,
            pieces,
            en_passant_possible: white_pawns && black_pawns,
            cand: [Bitset::new(n), Bitset::new(n), Bitset::new(n)],
            pending: Mutex::new(vec![Vec::new(); MAX_PLIES as usize + 2]),
        }
    }

    fn register_pending(&self, level: u32, idx: usize) {
        assert!(level <= MAX_PLIES, "egtb {}: distance {} exceeds the value range", self.t.mat.name(), level);
        self.pending.lock().unwrap()[level as usize].push(idx as u32);
    }

    fn pending_beyond(&self, level: u32) -> bool {
        let p = self.pending.lock().unwrap();
        p[level as usize + 1..].iter().any(|v| !v.is_empty())
    }

    /// Marks invalid indices and mates, makes the parents of the mates the
    /// candidates of level 1 and registers positions whose decision can only
    /// come from a smaller table. Returns (legal, mates).
    fn init_range(&self, lo: usize, hi: usize) -> (usize, usize) {
        let t = self.t;
        let mut sq = [Pos::NONE; 6];
        let mut buf = new_buffer();
        let n = self.pieces.len();
        let (mut legal, mut mates) = (0, 0);
        for idx in lo..hi {
            let s = &mut sq[..n];
            let white_move = t.decode(idx, s);
            if !distinct_squares(s) || t.index(white_move, s) != idx as i64 {
                t.set_value(idx, Value::INVALID); // doubly occupied or dead (equal pieces in the other order)
                continue;
            }
            let b = Board::from_pieces(white_move, s, &self.pieces);
            if b.opponent_in_check() {
                t.set_value(idx, Value::INVALID);
                continue;
            }
            legal += 1;
            let m = b.gen_moves(&mut buf);
            if m == 0 {
                if b.in_check() {
                    t.set_value(idx, Value::loss_in(0));
                    mates += 1;
                    self.mark_parents(s, white_move, &self.cand[1]);
                }
                continue; // stalemate stays a draw
            }
            // children in smaller tables are final already: a lost one decides the
            // position at its level; if there is no other kind of child, so do
            // won ones (a quiet move or an open child would have to decide first)
            let (mut min_loss, mut max_win, mut all_cross, mut all_win) = (u32::MAX, -1i64, true, true);
            for &mv in &buf[..m] {
                if mv.capture == Piece::NONE && mv.promo == Piece::NONE {
                    all_cross = false;
                    continue;
                }
                let v = self.child(&b, s, mv);
                if v.is_loss() {
                    min_loss = min_loss.min(v.plies());
                } else if v.is_win() {
                    max_win = max_win.max(v.plies() as i64);
                } else {
                    all_win = false;
                }
            }
            if min_loss < u32::MAX {
                self.register_pending(min_loss + 1, idx);
            } else if all_cross && all_win {
                self.register_pending(max_win as u32 + 1, idx);
            }
        }
        (legal, mates)
    }

    /// Evaluates one candidate of the level: odd levels look for a win in `level`
    /// plies (the shortest lost child has level-1), even levels for a loss (every
    /// child is won by the opponent, the longest in level-1). Children inside the
    /// table never exceed level-1 at this point, children in smaller tables can:
    /// such a position is registered for the level it waits for. Returns
    /// (1 if decided, 1 evaluation).
    fn scan_candidate(&self, level: u32, idx: usize) -> (usize, usize) {
        let t = self.t;
        if t.value(idx) != Value::DRAW {
            return (0, 0);
        }
        let mut sq = [Pos::NONE; 6];
        let mut buf = new_buffer();
        let n = self.pieces.len();
        let s = &mut sq[..n];
        let white_move = t.decode(idx, s);
        if self.en_passant_possible {
            // a double step into this position may have created an en passant right;
            // that parent is decided by this position's children, not by this value
            self.mark_en_passant_parents(s, white_move, &self.cand[((level + 1) % 3) as usize]);
        }
        let b = Board::from_pieces(white_move, s, &self.pieces);
        let m = b.gen_moves(&mut buf);
        if m == 0 {
            return (0, 1);
        }
        let win = level % 2 == 1;
        let (mut min_loss, mut max_win, mut all_win) = (u32::MAX, -1i64, true);
        for &mv in &buf[..m] {
            let v = self.child(&b, s, mv);
            if v.is_loss() {
                min_loss = min_loss.min(v.plies());
                all_win = false; // a lost child makes the position a win, never a loss
                if !win || min_loss < level {
                    break; // even level: decided; odd level: cannot get shorter
                }
            } else if v.is_win() {
                max_win = max_win.max(v.plies() as i64);
            } else {
                all_win = false;
                if !win {
                    break; // an open or drawn child saves the position from losing
                }
            }
        }
        let mut decided = false;
        if win {
            if min_loss == level - 1 {
                t.set_value(idx, Value::win_in(level));
                decided = true;
            } else if min_loss < level - 1 {
                panic!("egtb {}: {} has a child lost in {} at level {}", t.mat.name(), b.fen(), min_loss, level);
            } else if min_loss < u32::MAX {
                self.register_pending(min_loss + 1, idx);
            }
        } else if all_win {
            let max_win = max_win as u32;
            if max_win == level - 1 {
                t.set_value(idx, Value::loss_in(level));
                decided = true;
            } else if max_win < level - 1 {
                panic!("egtb {}: {} has longest child {} at level {}", t.mat.name(), b.fen(), max_win, level);
            } else {
                self.register_pending(max_win + 1, idx);
            }
        }
        if !decided {
            return (0, 1);
        }
        self.mark_parents(s, white_move, &self.cand[((level + 1) % 3) as usize]);
        (1, 1)
    }

    /// Sets the candidate bit of every position that reaches the position (sq,
    /// white_move) with a quiet move: a piece of the side that just moved is put
    /// back on an empty square it could have come from. Illegal parents are
    /// harmless (their index is INVALID or they get re-evaluated), what matters
    /// is that no legal parent is missed. Un-captures and un-promotions do not
    /// exist inside one table.
    fn mark_parents(&self, sq: &[Pos], white_move: bool, into: &Bitset) {
        let mover = if white_move { Piece::BLACK } else { Piece::WHITE };
        let mut occ = 0u64;
        for &s in sq {
            occ |= 1 << s.idx();
        }
        let mut p = [Pos::NONE; 6];
        let n = sq.len();
        p[..n].copy_from_slice(sq);
        for i in 0..n {
            let piece = self.pieces[i];
            if piece.color() != mover {
                continue;
            }
            let from = sq[i];
            let mut targets = if piece.is(Piece::PAWN) { pawn_origins(piece, from, occ) } else { piece_attacks(piece, from, occ) & !occ };
            while targets != 0 {
                let t = Pos(targets.trailing_zeros() as i8);
                targets &= targets - 1;
                p[i] = t;
                let idx = self.t.index(!white_move, &p[..n]);
                if idx >= 0 {
                    into.set(idx as usize);
                }
            }
            p[i] = from;
        }
    }

    /// Handles the one edge that does not lead to a table position: if the side
    /// that just moved has a pawn on its double-step rank next to an enemy pawn,
    /// the parent that double-stepped reached a position WITH an en passant
    /// right, whose value comes from this position's children. That parent
    /// becomes a candidate whenever this position is one.
    #[allow(clippy::needless_range_loop)] // j indexes two parallel slices
    fn mark_en_passant_parents(&self, sq: &[Pos], white_move: bool, into: &Bitset) {
        let mover = if white_move { Piece::BLACK_PAWN } else { Piece::WHITE_PAWN };
        let mut occ = 0u64;
        for &s in sq {
            occ |= 1 << s.idx();
        }
        let mut p = [Pos::NONE; 6];
        let n = sq.len();
        p[..n].copy_from_slice(sq);
        for i in 2..n {
            if self.pieces[i] != mover {
                continue;
            }
            let from = sq[i];
            let origins = pawn_origins(mover, from, occ);
            let two = if mover == Piece::WHITE_PAWN { from - 2 * WIDTH } else { from + 2 * WIDTH };
            if origins & (1 << two.idx()) == 0 {
                continue; // no double step possible
            }
            for j in 2..n {
                // enemy pawn beside it?
                if self.pieces[j] != mover.opponent() || sq[j].y() != from.y() || (sq[j].0 - from.0).abs() != 1 {
                    continue;
                }
                p[i] = two;
                let idx = self.t.index(!white_move, &p[..n]);
                if idx >= 0 {
                    into.set(idx as usize);
                }
                p[i] = from;
            }
        }
    }

    /// The value of the position after move m (from the point of view of the
    /// opponent, who is then to move). Quiet moves stay in the table and only
    /// need the moved square replaced; captures and promotions lead into smaller
    /// tables; a double step that creates an en passant right leads to a position
    /// outside every table, whose value comes from its own children.
    fn child(&self, b: &Board, sq: &[Pos], m: Move) -> Value {
        if m.capture != Piece::NONE || m.promo != Piece::NONE {
            let mut c = *b;
            c.do_move(m);
            return self.set.lookup(&c).unwrap_or_else(|| panic!("egtb {}: no table for {}", self.t.mat.name(), c.fen()));
        }
        if self.en_passant_possible && b.squares[m.from.idx()].is(Piece::PAWN) && (m.to.0 - m.from.0).abs() == 2 * WIDTH {
            let mut c = *b;
            c.do_move(m);
            if c.en_passant.valid() {
                return self.eval_en_passant(&c);
            }
        }
        let mut s = [Pos::NONE; 6];
        let n = sq.len();
        s[..n].copy_from_slice(sq);
        for x in s[..n].iter_mut() {
            if *x == m.from {
                *x = m.to;
                break;
            }
        }
        self.t.value(self.t.index(!b.white_move, &s[..n]) as usize)
    }

    /// Values a position with an en passant right from its children: the side to
    /// move wins if any child is lost (shortest), loses if all children are won
    /// (longest), otherwise the value is open (0). Thanks to the level invariant
    /// the result is final as soon as it is decided.
    fn eval_en_passant(&self, c: &Board) -> Value {
        let mut buf = new_buffer();
        let n = c.gen_moves(&mut buf);
        if n == 0 {
            return if c.in_check() { Value::loss_in(0) } else { Value::DRAW };
        }
        let (mut min_loss, mut max_win, mut all_win) = (u32::MAX, -1i64, true);
        for &m in &buf[..n] {
            let mut d = *c;
            d.do_move(m);
            let v = self.set.lookup(&d).unwrap_or_else(|| panic!("egtb {}: no value for {} after en passant position", self.t.mat.name(), d.fen()));
            if v.is_loss() {
                min_loss = min_loss.min(v.plies());
            } else if v.is_win() {
                max_win = max_win.max(v.plies() as i64);
            } else {
                all_win = false;
            }
        }
        if min_loss < u32::MAX {
            Value::win_in(min_loss + 1)
        } else if all_win {
            Value::loss_in(max_win as u32 + 1)
        } else {
            Value::DRAW
        }
    }
}

/// The squares a pawn can have stepped from: one back, and two back from its
/// double-step rank with the square between empty.
fn pawn_origins(piece: Piece, sq: Pos, occ: u64) -> u64 {
    // white pawns move north (smaller index), so they came from the south
    let (back, double_rank): (i8, i8) = if piece.color() == Piece::BLACK { (-WIDTH, 3) } else { (WIDTH, 4) };
    let one = sq + back;
    if one.y() < 1 || one.y() > 6 || occ & (1 << one.idx()) != 0 {
        return 0;
    }
    let mut targets = 1u64 << one.idx();
    if sq.y() == double_rank {
        let two = one + back;
        if occ & (1 << two.idx()) == 0 {
            targets |= 1 << two.idx();
        }
    }
    targets
}

fn distinct_squares(sq: &[Pos]) -> bool {
    let mut seen = 0u64;
    for &s in sq {
        if seen & (1 << s.idx()) != 0 {
            return false;
        }
        seen |= 1 << s.idx();
    }
    true
}

/// A candidate set with atomic insertion, so all workers can mark.
pub struct Bitset(Vec<AtomicU64>);

impl Bitset {
    fn new(n: usize) -> Bitset {
        Bitset((0..n.div_ceil(64)).map(|_| AtomicU64::new(0)).collect())
    }

    #[inline(always)]
    fn set(&self, i: usize) {
        self.0[i >> 6].fetch_or(1 << (i & 63), Ordering::Relaxed);
    }

    fn clear(&self) {
        for w in &self.0 {
            w.store(0, Ordering::Relaxed);
        }
    }

    fn any(&self) -> bool {
        self.0.iter().any(|w| w.load(Ordering::Relaxed) != 0)
    }
}

/// Runs f over the index range in chunks on `workers` threads and sums its two results.
fn parallel_chunks(workers: usize, size: usize, chunk: usize, f: &(dyn Fn(usize, usize) -> (usize, usize) + Sync)) -> (usize, usize) {
    let workers = workers.max(1);
    let next = AtomicI64::new(0);
    let (a, b) = (AtomicI64::new(0), AtomicI64::new(0));
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let lo = next.fetch_add(chunk as i64, Ordering::Relaxed) as usize;
                if lo >= size {
                    return;
                }
                let (x, y) = f(lo, (lo + chunk).min(size));
                a.fetch_add(x as i64, Ordering::Relaxed);
                b.fetch_add(y as i64, Ordering::Relaxed);
            });
        }
    });
    (a.load(Ordering::Relaxed) as usize, b.load(Ordering::Relaxed) as usize)
}

/// Runs f over the set bits of a candidate set in chunks of words.
fn parallel_bits(workers: usize, bits: &Bitset, f: &(dyn Fn(usize) -> (usize, usize) + Sync)) -> (usize, usize) {
    parallel_chunks(workers, bits.0.len(), 1 << 10, &|lo, hi| {
        let (mut x, mut y) = (0, 0);
        for w in lo..hi {
            let mut word = bits.0[w].load(Ordering::Relaxed);
            while word != 0 {
                let (dx, dy) = f(w << 6 | word.trailing_zeros() as usize);
                x += dx;
                y += dy;
                word &= word - 1;
            }
        }
        (x, y)
    })
}
