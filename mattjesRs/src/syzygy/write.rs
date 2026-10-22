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

/// Marks a position whose stored value does not matter; BOUNDED+v marks one
/// whose stored value may be anything up to v.
pub const DONT_CARE: u8 = 0xff;
const BOUNDED: u8 = 3;

/// The WDL of a material taken from an exact egtb table: per index the
/// symbol (0 loss, 1 draw, 2 win for the side to move), DONT_CARE for dead
/// and illegal indices, or BOUNDED+symbol for positions whose best capture
/// already achieves their value (the probe takes the maximum of the best
/// capture and the stored value).
pub struct Source<'a> {
    pub table: &'a Table,
    pub symbols: Vec<u8>,
    pub legal: usize,
    pub decided: usize,
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
    /// Derives the symbols from a table with exact values in memory, in parallel.
    pub fn new(set: &Set, t: &'a Table, workers: usize) -> Source<'a> {
        assert!(!t.values.is_empty() && t.is_exact(), "syzygy: Source needs an exact table in memory");
        let pieces = pieces_of(&t.mat);
        let n = t.size;
        let symbols: Vec<std::sync::atomic::AtomicU8> = (0..n).map(|_| std::sync::atomic::AtomicU8::new(DONT_CARE)).collect();
        let legal = AtomicUsize::new(0);
        let decided = AtomicUsize::new(0);
        let chunk = n.div_ceil(workers.max(1));
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
                        let mut own = symbol_of(t.value(idx));
                        let (mut best, mut has_capture) = (0u8, false);
                        let nm = b.gen_moves(&mut buf);
                        for &m in &buf[..nm] {
                            if m.capture == Piece::NONE {
                                continue;
                            }
                            let mut c = b;
                            c.do_move(m);
                            let v = set.lookup(&c).expect("syzygy: no table after a capture");
                            has_capture = true;
                            best = best.max(2 - symbol_of(v));
                        }
                        if has_capture && best == own {
                            nd += 1;
                            own += BOUNDED;
                        }
                        slot.store(own, Ordering::Relaxed);
                    }
                    legal.fetch_add(nl, Ordering::Relaxed);
                    decided.fetch_add(nd, Ordering::Relaxed);
                });
            }
        });
        Source { table: t, symbols: symbols.into_iter().map(|a| a.into_inner()).collect(), legal: legal.into_inner(), decided: decided.into_inner() }
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
}

impl Built {
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
        let mut w = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(path)?);
        w.write_all(&head)?;
        for s in &self.streams {
            w.write_all(&s.packed)?;
            let pad = ((s.packed.len() + 7) & !7) - s.packed.len();
            w.write_all(&[0u8; 8][..pad])?;
        }
        w.flush()
    }
}

pub(super) const OWN_MAGIC: &[u8; 4] = b"MWDL";
pub(super) const OWN_VERSION: u8 = 1;
pub(super) const OWN_HEADER: usize = 32;
pub(super) const OWN_DESCRIPTOR: usize = 32;

/// Packs the WDL of a source, trying every candidate order per side and
/// keeping the smallest stream per side and file.
pub fn build(src: &Source, opt: pairs::Options, workers: usize, progress: Option<&dyn Fn(&str)>) -> Built {
    let (first, second) = counts_of(&src.table.mat);
    let be = BaseEntry::describe(name_of(&first, &second), &first, &second).expect("syzygy: too many pieces");
    let sides = if be.symmetric { 1 } else { 2 };
    let files = if be.has_pawns { 4 } else { 1 };
    let cands = candidates(&be, &first, &second);
    let mut streams: Vec<Option<Stream>> = (0..sides * files).map(|_| None).collect();
    for side in 0..sides {
        for o in &cands {
            let (values, sizes) = build_side(src, &be, side, o, files, workers);
            for f in 0..files {
                let (packed, st) = pairs::pack_stats(&values[f], 3, opt);
                let slot = &mut streams[side * files + f];
                if slot.as_ref().is_none_or(|s| packed.len() < s.packed.len()) {
                    *slot = Some(Stream { order: o.clone(), size: sizes[f], packed, stats: st });
                }
                if let Some(p) = progress {
                    let s = slot.as_ref().unwrap();
                    p(&format!("{} side {} file {} order {:<12} {:>8} bytes ({} tokens, {} symbols)", be.name, side, f, o, s.packed.len(), st.tokens, st.symbols));
                }
            }
        }
    }
    Built { name: be.name.clone(), first, second, sides, files, streams: streams.into_iter().map(|s| s.unwrap()).collect(), candidates: cands.len() }
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
    for stream in &mut out {
        let mut prev = 1u8;
        for v in stream.iter_mut() {
            if *v == DONT_CARE {
                *v = prev;
            } else if *v >= BOUNDED {
                if prev > *v - BOUNDED {
                    prev = *v - BOUNDED;
                }
                *v = prev;
            } else {
                prev = *v;
            }
        }
    }
    (out, sizes)
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
pub fn write_wdl(set: &Set, t: &Table, path: &Path, opt: pairs::Options, workers: usize, progress: Option<&dyn Fn(&str)>) -> std::io::Result<Built> {
    let src = Source::new(set, t, workers);
    let b = build(&src, opt, workers, progress);
    b.write(path)?;
    Ok(b)
}
