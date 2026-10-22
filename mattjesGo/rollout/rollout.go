// Package rollout plays a position out with Patricia's network as the
// policy of both sides until the tables, a mate, a stalemate, dead
// material, a repetition or the length cap end it (stage 2 of the
// direction in CLAUDE.md, milestone 6: play variations out until win,
// loss or draw is fairly clear). The result is a guess, never a proof:
// the exact WDL search takes it as an order of the root moves and as a
// line to descend along first.
//
// A ply: the oracle (own tables, Syzygy, dead material) decides the
// position on sight; a position without moves is mate or stalemate; a
// repetition on the path is a draw. Otherwise every move is tried: a
// child the oracle calls lost for the opponent is taken at once (the
// table win is the point of the rollout), a child it calls won for the
// opponent is never taken, a known draw counts as zero, and the rest are
// ordered by the network's view of the child (one accumulator per
// position, the children incremental, the net chosen by the material).
package rollout

import (
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mateab"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/nnue"
)

// Outcome of a rollout for the side to move at its start.
type Outcome uint8

const (
	Unknown Outcome = iota // the length cap
	Win
	Draw
	Loss
)

func (o Outcome) String() string { return [...]string{"unknown", "win", "draw", "loss"}[o] }

// Reason the rollout ended.
type Reason uint8

const (
	Length     Reason = iota // the length cap
	Table                    // the oracle decided the position (tables or dead material)
	Mate                     // the side to move is mated
	Stalemate                // the side to move is stalemated
	Repetition               // a position of the line repeated
)

func (r Reason) String() string {
	return [...]string{"length", "table", "mate", "stalemate", "repetition"}[r]
}

// Options of a rollout.
type Options struct {
	// MaxPlies ends the rollout without a verdict (default 200).
	MaxPlies int
	// Oracle decides positions on sight (nil = dead material only).
	Oracle mateab.Oracle
	// Depth of the move choice: 1 (default) takes the child the net likes
	// best, 2 the child whose worst reply the net likes best (minimax over
	// two plies; a greedy opponent walked into losses and let draws look
	// like wins).
	Depth int
}

// replyMin scores a child c (the opponent to move) for the mover by the
// opponent's best reply: the smallest leaf score over the replies, where a
// leaf is the net's view for the mover (to move again) plus twice what it
// can capture for free; a reply that mates or wins by the tables refutes
// the child, a reply into a table draw scores zero.
func replyMin(c *bitboard.Board, acc *nnue.Accumulator, net *nnue.Net, oracle mateab.Oracle, r *Result) int {
	var buf chess.MoveBuffer
	n := c.GenMoves(&buf)
	worst := 1 << 29
	for _, m := range buf[:n] {
		g := *c
		g.DoMove(m)
		score := 0
		switch v, _, _ := oracle.Probe(&g); v {
		case mateab.Win:
			score = 1 << 29 // the mover to move wins: the opponent will not play it
		case mateab.Loss:
			return -(1 << 29) // the mover loses: the child is refuted
		case mateab.Draw:
			score = 0
		default:
			if !g.HasMoves() {
				if g.InCheck() {
					return -(1 << 29) // the mover is mated
				}
				score = 0 // stalemate
				break
			}
			d := nnue.DeltaOf(c, m)
			var leaf nnue.Accumulator
			leaf.Update(net, acc, &d)
			r.Evals++
			score = leaf.Evaluate(net, g.WhiteMove) + 2*hanging(&g)
		}
		worst = min(worst, score)
	}
	return worst
}

// Result of a rollout.
type Result struct {
	Outcome Outcome
	Reason  Reason
	Line    []chess.Move // the moves played
	Evals   int          // children evaluated by the network
}

// DefaultMaxPlies is the length cap when Options.MaxPlies is 0.
const DefaultMaxPlies = 200

// pieceValue by bitboard kind (king 0, queen, rook, bishop, knight, pawn).
var pieceValue = [6]int{0, 900, 500, 300, 300, 100}

// hanging is the material the side to move in b wins with its best capture
// (the captured piece less the capturer when the capture can be answered
// on the same square): a one-ply exchange estimate, 0 when nothing hangs.
func hanging(b *bitboard.Board) int {
	var buf chess.MoveBuffer
	n := b.GenMoves(&buf)
	best := 0
	for _, m := range buf[:n] {
		if m.Capture == chess.None {
			continue
		}
		gain := pieceValue[kindIdx(m.Capture)]
		c := *b
		c.DoMove(m)
		var reply chess.MoveBuffer
		rn := c.GenMoves(&reply)
		for _, r := range reply[:rn] {
			if r.To == m.To {
				gain -= pieceValue[kindIdx(b.Squares[m.From])]
				break
			}
		}
		best = max(best, gain)
	}
	return best
}

func kindIdx(p chess.Piece) int {
	k := 0
	for t := p & chess.TypeMask; t > 1; t >>= 1 {
		k++
	}
	return k
}

// Play plays root out and reports the outcome for the side to move in root.
func Play(root *bitboard.Board, opt Options) Result {
	maxPlies := opt.MaxPlies
	if maxPlies <= 0 {
		maxPlies = DefaultMaxPlies
	}
	oracle := opt.Oracle
	if oracle == nil {
		oracle = mateab.Material{}
	}
	var r Result
	b := *root
	keys := make([]bitboard.Key, 0, maxPlies+1)
	// the verdict of a terminal for the root's side: v is for the side to
	// move in b, which is the root's side after an even number of plies
	rootView := func(v mateab.Verdict, ply int) Outcome {
		switch v {
		case mateab.Draw:
			return Draw
		case mateab.Win:
			if ply%2 == 0 {
				return Win
			}
			return Loss
		case mateab.Loss:
			if ply%2 == 0 {
				return Loss
			}
			return Win
		}
		return Unknown
	}
	var acc nnue.Accumulator
	var net *nnue.Net
	for ply := 0; ; ply++ {
		if v, _, _ := oracle.Probe(&b); v != mateab.Unknown {
			r.Outcome, r.Reason = rootView(v, ply), Table
			return r
		}
		var buf chess.MoveBuffer
		n := b.GenMoves(&buf)
		if n == 0 {
			if b.InCheck() {
				r.Outcome, r.Reason = rootView(mateab.Loss, ply), Mate
			} else {
				r.Outcome, r.Reason = Draw, Stalemate
			}
			return r
		}
		for _, k := range keys {
			if k == b.Key {
				r.Outcome, r.Reason = Draw, Repetition
				return r
			}
		}
		keys = append(keys, b.Key)
		if ply >= maxPlies {
			r.Outcome, r.Reason = Unknown, Length
			return r
		}
		// the accumulator is carried along the line; a change of the net
		// (the material crossed the phase bound) refreshes it
		if phaseNet := nnue.Load(nnue.PhaseOf(&b)); phaseNet != net {
			net = phaseNet
			acc.Refresh(net, &b)
		}
		best, bestScore := chess.Move{}, int(-1<<30)
		var bestAcc nnue.Accumulator
		for _, m := range buf[:n] {
			c := b
			c.DoMove(m)
			score := 0
			var child nnue.Accumulator
			switch v, _, _ := oracle.Probe(&c); v {
			case mateab.Loss:
				score = 1 << 29 // the opponent to move loses: the table win
			case mateab.Win:
				continue // the opponent wins: never
			case mateab.Draw:
				score = 0
			default:
				if !c.HasMoves() {
					if c.InCheck() {
						score = 1 << 29 // mate
					} // else stalemate: a draw, score 0
					break
				}
				d := nnue.DeltaOf(&b, m)
				child.Update(net, &acc, &d)
				if opt.Depth >= 2 {
					// the opponent's best reply decides (minimax over two
					// plies, the net at the leaves from the mover's view)
					score = replyMin(&c, &child, net, oracle, &r)
				} else {
					r.Evals++
					score = -child.Evaluate(net, c.WhiteMove) // the opponent's view, negated
					// the static view does not see a hanging piece (the queen
					// walked into the king's reach in KQ-K): the opponent's best
					// capture that cannot be answered, twice counted
					score -= 2 * hanging(&c)
				}
				for _, k := range keys {
					if k == c.Key {
						// a position of the line again: only when nothing
						// else is left (both sides shuffled otherwise, KQ-K
						// ended as a repetition after 16 plies)
						score -= 1 << 28
						break
					}
				}
			}
			if score > bestScore {
				best, bestScore, bestAcc = m, score, child
			}
		}
		if bestScore == int(-1<<30) {
			// every move loses by the tables: take the first, the next ply ends it
			best = buf[0]
			c := b
			c.DoMove(best)
			d := nnue.DeltaOf(&b, best)
			bestAcc.Update(net, &acc, &d)
		}
		r.Line = append(r.Line, best)
		b.DoMove(best)
		acc = bestAcc
	}
}
