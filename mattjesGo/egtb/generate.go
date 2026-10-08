package egtb

import (
	"fmt"
	"math/bits"
	"sync"
	"sync/atomic"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// Progress receives one line per phase; nil = silent.
type Progress func(string)

// Stats describes a generated table.
type Stats struct {
	Legal, Mates, Wins, Losses, Draws int
	MaxWin, MaxLoss                   int // longest distances in plies
	Levels                            int
	Evaluations                       int // candidate evaluations (forward move generation) over all levels
	Duration                          time.Duration
	Overflow                          bool // distances beyond MaxPlies occurred, the table is incomplete
	Beyond                            int  // positions with distance MaxPlies+1 (or waiting for deeper cross-table children): a lower bound of what is missing
}

// LongestMate is the longest forced mate in moves (the usual way to quote it).
func (s Stats) LongestMate() int { return (s.MaxWin + 1) / 2 }

// KnownMaxima are the longest forced mates in moves from the literature, used
// to verify the generator. Materials not listed are only printed.
var KnownMaxima = map[string]int{
	"KQK": 10, "KRK": 16, "KBBK": 19, "KBNK": 33, "KQKR": 35,
	"KQKQ": 13, "KRKR": 19, "KQKN": 21, "KQKB": 17, "KRKB": 29, "KRKN": 40,
	"KPK": 28, "KPKP": 33,
	// five pieces (Nalimov DTM maxima)
	"KBBKN": 78, "KBNKN": 107, "KNNKP": 115, "KQPKQ": 124, "KPPKP": 127,
}

// dependencies lists the materials a table looks up: captures (one piece of
// either side removed), promotions (a pawn becomes a piece) and both at once.
func (m Material) dependencies() []Material {
	var deps []Material
	add := func(w, b []chess.Piece) {
		if len(w)+len(b) == 0 {
			return // king against king needs no table
		}
		deps = append(deps, canonicalMaterial(w, b))
	}
	without := func(ps []chess.Piece, i int) []chess.Piece {
		r := append([]chess.Piece{}, ps[:i]...)
		return append(r, ps[i+1:]...)
	}
	promote := func(ps []chess.Piece, i int, to chess.Piece) []chess.Piece {
		r := append([]chess.Piece{}, ps...)
		r[i] = to
		return r
	}
	for i := range m.White {
		add(without(m.White, i), m.Black)
	}
	for i := range m.Black {
		add(m.White, without(m.Black, i))
	}
	for _, to := range []chess.Piece{chess.Queen, chess.Rook, chess.Bishop, chess.Knight} {
		for i, p := range m.White {
			if p == chess.Pawn {
				add(promote(m.White, i, to), m.Black)
				for j := range m.Black {
					add(promote(m.White, i, to), without(m.Black, j))
				}
			}
		}
		for i, p := range m.Black {
			if p == chess.Pawn {
				add(m.White, promote(m.Black, i, to))
				for j := range m.White {
					add(without(m.White, j), promote(m.Black, i, to))
				}
			}
		}
	}
	return deps
}

// canonicalMaterial sorts both sides and puts the stronger one first.
func canonicalMaterial(w, b []chess.Piece) Material {
	w, b = append([]chess.Piece{}, w...), append([]chess.Piece{}, b...)
	sortTypes(w)
	sortTypes(b)
	if compareSides(w, b) < 0 {
		w, b = b, w
	}
	return Material{White: w, Black: b}
}

// GenerateAll computes every table that is not filled yet, in order.
func (s *Set) GenerateAll(workers int, progress Progress) {
	for _, t := range s.Tables {
		if t.Values == nil {
			s.Generate(t, workers, progress)
		}
	}
}

// Generate computes one table (and first the tables it depends on, if they
// are missing). See docs/m5-endgame-tables-design.md, section 3: level by
// level, a position is decided by looking at its children with the forward
// move generator; un-moves only tell which positions are worth looking at
// (the parents of positions decided in the previous level).
func (s *Set) Generate(t *Table, workers int, progress Progress) Stats {
	for _, d := range t.Mat.dependencies() {
		dt := s.AddMaterial(d) // beyond the base this registers the dependency on the fly
		if dt.Values == nil {
			if path := s.TablePath(dt); s.LoadTable(dt, path) == nil {
				if progress != nil {
					progress("egtb: loaded dependency " + path)
				}
			} else {
				s.Generate(dt, workers, progress)
			}
		}
	}
	if progress == nil {
		progress = func(string) {}
	}
	start := time.Now()
	t.Values = make([]Value, t.Size)
	g := newGenerator(s, t)
	for i := range g.cand {
		g.cand[i] = newBitset(t.Size)
	}
	g.invalid = newBitset(t.Size)
	t.invalid = g.invalid // kept until the table is written (fill), 1/8 of the table

	var st Stats
	legal, mates := g.parallelRange(workers, g.initRange)
	st.Legal, st.Mates = legal, mates
	progress(fmt.Sprintf("%-5s %10d indices, %10d legal, %8d mates", t.Mat.Name(), t.Size, legal, mates))

	completed := false
	for level := 1; level <= MaxPlies; level++ {
		cur := g.cand[level%3]
		for _, idx := range g.pending[level] {
			cur.set(int(idx))
		}
		g.pending[level] = nil
		g.level = level
		changed, evals := g.parallelBits(workers, cur, g.scanCandidates)
		st.Evaluations += evals
		cur.clear()
		if changed > 0 {
			st.Levels = level
			if level%2 == 1 {
				st.Wins += changed
				st.MaxWin = level
			} else {
				st.Losses += changed
				st.MaxLoss = level
			}
			progress(fmt.Sprintf("      level %3d: %9d positions, %10d evaluated", level, changed, evals))
		}
		if !g.cand[(level+1)%3].any() && !g.cand[(level+2)%3].any() && !g.pendingBeyond(level) {
			completed = true
			break
		}
	}
	if !completed || g.beyond.Load() > 0 {
		// distances beyond MaxPlies cannot be stored in a byte; the positions
		// that were still open stay 0 and would read as draws. One more scan
		// at level MaxPlies+1 without storing counts them (their own parents
		// are not followed, so the count is a lower bound).
		level := MaxPlies + 1
		cur := g.cand[level%3]
		for _, idx := range g.pending[level] {
			cur.set(int(idx))
		}
		g.pending[level] = nil
		g.level = level
		beyond, evals := g.parallelBits(workers, cur, g.scanCandidates)
		st.Evaluations += evals
		st.Beyond = beyond + int(g.beyond.Load())
		st.Overflow = true
		progress(fmt.Sprintf("%-5s WARNING: distances exceed %d plies, at least %d positions left as draws", t.Mat.Name(), MaxPlies, st.Beyond))
	}
	st.Losses += mates
	st.Draws = legal - st.Wins - st.Losses
	t.RawChecksum = Checksum(t.Values)
	st.Duration = time.Since(start)
	progress(fmt.Sprintf("%-5s wins %9d, losses %9d, draws %9d, longest mate %d plies = %d moves, %.1f s",
		t.Mat.Name(), st.Wins, st.Losses, st.Draws, st.MaxWin, st.LongestMate(), st.Duration.Seconds()))
	return st
}

type generator struct {
	set               *Set
	t                 *Table
	pieces            []chess.Piece // white king, black king, slots
	enPassantPossible bool          // pawns of both colors: a double step can create an en passant right
	level             int
	// cand[level%3] holds the candidates of a level: positions with a child
	// decided in the previous level (and, with en passant, two levels back).
	cand [3]bitset
	// invalid marks dead indices (doubly occupied, equal pieces in the other
	// order, diagonal twin) and illegal positions (opponent in check); their
	// value stays 0 and is never looked up.
	invalid bitset
	// pending[level] lists positions whose decision waits for a level that no
	// in-table child will trigger: their decisive children lie in smaller
	// tables (captures, promotions) with a larger distance.
	pending   [MaxPlies + 2][]int64 // 64 bit: six-piece tables have up to 45 G indices
	pendingMu sync.Mutex
	beyond    atomic.Int64 // positions waiting for a level beyond MaxPlies+1 (cannot be stored)
}

func newGenerator(s *Set, t *Table) *generator {
	g := &generator{set: s, t: t, pieces: append([]chess.Piece{chess.WhiteKing, chess.BlackKing}, t.slots...)}
	whitePawns, blackPawns := false, false
	for _, p := range t.slots {
		if p == chess.WhitePawn {
			whitePawns = true
		}
		if p == chess.BlackPawn {
			blackPawns = true
		}
	}
	g.enPassantPossible = whitePawns && blackPawns
	return g
}

func (g *generator) registerPending(level, idx int) {
	if level > MaxPlies+1 {
		g.beyond.Add(1) // beyond the value range: counted for the overflow report
		return
	}
	g.pendingMu.Lock()
	g.pending[level] = append(g.pending[level], int64(idx))
	g.pendingMu.Unlock()
}

func (g *generator) pendingBeyond(level int) bool {
	for l := level + 1; l < len(g.pending); l++ {
		if len(g.pending[l]) > 0 {
			return true
		}
	}
	return false
}

// bitset is a candidate set with atomic insertion, so all workers can mark.
type bitset []uint64

func newBitset(n int) bitset { return make(bitset, (n+63)/64) }

func (b bitset) set(i int) { atomic.OrUint64(&b[i>>6], 1<<(uint(i)&63)) }

func (b bitset) get(i int) bool { return b[i>>6]&(1<<(uint(i)&63)) != 0 }

func (b bitset) clear() {
	for i := range b {
		b[i] = 0
	}
}

func (b bitset) any() bool {
	for _, w := range b {
		if w != 0 {
			return true
		}
	}
	return false
}

// parallelRange runs fn over the index range in chunks and sums its results.
func (g *generator) parallelRange(workers int, fn func(lo, hi int) (int, int)) (int, int) {
	return parallelChunks(workers, g.t.Size, 1<<16, fn)
}

// parallelBits runs fn over the set bits of a candidate set in chunks of words.
func (g *generator) parallelBits(workers int, b bitset, fn func(idx int) (int, int)) (int, int) {
	return parallelChunks(workers, len(b), 1<<10, func(lo, hi int) (int, int) {
		x, y := 0, 0
		for w := lo; w < hi; w++ {
			for bits := b[w]; bits != 0; bits &= bits - 1 {
				dx, dy := fn(w<<6 | trailingZeros(bits))
				x += dx
				y += dy
			}
		}
		return x, y
	})
}

func parallelChunks(workers, size, chunk int, fn func(lo, hi int) (int, int)) (int, int) {
	if workers < 1 {
		workers = 1
	}
	var next, a, b atomic.Int64
	var wg sync.WaitGroup
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for {
				lo := int(next.Add(int64(chunk))) - chunk
				if lo >= size {
					return
				}
				x, y := fn(lo, min(lo+chunk, size))
				a.Add(int64(x))
				b.Add(int64(y))
			}
		}()
	}
	wg.Wait()
	return int(a.Load()), int(b.Load())
}

// initRange marks invalid indices and mates, makes the parents of the mates
// the candidates of level 1 and registers positions whose decision can only
// come from a smaller table. Returns (legal, mates).
func (g *generator) initRange(lo, hi int) (int, int) {
	t := g.t
	var sq [6]chess.Pos
	var buf chess.MoveBuffer
	legal, mates := 0, 0
	for idx := lo; idx < hi; idx++ {
		s := sq[:len(g.pieces)]
		whiteMove := t.Decode(idx, s)
		if !distinctSquares(s) || t.Index(whiteMove, s) != idx {
			g.invalid.set(idx) // doubly occupied or dead (equal pieces in the other order)
			continue
		}
		b := bitboard.FromPieces(whiteMove, s, g.pieces)
		if b.OpponentInCheck() {
			g.invalid.set(idx)
			continue
		}
		legal++
		n := b.GenMoves(&buf)
		if n == 0 {
			if b.InCheck() {
				t.Values[idx] = LossIn(0)
				mates++
				g.markParents(s, whiteMove, g.cand[1])
			}
			continue // stalemate stays a draw
		}
		// children in smaller tables are final already: a lost one decides the
		// position at its level; if there is no other kind of child, so do
		// won ones (a quiet move or an open child would have to decide first)
		minLoss, maxWin, allCross, allWin := 1<<30, -1, true, true
		for _, m := range buf[:n] {
			if m.Capture == chess.None && m.Promo == chess.None {
				allCross = false
				continue
			}
			switch v := g.child(&b, s, m); {
			case v.IsLoss():
				minLoss = min(minLoss, v.Plies())
			case v.IsWin():
				maxWin = max(maxWin, v.Plies())
			default:
				allWin = false
			}
		}
		if minLoss < 1<<30 {
			g.registerPending(minLoss+1, idx)
		} else if allCross && allWin {
			g.registerPending(maxWin+1, idx)
		}
	}
	return legal, mates
}

// scanCandidates evaluates one candidate of the current level: odd levels
// look for a win in `level` plies (the shortest lost child has level-1), even
// levels for a loss (every child is won by the opponent, the longest in
// level-1). Children inside the table never exceed level-1 at this point,
// children in smaller tables can: such a position is registered for the level
// it waits for. Returns (1 if decided, 1 evaluation). At level MaxPlies+1 the
// scan is dry: it only counts what the byte cannot hold.
func (g *generator) scanCandidates(idx int) (int, int) {
	t := g.t
	if t.Values[idx] != 0 || g.invalid.get(idx) {
		return 0, 0 // decided already, or an illegal parent reached by an un-move
	}
	level := g.level
	dry := level > MaxPlies
	var sq [6]chess.Pos
	var buf chess.MoveBuffer
	s := sq[:len(g.pieces)]
	whiteMove := t.Decode(idx, s)
	if g.enPassantPossible && !dry {
		// a double step into this position may have created an en passant right;
		// that parent is decided by this position's children, not by this value
		g.markEnPassantParents(s, whiteMove, g.cand[(level+1)%3])
	}
	b := bitboard.FromPieces(whiteMove, s, g.pieces)
	n := b.GenMoves(&buf)
	if n == 0 {
		return 0, 1
	}
	win := level%2 == 1
	minLoss, maxWin, allWin := 1<<30, -1, true
moves:
	for _, m := range buf[:n] {
		v := g.child(&b, s, m)
		switch {
		case v.IsLoss():
			minLoss = min(minLoss, v.Plies())
			allWin = false // a lost child makes the position a win, never a loss
			if !win || minLoss <= level-1 {
				break moves // even level: decided; odd level: cannot get shorter
			}
		case v.IsWin():
			maxWin = max(maxWin, v.Plies())
		default:
			allWin = false
			if !win {
				break moves // an open or drawn child saves the position from losing
			}
		}
	}
	decided := false
	if win {
		switch {
		case minLoss == level-1:
			if !dry {
				t.Values[idx] = WinIn(level)
			}
			decided = true
		case minLoss < level-1:
			panic(fmt.Sprintf("egtb %s: %s has a child lost in %d at level %d", t.Mat.Name(), b.FEN(), minLoss, level))
		case minLoss < 1<<30:
			g.registerPending(minLoss+1, idx)
		}
	} else if allWin {
		switch {
		case maxWin == level-1:
			if !dry {
				t.Values[idx] = LossIn(level)
			}
			decided = true
		case maxWin < level-1:
			panic(fmt.Sprintf("egtb %s: %s has longest child %d at level %d", t.Mat.Name(), b.FEN(), maxWin, level))
		default:
			g.registerPending(maxWin+1, idx)
		}
	}
	if !decided {
		return 0, 1
	}
	if !dry {
		g.markParents(s, whiteMove, g.cand[(level+1)%3])
	}
	return 1, 1
}

// markParents sets the candidate bit of every position that reaches the
// position (sq, whiteMove) with a quiet move: a piece of the side that just
// moved is put back on an empty square it could have come from. Illegal
// parents are harmless (the invalid bitset filters them), what matters is
// that no legal parent is missed. Un-captures and
// un-promotions do not exist inside one table.
func (g *generator) markParents(sq []chess.Pos, whiteMove bool, into bitset) {
	mover := chess.Black
	if !whiteMove {
		mover = chess.White
	}
	var occ uint64
	for _, s := range sq {
		occ |= 1 << uint(s)
	}
	var p [6]chess.Pos
	n := copy(p[:], sq)
	for i := 0; i < n; i++ {
		piece := g.pieces[i]
		if piece.Color() != mover {
			continue
		}
		from := sq[i]
		var targets uint64
		if piece.Is(chess.Pawn) {
			targets = pawnOrigins(piece, from, occ)
		} else {
			targets = bitboard.PieceAttacks(piece, from, occ) &^ occ
		}
		for targets != 0 {
			t := chess.Pos(trailingZeros(targets))
			targets &= targets - 1
			p[i] = t
			if idx := g.t.Index(!whiteMove, p[:n]); idx >= 0 {
				into.set(idx)
			}
		}
		p[i] = from
	}
}

// pawnOrigins returns the squares a pawn can have stepped from: one back, and
// two back from its double-step rank with the square between empty.
func pawnOrigins(piece chess.Piece, sq chess.Pos, occ uint64) uint64 {
	back := chess.Pos(chess.Width) // white pawns move north (smaller index), so they came from the south
	doubleRank := 4                // y of rank 4
	if piece.Color() == chess.Black {
		back, doubleRank = -back, 3
	}
	one := sq + back
	if one.Y() < 1 || one.Y() > 6 || occ&(1<<uint(one)) != 0 {
		return 0
	}
	targets := uint64(1) << uint(one)
	if sq.Y() == doubleRank {
		if two := one + back; occ&(1<<uint(two)) == 0 {
			targets |= 1 << uint(two)
		}
	}
	return targets
}

// markEnPassantParents handles the one edge that does not lead to a table
// position: if the side that just moved has a pawn on its double-step rank
// next to an enemy pawn, the parent that double-stepped reached a position
// WITH an en passant right, whose value comes from this position's children.
// That parent becomes a candidate whenever this position is one.
func (g *generator) markEnPassantParents(sq []chess.Pos, whiteMove bool, into bitset) {
	mover := chess.BlackPawn
	if !whiteMove {
		mover = chess.WhitePawn
	}
	var occ uint64
	for _, s := range sq {
		occ |= 1 << uint(s)
	}
	var p [6]chess.Pos
	n := copy(p[:], sq)
	for i := 2; i < n; i++ {
		if g.pieces[i] != mover {
			continue
		}
		from := sq[i]
		origins := pawnOrigins(mover, from, occ)
		two := from + 2*chess.Pos(chess.Width)
		if mover == chess.WhitePawn {
			two = from - 2*chess.Pos(chess.Width)
		}
		if origins&(1<<uint(two)) == 0 {
			continue // no double step possible
		}
		for j := 2; j < n; j++ { // enemy pawn beside it?
			if g.pieces[j] != mover^chess.ColorMask || sq[j].Y() != from.Y() || (sq[j]-from != 1 && from-sq[j] != 1) {
				continue
			}
			p[i] = two
			if idx := g.t.Index(!whiteMove, p[:n]); idx >= 0 {
				into.set(idx)
			}
			p[i] = from
		}
	}
}

// child returns the value of the position after move m (from the point of
// view of the opponent, who is then to move). Quiet moves stay in the table
// and only need the moved square replaced; captures and promotions lead into
// smaller tables; a double step that creates an en passant right leads to a
// position outside every table, whose value comes from its own children.
func (g *generator) child(b *bitboard.Board, sq []chess.Pos, m chess.Move) Value {
	if m.Capture != chess.None || m.Promo != chess.None {
		c := *b
		c.DoMove(m)
		v, ok := g.set.Lookup(&c)
		if !ok {
			panic(fmt.Sprintf("egtb %s: no table for %s", g.t.Mat.Name(), c.FEN()))
		}
		return v
	}
	if g.enPassantPossible && b.Squares[m.From].Is(chess.Pawn) && (m.To-m.From == 2*chess.Width || m.From-m.To == 2*chess.Width) {
		c := *b
		c.DoMove(m)
		if c.EnPassant.Valid() {
			return g.evalEnPassant(&c)
		}
	}
	var s [6]chess.Pos
	n := copy(s[:], sq)
	for i := 0; i < n; i++ {
		if s[i] == m.From {
			s[i] = m.To
			break
		}
	}
	return g.t.Values[g.t.Index(!b.WhiteMove, s[:n])]
}

// evalEnPassant values a position with an en passant right from its children:
// the side to move wins if any child is lost (shortest), loses if all children
// are won (longest), otherwise the value is open (0). Thanks to the level
// invariant the result is final as soon as it is decided.
func (g *generator) evalEnPassant(c *bitboard.Board) Value {
	var buf chess.MoveBuffer
	n := c.GenMoves(&buf)
	if n == 0 {
		if c.InCheck() {
			return LossIn(0)
		}
		return Draw
	}
	minLoss, maxWin, allWin := 1<<30, -1, true
	for _, m := range buf[:n] {
		d := *c
		d.DoMove(m)
		v, ok := g.set.Lookup(&d)
		if !ok {
			panic(fmt.Sprintf("egtb %s: no value for %s after en passant position", g.t.Mat.Name(), d.FEN()))
		}
		switch {
		case v.IsLoss():
			minLoss = min(minLoss, v.Plies())
		case v.IsWin():
			maxWin = max(maxWin, v.Plies())
		default:
			allWin = false
		}
	}
	if minLoss < 1<<30 {
		return WinIn(minLoss + 1)
	}
	if allWin {
		return LossIn(maxWin + 1)
	}
	return Draw
}

// Verify recomputes every position of a finished table from its children with
// the forward move generator alone (no un-moves, no levels) and counts the
// positions whose stored value disagrees. The first few are reported.
func (s *Set) Verify(t *Table, workers int, report func(string)) int {
	g := newGenerator(s, t)
	var reported atomic.Int32
	mismatches, _ := g.parallelRange(workers, func(lo, hi int) (int, int) {
		var sq [6]chess.Pos
		var buf chess.MoveBuffer
		bad := 0
		for idx := lo; idx < hi; idx++ {
			sl := sq[:len(g.pieces)]
			whiteMove := t.Decode(idx, sl)
			// dead and illegal entries are "don't care" (a loaded table has them filled for compression)
			if !distinctSquares(sl) || t.Index(whiteMove, sl) != idx {
				continue
			}
			b := bitboard.FromPieces(whiteMove, sl, g.pieces)
			if b.OpponentInCheck() {
				continue
			}
			want := Draw
			if n := b.GenMoves(&buf); n == 0 {
				if b.InCheck() {
					want = LossIn(0)
				}
			} else {
				minLoss, maxWin, allWin := 1<<30, -1, true
				for _, m := range buf[:n] {
					switch v := g.child(&b, sl, m); {
					case v.IsLoss():
						minLoss = min(minLoss, v.Plies())
					case v.IsWin():
						maxWin = max(maxWin, v.Plies())
					default:
						allWin = false
					}
				}
				if minLoss < 1<<30 {
					want = WinIn(minLoss + 1)
				} else if allWin {
					want = LossIn(maxWin + 1)
				}
			}
			if t.Values[idx] != want {
				bad++
				if reported.Add(1) <= 5 && report != nil {
					b := t.Board(idx)
					report(fmt.Sprintf("  %s: index %d %s stored %s, children say %s", t.Mat.Name(), idx, b.FEN(), t.Values[idx], want))
				}
			}
		}
		return bad, 0
	})
	return mismatches
}

func distinctSquares(sq []chess.Pos) bool {
	var seen uint64
	for _, s := range sq {
		if seen&(1<<uint(s)) != 0 {
			return false
		}
		seen |= 1 << uint(s)
	}
	return true
}

func trailingZeros(x uint64) int { return bits.TrailingZeros64(x) }
