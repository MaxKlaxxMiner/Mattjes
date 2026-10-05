package main

import (
	"fmt"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
)

// egtbIndexRoundtrip checks the position indexing of every table: decoding an
// index and encoding the squares again must give the same index, and every
// symmetric image of a position (mirrors, transposition, color swap) must be
// located at the same index. Four-piece tables are sampled with a stride.
func egtbIndexRoundtrip(stride int) {
	set := egtb.NewSet()
	fmt.Printf("egtb: %d materials, %d MB\n", len(set.Tables), set.TotalSize()>>20)
	start := time.Now()
	for _, t := range set.Tables {
		step := 1
		if t.Mat.Pieces() > 3 {
			step = stride
		}
		sq := make([]chess.Pos, t.Mat.Pieces())
		pieces := append([]chess.Piece{chess.WhiteKing, chess.BlackKing}, materialPieces(t.Mat)...)
		legal, checked, dead, images := 0, 0, 0, 0
		symmetric := len(t.Mat.White) == len(t.Mat.Black) && len(t.Mat.White) == 1 && t.Mat.White[0] == t.Mat.Black[0]
		for idx := 0; idx < t.Size; idx += step {
			whiteMove := t.Decode(idx, sq)
			if back := t.Index(whiteMove, sq); back != idx {
				// dead index (two equal pieces in the other order): the same position, other digits
				sq2 := make([]chess.Pos, len(sq))
				if t.Decode(back, sq2) != whiteMove || !samePosition(sq, sq2) {
					panic(fmt.Sprintf("%s: index %d decodes to %v, encodes to %d = %v", t.Mat.Name(), idx, sq, back, sq2))
				}
				dead++
				continue
			}
			checked++
			if !distinct(sq) {
				continue
			}
			b := bitboard.FromPieces(whiteMove, sq, pieces)
			if b.OpponentInCheck() {
				continue
			}
			legal++
			// every symmetric image must come back to this index
			transforms := 8
			if t.Mat.Pawns() > 0 {
				transforms = 2
			}
			for tf := 0; tf < transforms; tf++ {
				for flip := 0; flip < 2; flip++ {
					img := imageBoard(whiteMove, sq, pieces, tf, flip == 1)
					t2, idx2, ok := set.Locate(&img)
					// symmetric material (KQKQ): the color-swapped image is another position of the same table
					sameIndex := flip == 0 || !symmetric
					if !ok || t2 != t || (sameIndex && idx2 != idx) {
						panic(fmt.Sprintf("%s: index %d (%s), transform %d flip %v located as %v/%d ok=%v", t.Mat.Name(), idx, b.FEN(), tf, flip == 1, t2, idx2, ok))
					}
					images++
				}
			}
		}
		fmt.Printf("%-6s %10d indices, %10d checked, %10d dead, %10d legal, %10d images ok\n", t.Mat.Name(), t.Size, checked, dead, legal, images)
	}
	fmt.Printf("roundtrip ok in %.1f s\n", time.Since(start).Seconds())
}

// egtbGenerate computes the named tables (and their dependencies), compares
// the longest mates with the literature and the test positions of
// chess.MatePositions with the mate search results. verbose prints a line per
// level. Names "all" generates every table.
func egtbGenerate(names []string, workers int, verbose bool, verify bool) {
	set := egtb.NewSet()
	progress := func(line string) {
		if verbose || line[0] != ' ' {
			fmt.Println(line)
		}
	}
	start := time.Now()
	if len(names) == 1 && names[0] == "all" {
		names = nil
		for _, t := range set.Tables {
			names = append(names, t.Mat.Name())
		}
	}
	stats := map[string]egtb.Stats{}
	for _, name := range names {
		t := set.Find(name)
		if t == nil {
			panic("unknown material " + name)
		}
		if t.Values != nil {
			continue // already generated as a dependency
		}
		stats[name] = set.Generate(t, workers, progress)
	}
	fmt.Printf("\ngenerated in %.1f s\n\n", time.Since(start).Seconds())

	if verify {
		fmt.Println("forward verification (every position recomputed from its children):")
		start = time.Now()
		for _, t := range set.Tables {
			if t.Values == nil {
				continue
			}
			bad := set.Verify(t, workers, func(line string) { fmt.Println(line) })
			verdict := "OK"
			if bad > 0 {
				verdict = fmt.Sprintf("%d MISMATCHES", bad)
			}
			fmt.Printf("  %-5s %s\n", t.Mat.Name(), verdict)
		}
		fmt.Printf("verified in %.1f s\n\n", time.Since(start).Seconds())
	}

	fmt.Println("longest mates (moves) against the literature:")
	for _, t := range set.Tables {
		st, ok := stats[t.Mat.Name()]
		if !ok {
			continue
		}
		if want, known := egtb.KnownMaxima[t.Mat.Name()]; known {
			verdict := "OK"
			if st.LongestMate() != want {
				verdict = fmt.Sprintf("MISMATCH, expected %d", want)
			}
			fmt.Printf("  %-5s %3d  %s\n", t.Mat.Name(), st.LongestMate(), verdict)
		} else {
			fmt.Printf("  %-5s %3d  (not in the list)\n", t.Mat.Name(), st.LongestMate())
		}
	}

	fmt.Println("\ntest positions:")
	for _, p := range chess.MatePositions {
		b, err := bitboard.FromFEN(p.FEN)
		if err != nil {
			panic(err)
		}
		v, ok := set.Lookup(&b)
		if !ok {
			continue // more pieces or table not generated
		}
		verdict := "OK"
		if !v.IsWin() || v.Plies() != 2*p.MateIn-1 {
			verdict = fmt.Sprintf("MISMATCH, expected win in %d", 2*p.MateIn-1)
		}
		fmt.Printf("  %-7s %-12s %s\n", p.Name, v, verdict)
	}
}

// egtbLoadOrGenerate is the production path: the cache file next to the
// binary is loaded when present and correct, otherwise all tables are
// generated, checked against the checksum constants and written.
func egtbLoadOrGenerate(workers int) *egtb.Set {
	set := egtb.LoadOrGenerate(egtb.DefaultPath(), workers, func(line string) { fmt.Println(line) })
	fmt.Println()
	return set
}

// egtbProbe looks positions up in the tables and follows the optimal line:
// the winner picks a child lost in n-1, the loser a child won in the longest
// n-1 (captures and promotions cross into smaller tables). Positions with an
// en passant right are not in the tables; the line stops there.
func egtbProbe(fens ...string) {
	set := egtbLoadOrGenerate(12)
	for _, fen := range fens {
		b, err := bitboard.FromFEN(fen)
		if err != nil {
			panic(err)
		}
		v, ok := set.Lookup(&b)
		if !ok {
			fmt.Printf("%s: not covered by the tables\n", fen)
			continue
		}
		fmt.Printf("%s: %s", fen, v)
		if v.IsWin() {
			fmt.Printf(" = mate in %d", (v.Plies()+1)/2)
		}
		fmt.Println()
		var line []string
		for v.IsWin() || (v.IsLoss() && v.Plies() > 0) { // stop at mate (loss in 0)
			var buf chess.MoveBuffer
			n := b.GenMoves(&buf)
			best, bestVal, found := chess.Move{}, egtb.Draw, false
			for _, m := range buf[:n] {
				c := b
				c.DoMove(m)
				cv, ok := set.Lookup(&c)
				if !ok {
					continue
				}
				want := cv.IsLoss() && cv.Plies() == v.Plies()-1 // winner: the child that loses fastest
				if v.IsLoss() {
					want = cv.IsWin() && (!found || cv.Plies() > bestVal.Plies()) // loser: the child that wins slowest
				}
				if want {
					best, bestVal, found = m, cv, true
					if v.IsWin() {
						break
					}
				}
			}
			if !found {
				line = append(line, "(en passant, not in the tables)")
				break
			}
			line = append(line, best.UCI())
			b.DoMove(best)
			v = bestVal
		}
		fmt.Printf("    %s\n\n", pvString2(line))
	}
}

func pvString2(line []string) string {
	s := ""
	for i, m := range line {
		if i%2 == 0 {
			s += fmt.Sprintf("%d.", i/2+1)
		}
		s += m + " "
	}
	return s
}

func materialPieces(m egtb.Material) []chess.Piece {
	var ps []chess.Piece
	for _, p := range m.White {
		ps = append(ps, chess.White|p)
	}
	for _, p := range m.Black {
		ps = append(ps, chess.Black|p)
	}
	return ps
}

// samePosition reports whether both square lists hold the same kings and the
// same set of other squares (equal pieces may be listed in either order), or
// the transposed set when both kings are on the diagonal (the dead twin).
func samePosition(a, b []chess.Pos) bool {
	if a[0] != b[0] || a[1] != b[1] {
		return false
	}
	var sa, sb, st uint64
	for i := 2; i < len(a); i++ {
		sa |= 1 << uint(a[i])
		sb |= 1 << uint(b[i])
		st |= 1 << uint(chess.PosFromXY(7-b[i].Y(), 7-b[i].X())) // transposed
	}
	onDiag := func(p chess.Pos) bool { return p.X() == 7-p.Y() }
	return sa == sb || (sa == st && onDiag(a[0]) && onDiag(a[1]))
}

func distinct(sq []chess.Pos) bool {
	var seen uint64
	for _, s := range sq {
		if seen&(1<<uint(s)) != 0 {
			return false
		}
		seen |= 1 << uint(s)
	}
	return true
}

// imageBoard builds the position transformed by symmetry tf (1 mirror files,
// 2 mirror ranks, 4 transpose) and optionally with swapped colors (which also
// mirrors the ranks, so pawns keep their direction).
func imageBoard(whiteMove bool, sq []chess.Pos, pieces []chess.Piece, tf int, flip bool) bitboard.Board {
	sq2 := make([]chess.Pos, len(sq))
	ps2 := make([]chess.Piece, len(pieces))
	for i, s := range sq {
		f, r := s.X(), 7-s.Y()
		if tf&1 != 0 {
			f = 7 - f
		}
		if tf&2 != 0 {
			r = 7 - r
		}
		if tf&4 != 0 {
			f, r = r, f
		}
		if flip {
			r = 7 - r
		}
		sq2[i] = chess.PosFromXY(f, 7-r)
		ps2[i] = pieces[i]
		if flip {
			ps2[i] ^= chess.ColorMask
		}
	}
	return bitboard.FromPieces(whiteMove != flip, sq2, ps2)
}
