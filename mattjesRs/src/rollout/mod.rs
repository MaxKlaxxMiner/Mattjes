//! Plays a position out with Patricia's network as the policy of both sides
//! until the tables, a mate, a stalemate, dead material, a repetition or
//! the length cap end it (port of `mattjesGo/rollout`, see there). The
//! result is a guess, never a proof.

use crate::bitboard::Board;
use crate::chess::{new_buffer, Move, Piece};
use crate::mateab::{Material, Oracle, Tables, Verdict};
use crate::nnue;
use crate::tt::Key;

/// Outcome of a rollout for the side to move at its start.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The length cap.
    Unknown = 0,
    Win = 1,
    Draw = 2,
    Loss = 3,
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Outcome::Unknown => "unknown",
            Outcome::Win => "win",
            Outcome::Draw => "draw",
            Outcome::Loss => "loss",
        })
    }
}

/// Reason the rollout ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reason {
    Length,
    Table,
    Mate,
    Stalemate,
    Repetition,
}

impl std::fmt::Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Reason::Length => "length",
            Reason::Table => "table",
            Reason::Mate => "mate",
            Reason::Stalemate => "stalemate",
            Reason::Repetition => "repetition",
        })
    }
}

/// Options of a rollout.
#[derive(Clone, Copy, Default)]
pub struct Options<'a> {
    /// Ends the rollout without a verdict (0 = DEFAULT_MAX_PLIES).
    pub max_plies: usize,
    /// Decides positions on sight (None = dead material only).
    pub oracle: Option<Tables<'a>>,
    /// Depth of the move choice: 1 (0 counts as 1) takes the child the net
    /// likes best, 2 the child whose worst reply the net likes best
    /// (minimax over two plies).
    pub depth: usize,
}

/// Scores a child c (the opponent to move) for the mover by the opponent's
/// best reply: the smallest leaf score over the replies, where a leaf is
/// the net's view for the mover (to move again) plus twice what it can
/// capture for free; a reply that mates or wins by the tables refutes the
/// child, a reply into a table draw scores zero.
fn reply_min(c: &Board, acc: &nnue::Accumulator, net: &nnue::Net, oracle: &Option<Tables<'_>>, evals: &mut usize) -> i32 {
    let mut buf = new_buffer();
    let n = c.gen_moves(&mut buf);
    let mut worst = 1 << 29;
    for &m in &buf[..n] {
        let mut g = *c;
        g.do_move(m);
        let score = match probe(oracle, &g) {
            Verdict::Win => 1 << 29, // the mover to move wins: the opponent will not play it
            Verdict::Loss => return -(1 << 29), // the mover loses: the child is refuted
            Verdict::Draw => 0,
            Verdict::Unknown => {
                if !g.has_moves() {
                    if g.in_check() {
                        return -(1 << 29); // the mover is mated
                    }
                    0 // stalemate
                } else {
                    let d = nnue::Delta::of(c, m);
                    let mut leaf = nnue::Accumulator::default();
                    leaf.update(net, acc, &d);
                    *evals += 1;
                    leaf.evaluate(net, g.white_move) + 2 * hanging(&g)
                }
            }
        };
        worst = worst.min(score);
    }
    worst
}

/// Result of a rollout.
pub struct Result {
    pub outcome: Outcome,
    pub reason: Reason,
    /// The moves played.
    pub line: Vec<Move>,
    /// Children evaluated by the network.
    pub evals: usize,
}

/// The length cap when Options.max_plies is 0.
pub const DEFAULT_MAX_PLIES: usize = 200;

/// Piece value by bitboard kind (king 0, queen, rook, bishop, knight, pawn).
const PIECE_VALUE: [i32; 6] = [0, 900, 500, 300, 300, 100];

fn kind_idx(p: Piece) -> usize {
    (p.0 & Piece::TYPE_MASK.0).trailing_zeros() as usize
}

/// The material the side to move in b wins with its best capture (the
/// captured piece less the capturer when the capture can be answered on
/// the same square): a one-ply exchange estimate, 0 when nothing hangs.
fn hanging(b: &Board) -> i32 {
    let mut buf = new_buffer();
    let n = b.gen_moves(&mut buf);
    let mut best = 0;
    for &m in &buf[..n] {
        if m.capture == Piece::NONE {
            continue;
        }
        let mut gain = PIECE_VALUE[kind_idx(m.capture)];
        let mut c = *b;
        c.do_move(m);
        let mut reply = new_buffer();
        let rn = c.gen_moves(&mut reply);
        if reply[..rn].iter().any(|r| r.to == m.to) {
            gain -= PIECE_VALUE[kind_idx(b.squares[m.from.idx()])];
        }
        best = best.max(gain);
    }
    best
}

fn probe(oracle: &Option<Tables<'_>>, b: &Board) -> Verdict {
    match oracle {
        Some(t) => t.probe(b).0,
        None => Material.probe(b).0,
    }
}

/// Plays root out and reports the outcome for the side to move in root.
pub fn play(root: &Board, opt: Options<'_>) -> Result {
    let max_plies = if opt.max_plies == 0 { DEFAULT_MAX_PLIES } else { opt.max_plies };
    let mut r = Result { outcome: Outcome::Unknown, reason: Reason::Length, line: Vec::new(), evals: 0 };
    let mut b = *root;
    let mut keys: Vec<Key> = Vec::with_capacity(max_plies + 1);
    // the verdict of a terminal for the root's side: v is for the side to
    // move in b, which is the root's side after an even number of plies
    let root_view = |v: Verdict, ply: usize| match v {
        Verdict::Draw => Outcome::Draw,
        Verdict::Win => {
            if ply.is_multiple_of(2) {
                Outcome::Win
            } else {
                Outcome::Loss
            }
        }
        Verdict::Loss => {
            if ply.is_multiple_of(2) {
                Outcome::Loss
            } else {
                Outcome::Win
            }
        }
        Verdict::Unknown => Outcome::Unknown,
    };
    let mut acc = nnue::Accumulator::default();
    let mut net: Option<&nnue::Net> = None;
    let mut ply = 0;
    loop {
        let v = probe(&opt.oracle, &b);
        if v != Verdict::Unknown {
            r.outcome = root_view(v, ply);
            r.reason = Reason::Table;
            return r;
        }
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        if n == 0 {
            if b.in_check() {
                r.outcome = root_view(Verdict::Loss, ply);
                r.reason = Reason::Mate;
            } else {
                r.outcome = Outcome::Draw;
                r.reason = Reason::Stalemate;
            }
            return r;
        }
        if keys.contains(&b.key) {
            r.outcome = Outcome::Draw;
            r.reason = Reason::Repetition;
            return r;
        }
        keys.push(b.key);
        if ply >= max_plies {
            return r;
        }
        // the accumulator is carried along the line; a change of the net
        // (the material crossed the phase bound) refreshes it
        let phase_net = nnue::load(nnue::phase_of(&b));
        if net.is_none_or(|nt| !std::ptr::eq(nt, phase_net)) {
            net = Some(phase_net);
            acc.refresh(phase_net, &b);
        }
        let net = phase_net;
        let mut best = Move::NONE;
        let mut best_score = i32::MIN;
        let mut best_acc = nnue::Accumulator::default();
        for &m in &buf[..n] {
            let mut c = b;
            c.do_move(m);
            let mut child = nnue::Accumulator::default();
            let score = match probe(&opt.oracle, &c) {
                Verdict::Loss => 1 << 29, // the opponent to move loses: the table win
                Verdict::Win => continue, // the opponent wins: never
                Verdict::Draw => 0,
                Verdict::Unknown => {
                    if !c.has_moves() {
                        if c.in_check() {
                            1 << 29 // mate
                        } else {
                            0 // stalemate: a draw
                        }
                    } else {
                        let d = nnue::Delta::of(&b, m);
                        child.update(net, &acc, &d);
                        let mut s = if opt.depth >= 2 {
                            // the opponent's best reply decides (minimax over
                            // two plies, the net at the leaves from the
                            // mover's view)
                            reply_min(&c, &child, net, &opt.oracle, &mut r.evals)
                        } else {
                            r.evals += 1;
                            // the opponent's view, negated; the static view
                            // does not see a hanging piece
                            -child.evaluate(net, c.white_move) - 2 * hanging(&c)
                        };
                        // a position of the line again only when nothing
                        // else is left
                        if keys.contains(&c.key) {
                            s -= 1 << 28;
                        }
                        s
                    }
                }
            };
            if score > best_score {
                best = m;
                best_score = score;
                best_acc = child;
            }
        }
        if best_score == i32::MIN {
            // every move loses by the tables: take the first, the next ply ends it
            best = buf[0];
            let d = nnue::Delta::of(&b, best);
            best_acc.update(net, &acc, &d);
        }
        r.line.push(best);
        b.do_move(best);
        acc = best_acc;
        ply += 1;
    }
}
