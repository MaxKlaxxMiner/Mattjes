//! The chase, the perpetual-check searcher of the certificate builder. Port
//! of `mattjesGo/wdllist/chase.go`, see there for the full explanation: the
//! check chain is followed depth-first (the defender with chasing moves
//! only, the attacker with every reply), a repetition on the path closes
//! the chain, and the holds are kept exact with Tarjan-style low links
//! (a hold that relies on an ancestor is memoized only while that ancestor
//! is on the path); failures are memoized for good, cuts with the depth
//! they had left; the fixed point stays the judge. A mixed perpetual gets
//! free escapes from check and CHASE_QUIET_MOVES quiet moves per path
//! (at the top node only with CHASE_QUIET_TOP_ONLY); failures and cuts
//! are memoized with the quiet budget they were tried with.

use super::*;
use crate::chess::Piece;

/// Not chased yet, cut by a budget, or a conditional hold whose ancestor failed: chase again.
pub(super) const CH_UNKNOWN: u8 = 0;
/// A chain from here closes in itself.
pub(super) const CH_HOLD: u8 = 1;
/// A chain from here closes on an ancestor still on the path (chase_depth = its depth).
pub(super) const CH_HOLD_COND: u8 = 2;
/// Every chasing move fails, however the node was reached.
pub(super) const CH_FAIL: u8 = 3;
/// Cut off with chase_depth plies left: chase again only with more.
pub(super) const CH_CUT: u8 = 4;

/// The deepest cap of a chain; a node beyond the cap is cut, not failed.
pub const CHASE_MAX_DEPTH: usize = 256;

/// The first cap of the iterative deepening.
pub const CHASE_FIRST_DEPTH: usize = 16;

/// The chase nodes a chase at a policy node may visit (the chase from the
/// question node is unbounded).
pub const CHASE_NODE_BUDGET: u64 = 1 << 12;

/// The quiet moves the defender may spend on one path of a chase.
pub const CHASE_QUIET_MOVES: usize = 1;

/// Quiet moves only at the node a chase starts from (the chase as the
/// policy's look-ahead for a quiet move; measured against quiet moves
/// anywhere on the path, which made the chase a full-width search).
pub const CHASE_QUIET_TOP_ONLY: bool = true;

/// The quiet moves tried per defender node (the best by the look-ahead).
pub const CHASE_QUIET_WIDTH: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ChaseResult {
    Fail,
    Hold,
    /// The depth cap, the budget or the stop flag: no verdict.
    Cut,
}

impl std::fmt::Display for ChaseResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ChaseResult::Fail => "fail",
            ChaseResult::Hold => "hold",
            ChaseResult::Cut => "cut",
        })
    }
}

/// The low link of a hold that relies on no ancestor.
pub(super) const NO_LOW: usize = CHASE_MAX_DEPTH + 1;

impl<'a> Graph<'a> {
    /// The defender has a queen, rook, bishop or knight on the board, so it
    /// can give check and the chase makes sense; a lone king (or king and
    /// pawns) never chases, and its escapes from check would let the chase
    /// follow the attacker's checks to the cap at every policy node (seen
    /// on KBP-K with the wrong bishop).
    pub(super) fn defender_has_piece(&self, b: &Board) -> bool {
        let side = if self.attacker_white { 1 } else { 0 };
        b.pieces[side][1] | b.pieces[side][2] | b.pieces[side][3] | b.pieces[side][4] != 0
    }

    /// The node holds by the chase memo (for good or on the path).
    fn chase_held(&self, i: u32) -> bool {
        matches!(self.chase[i as usize], CH_HOLD | CH_HOLD_COND)
    }

    /// The node is chased when reached (not chased yet, forgotten, or cut
    /// off with less depth than a chase from the top has).
    pub(super) fn chaseable(&self, i: u32) -> bool {
        matches!(self.chase[i as usize], CH_UNKNOWN | CH_CUT)
    }

    /// Forgets the holds the fixed point did not confirm (none, if the low
    /// links are right: the fixed point is the judge, this is the
    /// insurance) and returns their number.
    pub(super) fn reset_chase(&mut self) -> usize {
        let mut n = 0;
        for i in 0..self.chase.len() {
            if self.chase[i] == CH_HOLD && self.status[i] == ST_UNKNOWN {
                self.chase[i] = CH_UNKNOWN;
                n += 1;
            }
        }
        n
    }
}

/// The number of checking moves of the side to move.
fn check_count(b: &Board) -> i32 {
    let mut buf = new_buffer();
    let n = b.gen_moves(&mut buf);
    buf[..n].iter().filter(|&&m| b.gives_check(m)).count() as i32
}

/// The usual piece count of a side (queen 9, rook 5, bishop and knight 3,
/// pawn 1).
pub(super) fn material(b: &Board, side: usize) -> i32 {
    [0, 9, 5, 3, 3, 1].iter().enumerate().map(|(kind, &value)| value * b.pieces[side][kind].count_ones() as i32).sum()
}

impl<'a> Searcher<'a> {
    /// Chases from node i with caps doubling from CHASE_FIRST_DEPTH to
    /// CHASE_MAX_DEPTH until the result is a verdict (the memo keeps what
    /// the shallower chases settled, only the cut parts are chased deeper);
    /// with a budget (nodes, 0 = none) the chase is cut when it is used up.
    pub(super) fn chase_top(&mut self, i: u32, budget: u64) -> ChaseResult {
        self.chases += 1;
        self.chase_limit = if budget > 0 { self.g.chase_visits + budget } else { 0 };
        let mut cap = CHASE_FIRST_DEPTH;
        let res = loop {
            self.chase_cap = cap.min(CHASE_MAX_DEPTH);
            let (before, nodes_before) = (self.g.recs.len(), self.g.chase_visits);
            let (res, _) = self.chase(i, 0, CHASE_QUIET_MOVES);
            if self.g.chase_visits - nodes_before >= 1 << 14 {
                let line = format!("chase from node {}, cap {}: {}, {} nodes, {} new positions, {} fails, {} cuts, {} holds ({} conditional), {} quiet moves so far, {}", i, self.chase_cap, res, self.g.chase_visits - nodes_before, self.g.recs.len() - before, self.chase_fails, self.chase_cuts, self.chase_holds, self.chase_cond, self.quiet_moves, fmt_dur(self.start.elapsed()));
                self.report(&line);
            }
            if res != ChaseResult::Cut || self.aborted || self.chase_cap == CHASE_MAX_DEPTH || self.chase_limit != 0 && self.g.chase_visits >= self.chase_limit {
                break res;
            }
            cap *= 2;
        };
        self.chase_limit = 0;
        res
    }

    /// Follows the check chain below node i: a defender node needs one
    /// move whose chain closes, an attacker node needs every reply to
    /// close; quiet is the number of quiet moves the defender may still
    /// spend on this path. Returns the result and, for a hold, its low
    /// link (the depth of the highest ancestor its repetitions reach;
    /// NO_LOW if none).
    fn chase(&mut self, i: u32, depth: usize, quiet: usize) -> (ChaseResult, usize) {
        let iu = i as usize;
        if self.g.defender_holds(i) {
            return (ChaseResult::Hold, NO_LOW);
        }
        if self.g.attacker_won(i) {
            return (ChaseResult::Fail, NO_LOW);
        }
        if self.g.flags[iu] & FL_ON_PATH != 0 {
            // a repetition closes the chain on this ancestor (checked before
            // the memo: a node on the path still carries the memo of its
            // last chase, a cut there would hide the repetition)
            return (ChaseResult::Hold, self.g.chase_depth[iu] as usize);
        }
        match self.g.chase[iu] {
            CH_HOLD => return (ChaseResult::Hold, NO_LOW),
            CH_HOLD_COND => return (ChaseResult::Hold, self.g.chase_depth[iu] as usize),
            CH_FAIL if self.g.chase_quiet[iu] as usize >= quiet => return (ChaseResult::Fail, NO_LOW),
            CH_CUT if self.chase_cap - depth <= self.g.chase_depth[iu] as usize && self.g.chase_quiet[iu] as usize >= quiet => return (ChaseResult::Cut, NO_LOW),
            _ => {}
        }
        if self.aborted || depth >= self.chase_cap || self.chase_limit != 0 && self.g.chase_visits >= self.chase_limit {
            return (ChaseResult::Cut, NO_LOW);
        }
        self.g.chase_visits += 1;
        if self.g.chase_visits & ((1 << 20) - 1) == 0 {
            let line = format!("chase: {} nodes, {} positions, {} fails, {} cuts, {} holds ({} conditional), {} quiet moves, depth {}, {}", self.g.chase_visits, self.g.recs.len(), self.chase_fails, self.chase_cuts, self.chase_holds, self.chase_cond, self.quiet_moves, depth, fmt_dur(self.start.elapsed()));
            self.report(&line);
        }
        self.g.flags[iu] |= FL_ON_PATH;
        self.g.chase_depth[iu] = depth as u16;
        self.cond_lists[depth].clear();
        let (res, mut low) = if self.g.attacker_at(i) { self.chase_attacker(i, depth, quiet) } else { self.chase_defender(i, depth, quiet) };
        self.g.flags[iu] &= !FL_ON_PATH;
        let dependents = std::mem::take(&mut self.cond_lists[depth]);
        match res {
            ChaseResult::Hold if low >= depth => {
                self.chase_holds += 1;
                self.g.chase[iu] = CH_HOLD;
                for &d in &dependents {
                    self.g.chase[d as usize] = CH_HOLD;
                }
                low = NO_LOW;
            }
            ChaseResult::Hold => {
                self.chase_cond += 1;
                self.g.chase[iu] = CH_HOLD_COND;
                self.g.chase_depth[iu] = low as u16;
                for &d in &dependents {
                    self.g.chase_depth[d as usize] = low as u16;
                }
                self.cond_lists[low].push(i);
                self.cond_lists[low].extend_from_slice(&dependents);
            }
            ChaseResult::Fail => {
                self.chase_fails += 1;
                self.g.chase[iu] = CH_FAIL;
                self.g.chase_quiet[iu] = quiet as u8;
                for &d in &dependents {
                    self.g.chase[d as usize] = CH_UNKNOWN;
                }
            }
            ChaseResult::Cut => {
                self.chase_cuts += 1;
                self.g.chase[iu] = CH_CUT;
                self.g.chase_depth[iu] = (self.chase_cap - depth) as u16;
                self.g.chase_quiet[iu] = quiet as u8;
                if self.chase_limit != 0 && self.g.chase_visits >= self.chase_limit {
                    self.g.chase[iu] = CH_UNKNOWN; // cut by the budget, not by the depth: chase again at any depth
                }
                for &d in &dependents {
                    self.g.chase[d as usize] = CH_UNKNOWN;
                }
            }
        }
        (res, low)
    }

    /// Every reply of the attacker must close (all replies are looked at
    /// even after a cut, a failing one is a verdict). The replies are tried
    /// failure first: known failures, then the replies that leave the
    /// defender the fewest checks (the escape), new positions before known
    /// ones, the known holds last.
    fn chase_attacker(&mut self, i: u32, depth: usize, quiet: usize) -> (ChaseResult, usize) {
        if self.g.flags[i as usize] & FL_EXPANDED == 0 {
            self.expand(i);
            if self.aborted {
                return (ChaseResult::Cut, NO_LOW);
            }
            if self.g.attacker_won(i) {
                return (ChaseResult::Fail, NO_LOW);
            }
            if self.g.defender_holds(i) {
                return (ChaseResult::Hold, NO_LOW);
            }
        }
        let mut replies: Vec<(u32, i32)> = Vec::with_capacity(self.g.nchildren[i as usize] as usize);
        let mut low = NO_LOW;
        let mut e = self.g.first_child[i as usize];
        while e != NO_EDGE {
            let c = self.g.cedges[e as usize].node;
            e = self.g.cedges[e as usize].next;
            let cu = c as usize;
            if self.g.attacker_won(c) || self.g.chase[cu] == CH_FAIL && self.g.chase_quiet[cu] as usize >= quiet {
                return (ChaseResult::Fail, NO_LOW);
            }
            if self.g.defender_holds(c) || self.g.chase[cu] == CH_HOLD {
                continue; // closes for free
            }
            if self.g.chase[cu] == CH_HOLD_COND || self.g.flags[cu] & FL_ON_PATH != 0 {
                low = low.min(self.g.chase_depth[cu] as usize);
                continue; // closes on an ancestor
            }
            let b = self.g.board(c);
            let mut score = if b.in_check() {
                1000 // a counter-check: the defender escapes, no cheap refutation
            } else {
                2 * check_count(&b)
            };
            if self.g.flags[cu] & FL_EXPANDED != 0 {
                score += 1; // a known position: the defender has been here
            }
            replies.push((c, score));
        }
        replies.sort_by_key(|r| r.1);
        let mut res = ChaseResult::Hold;
        for &(c, _) in &replies {
            let (rres, rlow) = self.chase(c, depth + 1, quiet);
            match rres {
                ChaseResult::Fail => return (ChaseResult::Fail, NO_LOW),
                ChaseResult::Cut => res = ChaseResult::Cut,
                ChaseResult::Hold => low = low.min(rlow),
            }
        }
        (res, low)
    }

    /// Tries the defender's chasing moves in order: moves into positions
    /// that hold (decided, chased or on the path) first, then the checks
    /// (every legal move when the defender is in check) by a two-ply
    /// look-ahead that seeks the closure (a move with a reply that wins for
    /// the attacker is refuted and skipped); when they all fail and the
    /// path still has a quiet move to spend, the best CHASE_QUIET_WIDTH
    /// quiet moves by the same look-ahead follow, scored only then. Every
    /// tried move becomes an edge; a node without a candidate fails at
    /// once and is left untouched (an expanded defender node without
    /// children would count as safe in the fixed point).
    fn chase_defender(&mut self, i: u32, depth: usize, quiet: usize) -> (ChaseResult, usize) {
        if self.g.full() {
            self.aborted = true;
            return (ChaseResult::Cut, NO_LOW);
        }
        let b = self.g.board(i);
        let mut buf = new_buffer();
        let n = b.gen_moves(&mut buf);
        let has_piece = self.g.defender_has_piece(&b);
        let in_check = has_piece && b.in_check(); // escapes are free only for a defender that can chase
        let mut cands: Vec<(Move, i32, usize)> = Vec::with_capacity(n);
        let mut quiet_moves: Vec<Move> = Vec::new();
        for &m in &buf[..n] {
            let mut c = b;
            c.do_move(m);
            let mut score = 0;
            let known = self.g.index(&c);
            if let Some(idx) = known {
                let iu = idx as usize;
                if self.g.attacker_won(idx) {
                    continue;
                }
                if self.g.defender_holds(idx) || self.g.chase[iu] == CH_HOLD {
                    score = 3000;
                } else if self.g.chase[iu] == CH_HOLD_COND {
                    score = 2500;
                } else if self.g.flags[iu] & FL_ON_PATH != 0 {
                    score = 2000;
                }
            }
            let capture = m.capture != Piece::NONE || m.promo != Piece::NONE;
            if score == 0 && capture && known.is_none() {
                // a capture into the tables: a terminal, decided on sight
                match self.g.probe(&c) {
                    Verdict::Draw | Verdict::Loss => score = 3000,
                    Verdict::Win => continue,
                    Verdict::Unknown => {}
                }
            }
            if score == 0 {
                if !in_check && !c.in_check() {
                    if has_piece && quiet > 0 && (!CHASE_QUIET_TOP_ONLY || depth == 0) {
                        quiet_moves.push(m);
                    }
                    continue;
                }
                let Some((s, _)) = self.look_ahead(&c, quiet) else { continue };
                score = s;
                if capture {
                    score += 1;
                }
            }
            if let Some(idx) = known {
                if self.g.chase[idx as usize] == CH_FAIL && self.g.chase_quiet[idx as usize] as usize >= quiet {
                    continue;
                }
            }
            cands.push((m, score, quiet));
        }
        cands.sort_by_key(|c| std::cmp::Reverse(c.1));
        let (res, low, cut) = self.chase_candidates(i, &b, &cands, depth);
        if res == ChaseResult::Hold || cut || quiet_moves.is_empty() {
            return (res, low);
        }
        // every chasing move failed: the quiet moves, scored now
        cands.clear();
        for &m in &quiet_moves {
            let mut c = b;
            c.do_move(m);
            if let Some(idx) = self.g.index(&c) {
                if self.g.chase[idx as usize] == CH_FAIL && self.g.chase_quiet[idx as usize] as usize >= quiet - 1 {
                    continue;
                }
            }
            let Some((score, min_checks)) = self.look_ahead(&c, quiet - 1) else { continue };
            if min_checks == 0 {
                continue;
            }
            cands.push((m, score, quiet - 1));
        }
        cands.sort_by_key(|c| std::cmp::Reverse(c.1));
        cands.truncate(CHASE_QUIET_WIDTH);
        self.quiet_moves += cands.len();
        let (qres, qlow, _) = self.chase_candidates(i, &b, &cands, depth);
        match qres {
            ChaseResult::Hold => (ChaseResult::Hold, qlow),
            ChaseResult::Cut => (ChaseResult::Cut, NO_LOW),
            ChaseResult::Fail => (res, NO_LOW),
        }
    }

    /// Links and chases the candidates (move, score, quiet budget below) of
    /// defender node i in order: the result, a hold's low link, and whether
    /// the run was cut off by the budget or the stop flag (the result is
    /// Cut then).
    fn chase_candidates(&mut self, i: u32, b: &Board, cands: &[(Move, i32, usize)], depth: usize) -> (ChaseResult, usize, bool) {
        let mut res = ChaseResult::Fail;
        for &(m, _, below) in cands {
            if self.aborted {
                return (ChaseResult::Cut, NO_LOW, true);
            }
            let mut c = *b;
            c.do_move(m);
            let Some(ci) = self.link(i, &c) else {
                return (ChaseResult::Cut, NO_LOW, true);
            };
            if self.g.attacker_won(i) {
                return (ChaseResult::Fail, NO_LOW, false);
            }
            if self.g.defender_holds(i) {
                return (ChaseResult::Hold, NO_LOW, false);
            }
            let (cres, clow) = self.chase(ci, depth + 1, below);
            match cres {
                ChaseResult::Hold => return (ChaseResult::Hold, clow, false),
                ChaseResult::Cut => res = ChaseResult::Cut,
                ChaseResult::Fail => {}
            }
        }
        (res, NO_LOW, false)
    }

    /// Scores a defender move (c is the position after it, the attacker to
    /// move): 1000 when every reply holds already, else 8 times the
    /// smallest number of checks the defender has left after a reply, minus
    /// 40 per reply into a new position and 20 per reply into a known open
    /// one (the chain should close, not roam), with that smallest number;
    /// None when a reply refutes the move (it wins for the attacker by the
    /// tables, the graph or the memo of a chase with at least the quiet
    /// budget the path has below the move). A counter-check is skipped for
    /// the count: the defender escapes, the chain goes on.
    fn look_ahead(&self, c: &Board, quiet: usize) -> Option<(i32, i32)> {
        let mut buf = new_buffer();
        let n = c.gen_moves(&mut buf);
        let mut worst = 1000;
        let mut penalty = 0;
        for &r in &buf[..n] {
            let mut d = *c;
            d.do_move(r);
            if let Some(idx) = self.g.index(&d) {
                let iu = idx as usize;
                if self.g.attacker_won(idx) || self.g.chase[iu] == CH_FAIL && self.g.chase_quiet[iu] as usize >= quiet {
                    return None;
                }
                if self.g.defender_holds(idx) || self.g.chase_held(idx) || self.g.flags[iu] & FL_ON_PATH != 0 {
                    continue;
                }
                penalty += 20;
            } else {
                if r.capture != Piece::NONE || r.promo != Piece::NONE {
                    match self.g.probe(&d) {
                        Verdict::Loss => return None, // the defender to move loses
                        Verdict::Win | Verdict::Draw => continue,
                        Verdict::Unknown => {}
                    }
                }
                penalty += 40;
            }
            if d.in_check() {
                continue;
            }
            worst = worst.min(check_count(&d));
        }
        if worst == 1000 {
            return Some((1000, 1000));
        }
        Some((8 * worst - penalty, worst))
    }

    /// Adds the edge from node i to the position c (a new node if needed)
    /// and updates the win counters as expand does; None when the budget is
    /// used up. An existing edge is left alone.
    pub(super) fn link(&mut self, i: u32, c: &Board) -> Option<u32> {
        let g = &mut self.g;
        let (ci, is_new, ok) = g.get_or_put(c.key);
        if !ok {
            self.aborted = true;
            return None;
        }
        if is_new {
            g.add(c);
        }
        let iu = i as usize;
        if g.flags[iu] & FL_EXPANDED == 0 {
            g.flags[iu] |= FL_EXPANDED;
            g.expansions += 1;
        }
        if g.has_child(i, ci) {
            return Some(ci);
        }
        g.add_edge(i, ci);
        if g.status[iu] != ST_UNKNOWN {
            return Some(ci); // decided already (and propagated then): the counters are over
        }
        if g.status[ci as usize] == ST_LOSS {
            g.status[iu] = ST_WIN;
        } else if g.status[ci as usize] == ST_WIN {
            g.remaining[iu] -= 1;
            if g.remaining[iu] == 0 {
                g.status[iu] = ST_LOSS;
            }
        }
        if g.status[iu] != ST_UNKNOWN {
            // propagated exactly once, on the change: a second propagation
            // would take another move from every parent's counter
            g.propagate(i);
        }
        Some(ci)
    }
}
