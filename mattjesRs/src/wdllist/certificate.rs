//! The certificate builder, the draw side of the search: it builds the
//! defender's safe set directly instead of hunting it with proof numbers.
//! Port of `mattjesGo/wdllist/certificate.go`, see there.

use std::time::Instant;

use super::*;
use crate::chess::{Piece, Pos};

impl Graph {
    /// Orders a defender's moves (see the Go package for the rationale):
    /// known holds and terminal draws first, known attacker wins and mates
    /// last, then stay put, leave nothing en prise, keep the pieces glued to
    /// the king, prefer known positions and used placements, captures.
    fn policy_score(&self, b: &Board, m: Move) -> i32 {
        let mut c = *b;
        c.do_move(m);
        let mut s = 0i32;
        if let Some(idx) = self.index(&c) {
            if self.attacker_won(idx) {
                return -100000;
            }
            if self.defender_holds(idx) {
                return 100000;
            }
            s += 500;
        }
        if Material.probe(&c).0 == Verdict::Draw {
            return 100000; // dead material
        }
        let mut buf = new_buffer();
        let n = c.gen_moves(&mut buf);
        if n == 0 {
            return if c.in_check() { -100000 } else { 100000 };
        }
        for &am in &buf[..n] {
            if c.squares[am.to.idx()] != Piece::NONE && !recapturable(&c, am) {
                s -= 2000; // the attacker wins material for free
            }
        }
        s -= 10 * n as i32; // the attacker's mobility
        if b.squares[m.to.idx()] != Piece::NONE {
            s += 300; // the defender captures
        }
        s -= 300 * self.displacement(&c);
        s += 400 * self.glued_pieces(&c);
        s += self.defender_configs.as_ref().map_or(0, |cfg| cfg.get(&self.defender_config(&c)).copied().unwrap_or(0).min(200) as i32);
        s
    }

    /// The defender's pieces other than the king that stand next to the king.
    fn glued_pieces(&self, b: &Board) -> i32 {
        let side = self.defender_side();
        let king = b.pieces[side][0].trailing_zeros() as i32;
        let mut n = 0;
        for kind in 1..6 {
            let mut pieces = b.pieces[side][kind];
            while pieces != 0 {
                if chebyshev(pieces.trailing_zeros() as i32, king) == 1 {
                    n += 1;
                }
                pieces &= pieces - 1;
            }
        }
        n
    }

    /// The Chebyshev distance of the defender's king from its root square.
    fn displacement(&self, b: &Board) -> i32 {
        chebyshev(b.pieces[self.defender_side()][0].trailing_zeros() as i32, self.root_defender[0].trailing_zeros() as i32)
    }

    /// Diagnostic: the defender king's squares over a sample of the graph.
    fn king_squares(&self) -> String {
        let side = self.defender_side();
        let mut hist = [0usize; 64];
        let mut i = 0;
        while i < self.recs.len() {
            let b = self.board(i as u32);
            hist[b.pieces[side][0].trailing_zeros() as usize] += 1;
            i += 97;
        }
        let mut top: Vec<(usize, usize)> = hist.iter().enumerate().filter(|(_, &n)| n > 0).map(|(sq, &n)| (sq, n)).collect();
        top.sort_by_key(|e| std::cmp::Reverse(e.1));
        let mut out = String::new();
        for (i, (sq, n)) in top.iter().enumerate() {
            if i == 6 {
                out += " ...";
                break;
            }
            out += &format!(" {}:{}", Pos(*sq as i8), n);
        }
        out
    }

    fn has_child(&self, i: u32, c: u32) -> bool {
        self.children[i as usize].contains(&c)
    }

    /// Every legal move of the node is a child edge.
    fn fully_tried(&self, i: u32) -> bool {
        self.flags[i as usize] & FL_EXPANDED != 0 && self.children[i as usize].len() >= self.nmoves[i as usize] as usize
    }

    /// Collects the defender nodes that still have untried moves, reachable
    /// from the root through unsafe open nodes, closest to the leak first
    /// (unsafe_level within two of the smallest).
    fn unsafe_defenders(&mut self, from: u32) -> Vec<u32> {
        let mut all = Vec::new();
        let mut stack = vec![from];
        let mut marked = vec![from];
        self.flags[from as usize] |= FL_REGION;
        let mut min_level = u16::MAX;
        let levels = self.unsafe_level.as_ref().expect("certificate graph");
        while let Some(i) = stack.pop() {
            if !self.attacker_at(i) && !self.fully_tried(i) {
                all.push(i);
                min_level = min_level.min(levels[i as usize]);
            }
            for ci in 0..self.children[i as usize].len() {
                let c = self.children[i as usize][ci];
                if self.flags[c as usize] & FL_REGION != 0 || self.status[c as usize] != ST_UNKNOWN {
                    continue;
                }
                self.flags[c as usize] |= FL_REGION;
                marked.push(c);
                stack.push(c);
            }
        }
        for &i in &marked {
            self.flags[i as usize] &= !FL_REGION;
        }
        all.into_iter().filter(|&i| levels[i as usize] <= min_level.saturating_add(2)).collect()
    }
}

/// After the capture m on board b, the side then to move can take back on
/// the same square (a one-ply exchange check).
fn recapturable(b: &Board, m: Move) -> bool {
    let mut c = *b;
    c.do_move(m);
    let mut buf = new_buffer();
    let n = c.gen_moves(&mut buf);
    buf[..n].iter().any(|r| r.to == m.to)
}

fn chebyshev(a: i32, b: i32) -> i32 {
    ((a & 7) - (b & 7)).abs().max(((a >> 3) - (b >> 3)).abs())
}

impl Searcher {
    /// Adds the next k untried policy moves of a defender node (generation
    /// on the first call); false when the position budget is used up
    /// (checked before the node is touched, as in expand).
    fn expand_partial(&mut self, i: u32, k: usize) -> bool {
        if self.g.full() {
            self.aborted = true;
            return false;
        }
        let b = self.g.board(i);
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        if self.g.flags[i as usize] & FL_EXPANDED == 0 {
            self.g.flags[i as usize] |= FL_EXPANDED;
            self.g.expansions += 1;
            self.g.children[i as usize] = Vec::with_capacity(n.min(k));
        }
        let moves = &buf[..n];
        let scores: Vec<i32> = moves.iter().map(|&m| self.g.policy_score(&b, m)).collect();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &c| scores[c].cmp(&scores[a]));
        let mut added = 0;
        for &j in &order {
            if added >= k {
                break;
            }
            let mut c = b;
            c.do_move(moves[j]);
            let (ci, is_new, ok) = self.g.get_or_put(c.key);
            if !ok {
                self.aborted = true;
                return false;
            }
            if !is_new && self.g.has_child(i, ci) {
                continue;
            }
            if is_new {
                self.g.add(&c);
            }
            self.g.children[i as usize].push(ci);
            self.g.parents[ci as usize].push(i);
            self.g.edges += 1;
            added += 1;
            if self.g.status[ci as usize] == ST_LOSS && self.g.status[i as usize] == ST_UNKNOWN {
                self.g.status[i as usize] = ST_WIN;
            } else if self.g.status[ci as usize] == ST_WIN {
                self.g.remaining[i as usize] -= 1;
            }
        }
        if self.g.status[i as usize] == ST_UNKNOWN && self.g.remaining[i as usize] == 0 {
            self.g.status[i as usize] = ST_LOSS;
        }
        if self.g.status[i as usize] != ST_UNKNOWN {
            self.g.propagate(i);
        }
        true
    }

    /// Expands breadth-first from the frontier until no open node is left:
    /// attacker nodes completely, defender nodes with one policy move. A node
    /// is queued once per wave (FL_REGION).
    fn closure(&mut self, frontier: Vec<u32>) {
        let mut queue = frontier;
        let mut next: Vec<u32> = Vec::new();
        while !queue.is_empty() && !self.aborted {
            next = Vec::new();
            for &i in &queue {
                self.g.flags[i as usize] &= !FL_REGION;
                if self.g.status[i as usize] != ST_UNKNOWN {
                    continue;
                }
                let before = self.g.recs.len();
                if self.g.attacker_at(i) {
                    if self.g.flags[i as usize] & FL_EXPANDED != 0 {
                        continue;
                    }
                    self.expand(i);
                    if self.aborted {
                        break;
                    }
                    if self.g.expansions & ((1 << 16) - 1) == 0 {
                        let line = format!("cert closure: {} positions, {} expansions, {} edges, {} defender placements ({}), {}", self.g.recs.len(), self.g.expansions, self.g.edges, self.g.defender_configs.as_ref().map_or(0, |c| c.len()), self.g.king_squares(), fmt_dur(self.start.elapsed()));
                        self.report(&line);
                    }
                } else if !self.expand_partial(i, 1) {
                    break;
                }
                for idx in before..self.g.recs.len() {
                    self.push_once(&mut next, idx as u32);
                }
                // an old child reached again that is still open and unexpanded
                for ci in 0..self.g.children[i as usize].len() {
                    let c = self.g.children[i as usize][ci];
                    if self.g.flags[c as usize] & FL_EXPANDED == 0 {
                        self.push_once(&mut next, c);
                    }
                }
            }
            if self.aborted {
                break; // the rest of the queue stays marked until below
            }
            queue = std::mem::take(&mut next);
        }
        // an interrupted wave leaves its queue and the next one marked
        for &i in queue.iter().chain(next.iter()) {
            self.g.flags[i as usize] &= !FL_REGION;
        }
    }

    fn push_once(&mut self, next: &mut Vec<u32>, c: u32) {
        if self.g.flags[c as usize] & FL_REGION != 0 || self.g.status[c as usize] != ST_UNKNOWN {
            return;
        }
        self.g.flags[c as usize] |= FL_REGION;
        next.push(c);
    }
}

impl Graph {
    /// Collects the open nodes without children reachable from the root
    /// through open nodes: the frontier of a resumed run, whose interrupted
    /// closure lost its queue (the nodes it had added are in the graph,
    /// partially expanded defenders keep their move).
    fn open_unexpanded(&mut self, from: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut stack = vec![from];
        let mut marked = vec![from];
        self.flags[from as usize] |= FL_REGION;
        while let Some(i) = stack.pop() {
            if self.flags[i as usize] & FL_EXPANDED == 0 {
                out.push(i);
                continue;
            }
            for ci in 0..self.children[i as usize].len() {
                let c = self.children[i as usize][ci];
                if self.flags[c as usize] & FL_REGION != 0 || self.status[c as usize] != ST_UNKNOWN {
                    continue;
                }
                self.flags[c as usize] |= FL_REGION;
                marked.push(c);
                stack.push(c);
            }
        }
        for &i in &marked {
            self.flags[i as usize] &= !FL_REGION;
        }
        out
    }
}

impl Searcher {
    /// Sets up the certificate builder of a question.
    pub(crate) fn new_cert(root: &Board, side_to_move_attacks: bool, opt: Options) -> Searcher {
        let g = Graph::new(root, side_to_move_attacks, false, opt.max_positions, true);
        let mut s = Searcher::new(g, opt);
        s.start = Instant::now();
        s
    }

    /// Continues the certificate builder up to `max_positions`.
    pub(crate) fn run_cert(&mut self, q: &Board, extra: usize) -> Result {
        let s = self;
        s.begin(extra);
        let mut r = Result::default();
        let Some(n) = s.locate(q) else {
            r.aborted = true;
            r.positions = s.g.recs.len();
            return r;
        };
        // the stay-put policy measures displacement from the question node
        let side = s.g.defender_side();
        s.g.root_defender = q.pieces[side];
        // the queue of an interrupted closure is gone (and another question
        // may have expanded nodes below this one): start from the open
        // nodes without children below the question node
        s.frontier = s.g.open_unexpanded(n);
        while !s.aborted {
            s.rounds += 1;
            let frontier = std::mem::take(&mut s.frontier);
            s.closure(frontier);
            if s.aborted {
                break;
            }
            s.g.safe_set();
            if s.g.attacker_won(n) || s.g.defender_holds(n) {
                break;
            }
            if s.stop_requested() {
                s.aborted = true;
                break;
            }
            s.frontier = s.g.unsafe_defenders(n);
            let (mut won, mut safe) = (0, 0);
            for i in 0..s.g.status.len() {
                if s.g.attacker_won(i as u32) {
                    won += 1;
                } else if s.g.status[i] == ST_SAFE {
                    safe += 1;
                }
            }
            let line = format!("cert round {}: {} positions, {} expansions, {} edges, {} won, {} safe, {} defender nodes widened, {}", s.rounds, s.g.recs.len(), s.g.expansions, s.g.edges, won, safe, s.frontier.len(), fmt_dur(s.start.elapsed()));
            s.report(&line);
            if s.frontier.is_empty() {
                s.aborted = true; // cannot happen: an unsafe root has an unsafe defender with untried moves below it
                s.stuck = true;
                break;
            }
        }
        r.rounds = s.rounds;
        r.aborted = s.aborted;
        r.stuck = s.stuck;
        s.g.fill_result(&mut r, n);
        r
    }
}
