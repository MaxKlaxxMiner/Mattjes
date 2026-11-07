//! Our own WDL tables in Syzygy's structure (the port of
//! mattjesGo/syzygy/write.go, docs/m5-endgame-tables.md "Eigene
//! WDL-Tabellen"): the same position index (`encode`), one stream per side
//! to move (and per file of the leading pawn), the piece order of every
//! stream chosen by trying the candidates like tbgen does, dead, illegal and
//! capture-decided positions as don't care, the streams packed with
//! `pairs`. The values come from an exact egtb table (DTM): 0 loss, 1 draw,
//! 2 win for the side to move. Go and Rust write byte-identical files.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::encode::{self, EncInfo, ENC_FILE, ENC_PIECE, MAX_PIECES};
use super::{name_of, BaseEntry};
use crate::bitboard::Board;
use crate::chess::{new_buffer, Piece, Pos};
use crate::egtb::{Material, Set, Table, Value};
use crate::pairs;

/// The Syzygy code of a coloured piece (pawn 1 .. king 6, black + 8).
fn code_of(p: Piece) -> u8 {
    let t = p.0 & Piece::TYPE_MASK.0;
    let mut c = if t == Piece::KING.0 {
        6
    } else if t == Piece::QUEEN.0 {
        5
    } else if t == Piece::ROOK.0 {
        4
    } else if t == Piece::BISHOP.0 {
        3
    } else if t == Piece::KNIGHT.0 {
        2
    } else {
        1
    };
    if p.0 & Piece::BLACK.0 != 0 {
        c += 8;
    }
    c
}

/// The bitboard kind (K 0 .. P 5) of a piece.
fn kind_of(p: Piece) -> usize {
    6 - (code_of(p) & 7) as usize
}

/// Renders a Syzygy piece code ("K" white, "k" black).
fn code_name(c: u8) -> char {
    let ch = b"?PNBRQK"[(c & 7) as usize];
    if c & 8 != 0 {
        ch.to_ascii_lowercase() as char
    } else {
        ch as char
    }
}

/// The piece counts of a material per side (our white is the first side).
pub fn counts_of(m: &Material) -> ([u8; 6], [u8; 6]) {
    let (mut first, mut second) = ([0u8; 6], [0u8; 6]);
    first[0] = 1;
    second[0] = 1;
    for &p in &m.white {
        first[kind_of(p)] += 1;
    }
    for &p in &m.black {
        second[kind_of(p)] += 1;
    }
    (first, second)
}

/// The coloured pieces of an egtb table in its square order.
fn pieces_of(m: &Material) -> Vec<Piece> {
    let mut ps = vec![Piece::WHITE_KING, Piece::BLACK_KING];
    ps.extend(m.slots());
    ps
}

/// Marks a position whose stored value does not matter; a symbol with the
/// BOUNDED_FLAG marks one whose stored value may be anything up to the
/// symbol below the flag.
pub const DONT_CARE: u8 = 0xff;
const BOUNDED_FLAG: u8 = 0x80;

/// The WDL of a material taken from an exact egtb table: per index the
/// symbol (0 loss, 1 draw, 2 win for the side to move), DONT_CARE for dead
/// and illegal indices, or BOUNDED+symbol for positions whose best capture
/// already achieves their value (the probe takes the maximum of the best
/// capture and the stored value).
pub struct Source<'a> {
    pub table: &'a Table,
    /// The tables the captures lead into (the sampling looks positions up here).
    pub set: &'a Set,
    /// WDL tables for the children `set` lacks (`new_with`).
    pub wdl: Option<&'a super::Tables>,
    pub symbols: Vec<u8>,
    /// Symbols the stream uses.
    pub alphabet: usize,
    pub legal: usize,
    /// Decided by a capture (WDL) or free as draws (DTM).
    pub decided: usize,
    /// DTM: the symbol is the quantised distance bucket of `step` moves
    /// (`signed`: wins and losses apart, else the bucket alone; `draw_free`:
    /// draws as don't care, the WDL table tells them).
    pub dtm: bool,
    pub step: u32,
    pub signed: bool,
    pub draw_free: bool,
    /// DTM: the longest distance in moves the buckets cover (127 for a
    /// one-byte table).
    pub top: u32,
}

/// The longest distance of a table in moves.
fn longest_moves(t: &Table, workers: usize) -> u32 {
    let n = t.size;
    let chunk = n.div_ceil(workers.max(1));
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers.max(1)).map(|w| s.spawn(move || (w * chunk..n.min((w + 1) * chunk)).map(|i| t.value(i).moves()).max().unwrap_or(0))).collect();
        handles.into_iter().map(|h| h.join().unwrap()).max().unwrap_or(0)
    })
}

fn symbol_of(v: Value) -> u8 {
    if v.is_win() {
        2
    } else if v.is_loss() {
        0
    } else {
        1
    }
}

impl Source<'_> {
    /// The stream symbol of a legal position with the table value v.
    fn symbol_at(&self, b: &Board, buf: &mut crate::chess::MoveBuffer, v: Value) -> u8 {
        if !self.dtm {
            return capture_bound(self.set, self.wdl, b, buf, symbol_of(v));
        }
        let q = v.quantize_to(self.step, self.top);
        let step = self.step as usize;
        if q.0 == 0 {
            return if self.draw_free { DONT_CARE } else { 0 };
        }
        if q.is_win() {
            return (q.moves() as usize).div_ceil(step) as u8; // 1 .. ceil(top/step)
        }
        let m = q.moves() as usize; // loss: bucket 0 = mated now
        let bucket = m.div_ceil(step) as u8;
        if self.signed {
            (self.top as usize).div_ceil(step) as u8 + 1 + bucket
        } else {
            bucket
        }
    }
}

fn distinct_squares(sq: &[Pos]) -> bool {
    let mut seen = 0u64;
    for s in sq {
        let bit = 1u64 << s.0;
        if seen & bit != 0 {
            return false;
        }
        seen |= bit;
    }
    true
}

impl<'a> Source<'a> {
    /// Derives the WDL symbols from a table with exact values in memory, in parallel.
    pub fn new(set: &'a Set, t: &'a Table, workers: usize) -> Source<'a> {
        Source::new_with(set, t, None, workers)
    }

    /// `new` with WDL tables that answer the children `set` has no table for
    /// (the promotions of a material keep its piece count: their targets are
    /// looked up in our own WDL files written before, PROMOTION_BOUND).
    pub fn new_with(set: &'a Set, t: &'a Table, wdl: Option<&'a super::Tables>, workers: usize) -> Source<'a> {
        Source { table: t, set, wdl, symbols: Vec::new(), alphabet: 3, legal: 0, decided: 0, dtm: false, step: 1, signed: false, draw_free: false, top: 0 }.derive(workers)
    }

    /// Derives the quantised distance symbols (buckets of step moves; signed
    /// = wins and losses apart, else the bucket alone; draw_free = draws as
    /// don't care) from a table with exact values in memory.
    pub fn new_dtm(set: &'a Set, t: &'a Table, step: u32, signed: bool, draw_free: bool, workers: usize) -> Source<'a> {
        // a one-byte table reaches 127 moves; a widened one (WIDE_TABLES) up to its own longest
        // distance, which the buckets must cover instead of capping it at 127
        let top = if t.is_wide() { longest_moves(t, workers).max(127) } else { 127 };
        let buckets = top.div_ceil(step) as usize + 1; // 0 .. ceil(top/step)
        let alphabet = if signed { 2 * buckets + 1 } else { buckets };
        // symbols from 0x80 on mean "bounded" to the filling
        assert!(alphabet < BOUNDED_FLAG as usize, "syzygy: {} DTM buckets of {} moves do not fit a symbol", alphabet, step);
        Source { table: t, set, wdl: None, symbols: Vec::new(), alphabet, legal: 0, decided: 0, dtm: true, step, signed, draw_free, top }.derive(workers)
    }

    fn derive(mut self, workers: usize) -> Source<'a> {
        let (set, t) = (self.set, self.table);
        assert!((!t.values.is_empty() || t.is_wide()) && t.is_exact(), "syzygy: Source needs an exact table in memory");
        let pieces = pieces_of(&t.mat);
        let n = t.size;
        let symbols: Vec<std::sync::atomic::AtomicU8> = (0..n).map(|_| std::sync::atomic::AtomicU8::new(DONT_CARE)).collect();
        let legal = AtomicUsize::new(0);
        let decided = AtomicUsize::new(0);
        let chunk = n.div_ceil(workers.max(1));
        let src = &self;
        std::thread::scope(|s| {
            for w in 0..workers.max(1) {
                let (lo, hi) = (w * chunk, n.min((w + 1) * chunk));
                let (pieces, symbols, legal, decided) = (&pieces, &symbols, &legal, &decided);
                s.spawn(move || {
                    let mut sq = vec![Pos(0); pieces.len()];
                    let mut buf = new_buffer();
                    let (mut nl, mut nd) = (0, 0);
                    for (idx, slot) in symbols.iter().enumerate().take(hi).skip(lo) {
                        let white_move = t.decode(idx, &mut sq);
                        if !distinct_squares(&sq) || t.index(white_move, &sq) != idx as i64 {
                            continue;
                        }
                        let b = Board::from_pieces(white_move, &sq, pieces);
                        if b.opponent_in_check() {
                            continue;
                        }
                        nl += 1;
                        let own = src.symbol_at(&b, &mut buf, t.value(idx));
                        if own & BOUNDED_FLAG != 0 {
                            nd += 1; // bounded, or DONT_CARE (a free draw)
                        }
                        slot.store(own, Ordering::Relaxed);
                    }
                    legal.fetch_add(nl, Ordering::Relaxed);
                    decided.fetch_add(nd, Ordering::Relaxed);
                });
            }
        });
        let _ = set;
        self.symbols = symbols.into_iter().map(|a| a.into_inner()).collect();
        self.legal = legal.into_inner();
        self.decided = decided.into_inner();
        self
    }
}

/// Whether promotions count like captures for the bound (an experiment
/// switch, card MT-8): the reader resolves promotions as well, which is
/// correct for every table (it only adds true values of legal moves).
pub static PROMOTION_BOUND: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The symbol as the stream stores it: own, or BOUNDED+own when the best
/// capture already achieves it.
fn capture_bound(set: &Set, wdl: Option<&super::Tables>, b: &Board, buf: &mut crate::chess::MoveBuffer, own: u8) -> u8 {
    let (mut best, mut has_capture) = (0u8, false);
    let nm = b.gen_moves(buf);
    let promotions = PROMOTION_BOUND.load(Ordering::Relaxed);
    for &m in &buf[..nm] {
        if m.capture == Piece::NONE && !(promotions && m.promo != Piece::NONE) {
            continue;
        }
        let mut c = *b;
        c.do_move(m);
        // the child's symbol from its side to move: the exact table, else the WDL files
        let child = match set.lookup(&c) {
            Some(v) => symbol_of(v),
            None => match wdl.and_then(|w| w.probe_wdl_with(&c, Some(set))) {
                Some(v) => (v.signum() + 1) as u8, // cursed results count as results
                None if m.capture == Piece::NONE => continue, // a promotion without a table: no bound
                None => panic!("syzygy: no table after a capture {}", c.fen()),
            },
        };
        has_capture = true;
        best = best.max(2 - child);
    }
    if has_capture && best == own {
        own | BOUNDED_FLAG
    } else {
        own
    }
}

/// How the don't-care symbols are filled (an experiment switch of the packing
/// bench, card MT-8): 0 = the predecessor (bounded ones the bound when the
/// predecessor does not fit), 1 = per run of free symbols the left neighbour
/// as far as it fits, then the right neighbour from the other end, the rest
/// as in 0.
pub static FILL_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Whether the plain symbol v may stand at a position holding s (a plain
/// symbol only itself, a bounded one anything up to its bound, don't care
/// anything).
fn fits(s: u8, v: u8) -> bool {
    s == DONT_CARE || (s & BOUNDED_FLAG != 0 && v <= s & !BOUNDED_FLAG) || s == v
}

fn fill_dont_care(values: &mut [u8]) {
    fill_mode(values, FILL_MODE.load(Ordering::Relaxed));
}

/// Fills the free symbols by the rule `mode` (see FILL_MODE).
pub fn fill_mode(values: &mut [u8], mode: u8) {
    match mode {
        1 => fill_neighbours(values),
        3 => fill_runs(values, false),
        4 => fill_runs(values, true),
        2 => {
            let p = best_period(values);
            fill_period(values, p);
        }
        _ => fill_predecessor(values),
    }
}

/// The period (2..=4096) at which plain symbols repeat most often: the
/// stream walks the last piece over its squares, so the same constellation
/// with the last piece one square back lies one period earlier. Measured on
/// a sample of plain positions.
pub fn best_period(values: &[u8]) -> usize {
    let plain = |s: u8| s != DONT_CARE && s & BOUNDED_FLAG == 0;
    let n = values.len();
    if n < 8192 {
        return 64.min(n.max(2) - 1).max(1);
    }
    let samples = 1 << 16;
    let stride = (n - 4096) / samples;
    let mut best = (0usize, 64usize);
    for p in 2..=4096usize {
        let mut hits = 0;
        let mut i = 4096;
        while i < n {
            let (a, b) = (values[i], values[i - p]);
            if plain(a) && plain(b) && a == b {
                hits += 1;
            }
            i += stride.max(1);
        }
        if hits > best.0 {
            best = (hits, p);
        }
    }
    best.1
}

/// Fill mode 2: a free position takes the symbol one period earlier when it
/// fits (the same constellation with the last piece one square back), else
/// the predecessor rule.
pub fn fill_period(values: &mut [u8], period: usize) {
    let mut prev = 1u8;
    for i in 0..values.len() {
        let s = values[i];
        if s != DONT_CARE && s & BOUNDED_FLAG == 0 {
            prev = s;
            continue;
        }
        let v = if i >= period && fits(s, values[i - period]) {
            values[i - period]
        } else if fits(s, prev) {
            prev
        } else {
            s & !BOUNDED_FLAG
        };
        values[i] = v;
        prev = v;
    }
}

/// Fill mode 1: every maximal run of free positions (don't care or bounded)
/// takes the plain symbol on its left as far as it fits, the plain symbol
/// on its right from the other end as far as it fits, and the middle that
/// neither fits the predecessor rule.
fn fill_neighbours(values: &mut [u8]) {
    let n = values.len();
    let free = |s: u8| s == DONT_CARE || s & BOUNDED_FLAG != 0;
    let mut prev: Option<u8> = None;
    let mut i = 0;
    while i < n {
        if !free(values[i]) {
            prev = Some(values[i]);
            i += 1;
            continue;
        }
        let mut j = i;
        while j < n && free(values[j]) {
            j += 1;
        }
        let next = if j < n { Some(values[j]) } else { None };
        let mut a = i;
        if let Some(l) = prev {
            while a < j && fits(values[a], l) {
                values[a] = l;
                a += 1;
            }
        }
        let mut b = j;
        if let Some(r) = next {
            while b > a && fits(values[b - 1], r) {
                values[b - 1] = r;
                b -= 1;
            }
        }
        if a < b {
            fill_predecessor_from(&mut values[a..b], next.or(prev).unwrap_or(1));
        }
        i = j;
    }
}

/// Fill modes 3 and 4: every maximal run of free positions takes one
/// neighbour as a whole when it fits the whole run: mode 3 the right one
/// first, mode 4 the left one when the two plain symbols before the run are
/// equal (a run continues), else the right one; when neither fits, the
/// neighbour rule of mode 1.
fn fill_runs(values: &mut [u8], pin_left: bool) {
    let n = values.len();
    let free = |s: u8| s == DONT_CARE || s & BOUNDED_FLAG != 0;
    let mut i = 0;
    while i < n {
        if !free(values[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < n && free(values[j]) {
            j += 1;
        }
        let left = if i > 0 { Some(values[i - 1]) } else { None };
        let right = if j < n { Some(values[j]) } else { None };
        let all = |v: u8| values[i..j].iter().all(|&s| fits(s, v));
        let pinned = pin_left && i >= 2 && values[i - 2] == values[i - 1];
        let choice = if pinned && left.is_some_and(all) {
            left
        } else if right.is_some_and(all) {
            right
        } else if left.is_some_and(all) {
            left
        } else {
            None
        };
        match choice {
            Some(v) => values[i..j].iter_mut().for_each(|s| *s = v),
            None => {
                // neither neighbour fits the whole run: mode 1 on this run alone
                let lo = i.saturating_sub(1);
                let hi = (j + 1).min(n);
                fill_neighbours(&mut values[lo..hi]);
            }
        }
        i = j;
    }
}

/// Fill mode 0: the predecessor.
fn fill_predecessor(values: &mut [u8]) {
    fill_predecessor_from(values, 1);
}

/// Replaces the don't-care symbols by their predecessor (bounded ones by
/// the predecessor when it fits, else the bound), starting with `prev`.
fn fill_predecessor_from(values: &mut [u8], mut prev: u8) {
    for v in values.iter_mut() {
        if *v == DONT_CARE {
            *v = prev;
        } else if *v & BOUNDED_FLAG != 0 {
            let bound = *v & !BOUNDED_FLAG;
            if prev > bound {
                prev = bound;
            }
            *v = prev;
        } else {
            prev = *v;
        }
    }
}

/// The coloured piece of a Syzygy code.
fn piece_of_code(c: u8) -> Piece {
    let p = match c & 7 {
        1 => Piece::PAWN,
        2 => Piece::KNIGHT,
        3 => Piece::BISHOP,
        4 => Piece::ROOK,
        5 => Piece::QUEEN,
        _ => Piece::KING,
    };
    if c & 8 != 0 {
        Piece::BLACK | p
    } else {
        Piece::WHITE | p
    }
}

/// The piece order of one stream (Syzygy codes, the leading group first)
/// with the chain positions of the leading group and of the other colour's
/// pawns (0x0f without).
#[derive(Clone, Debug)]
pub struct Order {
    pub pieces: Vec<u8>,
    pub order: usize,
    pub order2: usize,
}

impl std::fmt::Display for Order {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for &c in &self.pieces {
            write!(f, "{}", code_name(c))?;
        }
        write!(f, "/{}", self.order)?;
        if self.order2 != 0x0f {
            write!(f, "/{}", self.order2)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Group {
    code: u8,
    count: usize,
}

fn permutations(n: usize) -> Vec<Vec<usize>> {
    if n == 0 {
        return vec![vec![]];
    }
    let mut out = Vec::new();
    for rest in permutations(n - 1) {
        for pos in 0..n {
            let mut p = Vec::with_capacity(n);
            p.extend_from_slice(&rest[..pos]);
            p.push(n - 1);
            p.extend_from_slice(&rest[pos..]);
            out.push(p);
        }
    }
    out
}

/// The piece orders worth trying for a stream of the material (see the Go
/// package for the rules).
pub fn candidates(be: &BaseEntry, first: &[u8; 6], second: &[u8; 6]) -> Vec<Order> {
    let mut groups: Vec<Group> = Vec::new();
    for (colour, counts) in [first, second].into_iter().enumerate() {
        for (kind, &c) in counts.iter().enumerate() {
            if c > 0 {
                groups.push(Group { code: (6 - kind as u8) | (colour as u8) << 3, count: c as usize });
            }
        }
    }
    let expand = |lead: &[u8], rest: &[Group], perm: &[usize]| -> Vec<u8> {
        let mut pieces = lead.to_vec();
        for &i in perm {
            for _ in 0..rest[i].count {
                pieces.push(rest[i].code);
            }
        }
        pieces
    };
    let mut out = Vec::new();
    if !be.has_pawns {
        let singles: Vec<u8> = groups.iter().filter(|g| g.count == 1).map(|g| g.code).collect();
        let multis: Vec<Group> = groups.iter().filter(|g| g.count >= 2).copied().collect();
        let mut leads: Vec<Vec<u8>> = Vec::new();
        let mut rests: Vec<Vec<Group>> = Vec::new();
        if be.shape.kk_enc {
            leads.push(vec![6, 14]);
            leads.push(vec![14, 6]);
            rests.push(multis.clone());
            rests.push(multis.clone());
        } else {
            for &a in &singles {
                for &b in &singles {
                    for &c in &singles {
                        if a == b || a == c || b == c {
                            continue;
                        }
                        leads.push(vec![a, b, c]);
                        let mut rest = multis.clone();
                        for &s in &singles {
                            if s != a && s != b && s != c {
                                rest.push(Group { code: s, count: 1 });
                            }
                        }
                        rests.push(rest);
                    }
                }
            }
        }
        for (lead, rest) in leads.iter().zip(rests.iter()) {
            for perm in permutations(rest.len()) {
                let pieces = expand(lead, rest, &perm);
                for order in 0..=rest.len() {
                    out.push(Order { pieces: pieces.clone(), order, order2: 0x0f });
                }
            }
        }
        return out;
    }
    let lead_colour = if second[5] != 0 && (first[5] == 0 || first[5] > second[5]) { 1 } else { 0 };
    let mut lead: Vec<u8> = Vec::new();
    let mut rest: Vec<Group> = Vec::new();
    let mut other = false;
    for g in &groups {
        if g.code & 7 == 1 && (g.code >> 3) as usize == lead_colour {
            for _ in 0..g.count {
                lead.push(g.code);
            }
        } else if g.code & 7 == 1 {
            other = true;
        } else {
            rest.push(*g);
        }
    }
    if other {
        let code = 1u8 | ((1 - lead_colour) as u8) << 3;
        let total = (first[5] + second[5]) as usize;
        for _ in 0..total - lead.len() {
            lead.push(code);
        }
    }
    let chain = rest.len() + 1 + other as usize;
    for perm in permutations(rest.len()) {
        let pieces = expand(&lead, &rest, &perm);
        for order in 0..chain {
            if !other {
                out.push(Order { pieces: pieces.clone(), order, order2: 0x0f });
                continue;
            }
            for order2 in 0..chain {
                if order2 != order {
                    out.push(Order { pieces: pieces.clone(), order, order2 });
                }
            }
        }
    }
    out
}

/// One packed value stream: a side to move (and a pawn file).
pub struct Stream {
    pub order: Order,
    pub size: u64,
    pub packed: Vec<u8>,
    pub stats: pairs::Stats,
}

/// A WDL table ready to be written: its material and its streams (per side
/// to move, pawn files innermost; one side for symmetric materials).
pub struct Built {
    pub name: String,
    pub first: [u8; 6],
    pub second: [u8; 6],
    pub sides: usize,
    pub files: usize,
    pub streams: Vec<Stream>,
    pub candidates: usize,
    /// DTM: the streams hold distance buckets of `step` moves instead of
    /// WDL symbols (signed: wins and losses apart; draw_free: draws are
    /// don't care, the WDL table tells them).
    pub dtm: bool,
    pub step: u32,
    pub signed: bool,
    pub draw_free: bool,
    pub alphabet: usize,
    /// DTM: only this side to move is stored (`keep_side`).
    pub stored_side: Option<usize>,
}

impl Built {
    /// DTM of a material that is not symmetric: drops the streams of the
    /// side that packs larger (the reader values that side from its
    /// children, a ply deeper). Returns the side kept.
    pub fn keep_smaller_side(&mut self) -> Option<usize> {
        if !self.dtm || self.sides != 2 {
            return None;
        }
        let bytes = |s: usize| -> usize { self.streams[s * self.files..(s + 1) * self.files].iter().map(|st| st.packed.len()).sum() };
        let keep = if bytes(1) < bytes(0) { 1 } else { 0 };
        let drop = 1 - keep;
        self.streams.drain(drop * self.files..(drop + 1) * self.files);
        self.sides = 1;
        self.stored_side = Some(keep);
        Some(keep)
    }
    /// Per side the distinct orders of its streams (one for a pawnless
    /// material, up to four with pawns): `BuildOptions::orders` for a second
    /// stream of the same material.
    pub fn orders(&self) -> Vec<Vec<Order>> {
        (0..self.sides)
            .map(|side| {
                let mut orders: Vec<Order> = Vec::new();
                for f in 0..self.files {
                    let o = &self.streams[side * self.files + f].order;
                    if !orders.iter().any(|have| have.to_string() == o.to_string()) {
                        orders.push(o.clone());
                    }
                }
                orders
            })
            .collect()
    }

    /// The size of all packed streams.
    pub fn bytes(&self) -> usize {
        self.streams.iter().map(|s| s.packed.len()).sum()
    }

    /// Writes the table to path (the folder is created): a 32-byte header
    /// ("MWDL", version, pieces, sides, files, the piece counts of both
    /// sides), one 32-byte descriptor per stream (piece codes, chain
    /// positions, offset and length) and the containers, each 8-aligned.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let n = self.streams.len();
        let mut head = vec![0u8; OWN_HEADER + OWN_DESCRIPTOR * n];
        head[..4].copy_from_slice(OWN_MAGIC);
        head[4] = OWN_VERSION;
        let pieces: u8 = self.first.iter().chain(self.second.iter()).sum();
        head[5] = pieces;
        head[6] = self.sides as u8;
        head[7] = self.files as u8;
        head[8..14].copy_from_slice(&self.first);
        head[14..20].copy_from_slice(&self.second);
        if self.dtm {
            head[..4].copy_from_slice(DTM_MAGIC);
            head[20] = self.step as u8;
            head[21] = if self.signed { DTM_SIGNED } else { 0 } | if self.draw_free { DTM_DRAW_FREE } else { 0 };
            if let Some(s) = self.stored_side {
                head[21] |= DTM_ONE_SIDE | if s == 1 { DTM_SECOND_SIDE } else { 0 };
            }
            head[22] = self.alphabet as u8;
        }
        let mut offset = head.len();
        for (i, s) in self.streams.iter().enumerate() {
            let d = &mut head[OWN_HEADER + OWN_DESCRIPTOR * i..];
            d[..s.order.pieces.len()].copy_from_slice(&s.order.pieces);
            d[7] = s.order.order as u8;
            d[8] = s.order.order2 as u8;
            d[16..24].copy_from_slice(&(offset as u64).to_le_bytes());
            d[24..32].copy_from_slice(&(s.packed.len() as u64).to_le_bytes());
            offset += (s.packed.len() + 7) & !7;
        }
        // written beside the target and renamed when complete: a reader
        // never sees a partial file (a sweep may be killed while writing)
        let tmp = crate::egtb::tmp_path(path);
        let mut w = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(&tmp)?);
        w.write_all(&head)?;
        for s in &self.streams {
            w.write_all(&s.packed)?;
            let pad = ((s.packed.len() + 7) & !7) - s.packed.len();
            w.write_all(&[0u8; 8][..pad])?;
        }
        w.flush()?;
        drop(w);
        std::fs::rename(&tmp, path)
    }
}

pub(super) const OWN_MAGIC: &[u8; 4] = b"MWDL";
/// The same layout with byte 20 = step, 21 = flags, 22 = alphabet.
pub(super) const DTM_MAGIC: &[u8; 4] = b"MDTM";
pub(super) const DTM_SIGNED: u8 = 1;
pub(super) const DTM_DRAW_FREE: u8 = 2;
/// Only one side to move is stored (`Built::keep_side`); the reader values
/// the other side from its children (`Tables::probe_dtm_bounds`).
pub(super) const DTM_ONE_SIDE: u8 = 4;
/// With DTM_ONE_SIDE: the stored side is the table's second one.
pub(super) const DTM_SECOND_SIDE: u8 = 8;

/// Derives, builds and writes the own DTM table of an exact egtb table: the
/// distance buckets of egtb::FILE_STEP moves, wins and losses as one bucket
/// (the WDL table tells which), draws free. Read through Tables::probe_dtm
/// after a WDL verdict.
pub fn write_dtm(set: &Set, t: &Table, path: &Path, opt: pairs::Options, bo: BuildOptions, workers: usize, progress: Option<&dyn Fn(&str)>) -> std::io::Result<Built> {
    let src = Source::new_dtm(set, t, crate::egtb::FILE_STEP, false, true, workers);
    let b = build(&src, opt, bo, workers, progress);
    b.write(path)?;
    Ok(b)
}
pub(super) const OWN_VERSION: u8 = 1;
pub(super) const OWN_HEADER: usize = 32;
pub(super) const OWN_DESCRIPTOR: usize = 32;

/// Controls the candidate preselection: a large stream is not built for
/// every candidate order; windows spread over the stream are enumerated with
/// the decoder, looked up and packed, and only the best candidates of that
/// sample are built completely. Zero values take the defaults.
#[derive(Clone, Debug, Default)]
pub struct BuildOptions {
    /// Positions sampled per stream (0 = 2 Mi); a stream with fewer than
    /// twice that many positions is built completely for every candidate.
    pub sample_target: usize,
    /// Candidates built completely per stream after the sampling (0 = 3).
    pub finalists: usize,
    /// Positions per sample window (0 = 4096).
    pub window: usize,
    /// A stream with more positions than this is packed with a dictionary
    /// from a sample of dict_sample values instead of Re-Pair over the whole
    /// stream (0 = 512 Mi; see the Go package).
    pub dict_above: usize,
    /// The sample of the dictionary above dict_above (0 = 16 Mi).
    pub dict_sample: usize,
    /// Caps the packer's alphabet while judging a sample (0 = pairs' 4095):
    /// a ranking does not need the full dictionary, and every 64 symbols
    /// are a pass over the sample.
    pub sample_symbols: usize,
    /// When set (one list per side), replaces the candidates: no sampling,
    /// these orders are built and the best per file kept. The DTM file takes
    /// the orders its WDL file chose (`Built::orders`), which saves the
    /// sampling, the larger half of its time on five pieces.
    pub orders: Vec<Vec<Order>>,
}

impl BuildOptions {
    fn with_defaults(mut self) -> BuildOptions {
        // measured on KNNKQ and KBNKQ (docs/m5-endgame-tables.md, "Schneller
        // schreiben"): 1 Mi positions and 512 symbols judge the candidates as
        // well as 2 Mi and the full dictionary at a tenth of the time, three
        // finalists are needed (the sample's best is often not the real best)
        if self.sample_target == 0 {
            self.sample_target = 1 << 20;
        }
        if self.sample_symbols == 0 {
            self.sample_symbols = 512;
        }
        if self.finalists == 0 {
            self.finalists = 3;
        }
        if self.window == 0 {
            self.window = 4096;
        }
        if self.dict_above == 0 {
            self.dict_above = 512 << 20;
        }
        if self.dict_sample == 0 {
            self.dict_sample = 16 << 20;
        }
        self
    }
}

/// Packs the WDL of a source: every candidate order per side on small
/// streams, the finalists of the sampled windows on large ones, keeping the
/// smallest stream per side and file.
pub fn build(src: &Source, opt: pairs::Options, bo: BuildOptions, workers: usize, progress: Option<&dyn Fn(&str)>) -> Built {
    let bo = bo.with_defaults();
    let (first, second) = counts_of(&src.table.mat);
    let be = BaseEntry::describe(name_of(&first, &second), &first, &second).expect("syzygy: too many pieces");
    let sides = if be.symmetric { 1 } else { 2 };
    let files = if be.has_pawns { 4 } else { 1 };
    let cands = candidates(&be, &first, &second);
    let enc = if be.has_pawns { ENC_FILE } else { ENC_PIECE };
    let mut streams: Vec<Option<Stream>> = (0..sides * files).map(|_| None).collect();
    for side in 0..sides {
        let mut ei = EncInfo::default();
        ei.pieces[..cands[0].pieces.len()].copy_from_slice(&cands[0].pieces);
        let size = encode::setup(&mut ei, &be.shape, cands[0].order, cands[0].order2, 0, enc);
        let mut chosen: Vec<&Order> = cands.iter().collect();
        if bo.orders.len() == sides {
            chosen = bo.orders[side].iter().collect(); // given: no sampling
        } else if size > 2 * bo.sample_target as u64 && cands.len() > bo.finalists {
            // the sample: per candidate and file the packed size of the windows
            let scores: Vec<Vec<usize>> = cands
                .iter()
                .map(|o| {
                    let s = sample_side(src, &be, side, o, files, &bo, opt, workers);
                    if let Some(p) = progress {
                        p(&format!("{} side {} order {:<12} sample {:?} bytes", be.name, side, o, s));
                    }
                    s
                })
                .collect();
            // the finalists: the best of every file, as a set
            let mut picked = vec![false; cands.len()];
            for f in 0..files {
                let mut order: Vec<usize> = (0..cands.len()).collect();
                order.sort_by_key(|&c| scores[c].get(f).copied());
                for &c in &order[..bo.finalists] {
                    picked[c] = true;
                }
            }
            chosen = cands.iter().enumerate().filter(|(c, _)| picked[*c]).map(|(_, o)| o).collect();
        }
        for o in chosen {
            let (values, sizes) = build_side(src, &be, side, o, files, workers);
            for f in 0..files {
                let mut po = opt;
                if sizes[f] > bo.dict_above as u64 && po.dict_sample == 0 {
                    po.dict_sample = bo.dict_sample;
                }
                let (packed, st) = pairs::pack_stats(&values[f], src.alphabet, po);
                let bytes = packed.len();
                let slot = &mut streams[side * files + f];
                if slot.as_ref().is_none_or(|s| bytes < s.packed.len()) {
                    *slot = Some(Stream { order: o.clone(), size: sizes[f], packed, stats: st });
                }
                if let Some(p) = progress {
                    p(&format!("{} side {} file {} order {:<12} {:>8} bytes ({} tokens, {} symbols)", be.name, side, f, o, bytes, st.tokens, st.symbols));
                }
            }
        }
    }
    Built { name: be.name.clone(), first, second, sides, files, streams: streams.into_iter().map(|s| s.unwrap()).collect(), candidates: cands.len(), dtm: src.dtm, step: src.step, signed: src.signed, draw_free: src.draw_free, alphabet: src.alphabet, stored_side: None }
}

/// Lays the symbols of one side to move out in the index order of a
/// candidate (one stream per pawn file), don't-care positions filled with
/// their predecessor (bounded ones with the predecessor when it fits, else
/// the bound).
fn build_side(src: &Source, be: &BaseEntry, side: usize, o: &Order, files: usize, workers: usize) -> (Vec<Vec<u8>>, Vec<u64>) {
    let enc = if be.has_pawns { ENC_FILE } else { ENC_PIECE };
    let mut eis = vec![EncInfo::default(); files];
    let mut sizes = vec![0u64; files];
    let mut values: Vec<Vec<std::sync::atomic::AtomicU8>> = Vec::with_capacity(files);
    for f in 0..files {
        eis[f].pieces[..o.pieces.len()].copy_from_slice(&o.pieces);
        sizes[f] = encode::setup(&mut eis[f], &be.shape, o.order, o.order2, f, enc);
        values.push((0..sizes[f]).map(|_| std::sync::atomic::AtomicU8::new(DONT_CARE)).collect());
    }
    let t = src.table;
    let pieces = pieces_of(&t.mat);
    let n = t.size;
    let chunk = n.div_ceil(workers.max(1));
    std::thread::scope(|s| {
        for w in 0..workers.max(1) {
            let (lo, hi) = (w * chunk, n.min((w + 1) * chunk));
            let (pieces, eis, values) = (&pieces, &eis, &values);
            s.spawn(move || {
                let mut sq = vec![Pos(0); pieces.len()];
                let mut p = [0usize; MAX_PIECES];
                for idx in lo..hi {
                    let sym = src.symbols[idx];
                    if sym == DONT_CARE {
                        continue;
                    }
                    if t.decode(idx, &mut sq) != (side == 0) {
                        continue;
                    }
                    let mut i = 0;
                    while i < o.pieces.len() {
                        i = squares_of(o.pieces[i], pieces, &sq, &mut p, i);
                    }
                    let f = if be.has_pawns { encode::leading_pawn(&mut p, &be.shape, ENC_FILE) } else { 0 };
                    let mut q = p;
                    values[f][encode::encode(&mut q, &eis[f], &be.shape, enc) as usize].store(sym, Ordering::Relaxed);
                    if !be.has_pawns {
                        // the squares as encode sees them before its diagonal
                        // step: mirrored into the a1-d4 quadrant by the first piece
                        q = p;
                        if q[0] & 4 != 0 {
                            for v in q.iter_mut().take(be.shape.num) {
                                *v ^= 7;
                            }
                        }
                        if q[0] & 0x20 != 0 {
                            for v in q.iter_mut().take(be.shape.num) {
                                *v ^= 0x38;
                            }
                        }
                        let on_diagonal = (0..eis[0].norm[0] as usize).all(|i| encode::OFF_DIAG[q[i]] == 0);
                        if on_diagonal {
                            for v in q.iter_mut().take(be.shape.num) {
                                *v = encode::FLIP_DIAG[*v] as usize;
                            }
                            values[f][encode::encode(&mut q, &eis[f], &be.shape, enc) as usize].store(sym, Ordering::Relaxed);
                        }
                    }
                }
            });
        }
    });
    let mut out: Vec<Vec<u8>> = values.into_iter().map(|v| v.into_iter().map(|a| a.into_inner()).collect()).collect();
    if !RAW_STREAMS.load(Ordering::Relaxed) {
        for stream in &mut out {
            fill_dont_care(stream);
        }
    }
    (out, sizes)
}

/// Diagnostics (card MT-8): `side_streams` leaves the free symbols unfilled.
static RAW_STREAMS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The streams of one side in a given order (one per pawn file), unfilled
/// (DONT_CARE, BOUNDED_FLAG + bound) or filled: a diagnostic of the packing
/// bench against Syzygy's decoded streams.
pub fn side_streams(src: &Source, side: usize, o: &Order, filled: bool, workers: usize) -> Vec<Vec<u8>> {
    let (first, second) = counts_of(&src.table.mat);
    let be = BaseEntry::describe(name_of(&first, &second), &first, &second).expect("syzygy: too many pieces");
    let files = if be.has_pawns { 4 } else { 1 };
    RAW_STREAMS.store(!filled, Ordering::Relaxed);
    let (out, _) = build_side(src, &be, side, o, files, workers);
    RAW_STREAMS.store(false, Ordering::Relaxed);
    out
}

/// Judges a candidate order of one side: windows spread over every file's
/// stream are enumerated with the decoder, looked up in the tables, filled
/// like the real stream and packed; returns the packed bytes per file.
#[allow(clippy::too_many_arguments)]
fn sample_side(src: &Source, be: &BaseEntry, side: usize, o: &Order, files: usize, bo: &BuildOptions, opt: pairs::Options, workers: usize) -> Vec<usize> {
    let set = src.set;
    let enc = if be.has_pawns { ENC_FILE } else { ENC_PIECE };
    let num = be.shape.num;
    let pieces: Vec<Piece> = o.pieces.iter().map(|&c| piece_of_code(c)).collect();
    let mut scores = vec![0usize; files];
    for (f, score) in scores.iter_mut().enumerate() {
        let mut ei = EncInfo::default();
        ei.pieces[..o.pieces.len()].copy_from_slice(&o.pieces);
        let size = encode::setup(&mut ei, &be.shape, o.order, o.order2, f, enc);
        let d = super::decode::Decoder::new(be, &ei, enc, f);
        let mut window = bo.window as u64;
        let mut num_windows = (bo.sample_target / bo.window).max(1) as u64;
        if window * num_windows >= size {
            window = size;
            num_windows = 1;
        }
        let sample: Vec<std::sync::atomic::AtomicU8> = (0..window * num_windows).map(|_| std::sync::atomic::AtomicU8::new(DONT_CARE)).collect();
        let next = AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..workers.max(1).min(num_windows as usize) {
                let (d, pieces, sample, next) = (&d, &pieces, &sample, &next);
                s.spawn(move || {
                    let mut p = [0usize; MAX_PIECES];
                    let mut sq = vec![Pos(0); num];
                    let mut buf = new_buffer();
                    loop {
                        let wi = next.fetch_add(1, Ordering::Relaxed) as u64;
                        if wi >= num_windows {
                            return;
                        }
                        let start = if num_windows > 1 { wi * (size - window) / (num_windows - 1) } else { 0 };
                        for i in 0..window {
                            d.squares(start + i, &mut p[..num]);
                            for j in 0..num {
                                sq[j] = Pos((p[j] ^ 0x38) as i8);
                            }
                            let b = Board::from_pieces(side == 0, &sq, pieces);
                            let slot = &sample[(wi * window + i) as usize];
                            if b.opponent_in_check() {
                                continue;
                            }
                            let Some(v) = set.lookup(&b) else { continue }; // adjacent kings
                            slot.store(src.symbol_at(&b, &mut buf, v), Ordering::Relaxed);
                        }
                    }
                });
            }
        });
        let mut values: Vec<u8> = sample.into_iter().map(|a| a.into_inner()).collect();
        fill_dont_care(&mut values);
        let mut po = opt;
        if bo.sample_symbols > 0 {
            po.max_symbols = bo.sample_symbols;
        }
        let (_, st) = pairs::pack_stats(&values, src.alphabet, po);
        *score = st.bytes;
    }
    scores
}

/// Appends the Syzygy squares of every piece with the code c (ascending
/// bitboard square order, as fill_squares lists them) to p at i.
fn squares_of(c: u8, pieces: &[Piece], sq: &[Pos], p: &mut [usize], mut i: usize) -> usize {
    let start = i;
    for (k, &pc) in pieces.iter().enumerate() {
        if code_of(pc) == c {
            p[i] = sq[k].0 as usize;
            i += 1;
        }
    }
    p[start..i].sort_unstable();
    for v in p[start..i].iter_mut() {
        *v ^= 0x38;
    }
    i
}

/// Derives, builds and writes the own WDL table of an exact egtb table.
pub fn write_wdl(set: &Set, t: &Table, path: &Path, opt: pairs::Options, bo: BuildOptions, workers: usize, progress: Option<&dyn Fn(&str)>) -> std::io::Result<Built> {
    let src = Source::new(set, t, workers);
    let b = build(&src, opt, bo, workers, progress);
    b.write(path)?;
    Ok(b)
}
