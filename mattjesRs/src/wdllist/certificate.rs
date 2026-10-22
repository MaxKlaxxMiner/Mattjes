//! The certificate builder, the draw side of the search: it builds the
//! defender's safe set directly instead of hunting it with proof numbers.
//! Port of `mattjesGo/wdllist/certificate.go`, see there.

use std::time::Instant;

use super::*;
use crate::chess::{Piece, Pos};
use crate::nnue;

impl<'a> Graph<'a> {
    /// Orders a defender's moves (see the Go package for the rationale):
    /// known holds and terminal draws first, known attacker wins and mates
    /// last, then stay put, leave nothing en prise, keep the pieces glued to
    /// the king, prefer known positions and used placements, captures.
    fn policy_score(&self, b: &Board, m: Move, acc: Option<(&nnue::Accumulator, &nnue::Net)>) -> i32 {
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
        match self.probe(&c) {
            Verdict::Draw => return 100000, // dead material, or a table draw
            Verdict::Loss => return 100000, // the attacker to move loses by the table
            Verdict::Win => return -100000, // the attacker to move wins by the table
            Verdict::Unknown => {}
        }
        let mut buf = new_buffer();
        let n = c.gen_moves(&mut buf);
        if n == 0 {
            return if c.in_check() { -100000 } else { 100000 };
        }
        for &am in &buf[..n] {
            if c.squares[am.to.idx()] != Piece::NONE && self.capture_free(&c, am) {
                s -= 2000; // the attacker wins material for free
            }
        }
        s -= 10 * n as i32; // the attacker's mobility
        if c.in_check() {
            // a check leaves the attacker only king moves, captures and
            // blocks: the certificate of a perpetual check is the loop of
            // checks, and it must beat the glue and stay-put terms below
            s += 1500;
        }
        if b.squares[m.to.idx()] != Piece::NONE {
            s += 300; // the defender captures
        }
        s -= 300 * self.displacement(&c);
        s += 400 * self.glued_pieces(&c);
        s += self.defender_configs.as_ref().map_or(0, |cfg| cfg.get(&self.defender_config(&c)).copied().unwrap_or(0).min(200) as i32);
        if let Some((acc, net)) = acc {
            // Patricia's view of the child from the attacker's side (it is
            // to move): the defender wants it as low as possible; one
            // centipawn is one point of the policy
            let d = nnue::Delta::of(b, m);
            let mut child = nnue::Accumulator::default();
            child.update(net, acc, &d);
            s -= self.net_weight * child.evaluate(net, c.white_move);
        }
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

    pub(super) fn has_child(&self, i: u32, c: u32) -> bool {
        let mut e = self.first_child[i as usize];
        while e != NO_EDGE {
            if self.cedges[e as usize].node == c {
                return true;
            }
            e = self.cedges[e as usize].next;
        }
        false
    }

    /// Every legal move of the node is a child edge.
    fn fully_tried(&self, i: u32) -> bool {
        self.flags[i as usize] & FL_EXPANDED != 0 && self.nchildren[i as usize] >= self.nmoves[i as usize]
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
            let mut e = self.first_child[i as usize];
            while e != NO_EDGE {
                let c = self.cedges[e as usize].node;
                e = self.cedges[e as usize].next;
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
impl<'a> Graph<'a> {
    /// The capture m on board b wins material for free: after it the side
    /// then to move can neither take back on the same square (a one-ply
    /// exchange check) nor is the position a draw on sight (stalemate, dead
    /// material or a table draw: in the KQ-KP stalemate trick Qxc2 after
    /// Ka1 is stalemate, and the policy preferred c1=Q+ over Ka1 because
    /// the pawn "hung").
    fn capture_free(&self, b: &Board, m: Move) -> bool {
        let mut c = *b;
        c.do_move(m);
        match self.probe(&c) {
            Verdict::Draw | Verdict::Loss => return false, // the capturer gets nothing (or loses by the table)
            Verdict::Win => return true,
            Verdict::Unknown => {}
        }
        let mut buf = new_buffer();
        let n = c.gen_moves(&mut buf);
        if n == 0 {
            return c.in_check(); // mate wins, stalemate is a draw
        }
        !buf[..n].iter().any(|r| r.to == m.to)
    }
}

fn chebyshev(a: i32, b: i32) -> i32 {
    ((a & 7) - (b & 7)).abs().max(((a >> 3) - (b >> 3)).abs())
}

impl<'a> Searcher<'a> {
    /// Adds the next k untried policy moves of a defender node (generation
    /// on the first call); false when the position budget is used up
    /// (checked before the node is touched, as in expand). When the
    /// defender is the weaker side, a node not chased yet (or cut by the
    /// budget) is chased first with CHASE_NODE_BUDGET: a closing check
    /// chain is its move, the policy only steps in where the chain fails
    /// or is cut.
    fn expand_partial(&mut self, i: u32, k: usize) -> bool {
        if self.g.full() {
            self.aborted = true;
            return false;
        }
        let b = self.g.board(i);
        if self.chase_nodes && self.g.chaseable(i) && self.g.defender_chases(&b) {
            match self.chase_top(i, CHASE_NODE_BUDGET) {
                ChaseResult::Hold => return true,
                ChaseResult::Cut if self.aborted => return false,
                _ => {}
            }
            if self.g.status[i as usize] != ST_UNKNOWN {
                return true;
            }
        }
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        if self.g.flags[i as usize] & FL_EXPANDED == 0 {
            self.g.flags[i as usize] |= FL_EXPANDED;
            self.g.expansions += 1;
        }
        let moves = &buf[..n];
        let net = if self.opt.net_weight > 0 { Some(nnue::load(nnue::phase_of(&b))) } else { None };
        let mut acc = nnue::Accumulator::default();
        if let Some(net) = net {
            acc.refresh(net, &b);
        }
        let scores: Vec<i32> = moves.iter().map(|&m| self.g.policy_score(&b, m, net.map(|n| (&acc, n)))).collect();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &c| scores[c].cmp(&scores[a]));
        let mut added = 0;
        for &j in &order {
            if added >= k {
                break;
            }
            let mut c = b;
            c.do_move(moves[j]);
            let before = self.g.nchildren[i as usize];
            if self.link(i, &c).is_none() {
                return false;
            }
            if self.g.nchildren[i as usize] > before {
                added += 1;
            }
        }
        true
    }

    /// Expands breadth-first from the frontier until no open node is left:
    /// attacker nodes completely, defender nodes with one policy move (one
    /// more when they are in the frontier again). A node is queued once per
    /// wave (FL_REGION). A defender node the chase has already given its
    /// move (edges, but reached by a wave, not by the frontier) keeps it.
    fn closure(&mut self, frontier: Vec<u32>) {
        let mut queue = frontier;
        let mut next: Vec<u32> = Vec::new();
        let mut from_frontier = true;
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
                } else {
                    if self.g.chase[i as usize] == CH_HOLD || !from_frontier && self.g.flags[i as usize] & FL_EXPANDED != 0 {
                        continue; // chased: it has its move
                    }
                    if !self.expand_partial(i, 1) {
                        break;
                    }
                }
                for idx in before..self.g.recs.len() {
                    self.push_once(&mut next, idx as u32);
                }
                // an old child reached again that is still open and unexpanded
                let mut e = self.g.first_child[i as usize];
                while e != NO_EDGE {
                    let c = self.g.cedges[e as usize].node;
                    e = self.g.cedges[e as usize].next;
                    if self.g.flags[c as usize] & FL_EXPANDED == 0 {
                        self.push_once(&mut next, c);
                    }
                }
            }
            if self.aborted {
                break; // the rest of the queue stays marked until below
            }
            queue = std::mem::take(&mut next);
            from_frontier = false;
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

impl<'a> Graph<'a> {
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
            let mut e = self.first_child[i as usize];
            while e != NO_EDGE {
                let c = self.cedges[e as usize].node;
                e = self.cedges[e as usize].next;
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

impl<'a> Searcher<'a> {
    /// Sets up the certificate builder of a question.
    pub(crate) fn new_cert(root: &Board, side_to_move_attacks: bool, opt: Options<'a>) -> Searcher<'a> {
        let mut g = Graph::new(root, side_to_move_attacks, false, opt.max_positions, true, opt.oracle);
        g.shape = opt.progress.clone();
        let no_chase = opt.no_chase;
        let mut s = Searcher::new(g, opt);
        s.start = Instant::now();
        s.cond_lists = vec![Vec::new(); CHASE_MAX_DEPTH + 2];
        // a perpetual check is the weaker side's resource: only such policy
        // nodes are chased (defender_chases, per node; a stronger defender's
        // checks are an attack, and chasing them cost millions of positions
        // on KQ-KRRPP)
        s.chase_nodes = !no_chase;
        s.g.net_weight = s.opt.net_weight;
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
        let no_chase = s.opt.no_chase;
        while !s.aborted {
            s.rounds += 1;
            // the chase from the question node first: a perpetual check is
            // found as a chain, and after a rejected hold the chain is
            // repaired from the top (the leak is far below the check that
            // was wrong, the widening at the leak would never reach it)
            if !no_chase && s.g.status[n as usize] == ST_UNKNOWN && s.g.chaseable(n) && s.g.defender_has_piece(q) {
                let res = s.chase_top(n, 0);
                let line = format!("cert chase from the question node: {}, {} chains, {} chase nodes, {} positions, {}", res, s.chases, s.g.chase_visits, s.g.recs.len(), fmt_dur(s.start.elapsed()));
                s.report(&line);
                if s.aborted {
                    break;
                }
                if s.g.attacker_won(n) || s.g.defender_holds(n) {
                    break;
                }
                s.frontier = s.g.open_unexpanded(n);
            }
            let frontier = std::mem::take(&mut s.frontier);
            s.closure(frontier);
            if s.aborted {
                break;
            }
            if s.fixed_point() {
                s.aborted = true;
                break;
            }
            if s.g.attacker_won(n) || s.g.defender_holds(n) {
                break;
            }
            if s.stop_requested() {
                s.aborted = true;
                break;
            }
            let forgotten = s.g.reset_chase();
            s.frontier = s.g.unsafe_defenders(n);
            let (mut won, mut safe) = (0, 0);
            for i in 0..s.g.status.len() {
                if s.g.attacker_won(i as u32) {
                    won += 1;
                } else if s.g.status[i] == ST_SAFE {
                    safe += 1;
                }
            }
            let line = format!("cert round {}: {} positions, {} expansions, {} edges, {} won, {} safe, {} defender nodes widened, {} chains, {} chase nodes, {} holds forgotten, {}", s.rounds, s.g.recs.len(), s.g.expansions, s.g.edges, won, safe, s.frontier.len(), s.chases, s.g.chase_visits, forgotten, fmt_dur(s.start.elapsed()));
            s.report(&line);
            if s.frontier.is_empty() {
                s.aborted = true; // cannot happen: an unsafe root has an unsafe defender with untried moves below it
                s.stuck = true;
                break;
            }
        }
        r.rounds = s.rounds;
        r.chases = s.chases;
        r.resets = s.resets;
        r.stuck = s.stuck;
        s.g.fill_result(&mut r, n);
        // a decision stands even when the run was cut off afterwards (the
        // other racer's stop reaches a chase whose structure the fixed
        // point then confirms)
        r.aborted = s.aborted && r.outcome == Outcome::Undecided;
        r
    }
}
