package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/lz"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/pairs"
)

// --- own WDL tables: how small can a WDL table derived from the DTM base get? ---

// wdlSymbol is the three-valued result from the point of view of the side to
// move: 0 loss, 1 draw, 2 win (the order Syzygy uses, minus the fifty-move
// shades).
func wdlSymbol(v egtb.Value) byte {
	switch {
	case v.IsWin():
		return 2
	case v.IsLoss():
		return 0
	}
	return 1
}

// wdlTable holds the WDL of one base table with the two don't-care sets.
type wdlTable struct {
	t       *egtb.Table
	plain   []byte // the symbol per index (meaningless at invalid indices)
	valid   []bool // a legal, canonical index
	decided []bool // its best capture already achieves its value (Syzygy's don't care)
	radix   []int  // the index digits, slowest first: side, king pair, then the pieces
}

// deriveWdl computes the symbols and the don't-care sets of a base table in
// parallel.
func deriveWdl(set *egtb.Set, t *egtb.Table, workers int) *wdlTable {
	pieces := append([]chess.Piece{chess.WhiteKing, chess.BlackKing}, materialPieces(t.Mat)...)
	n := t.Size
	w := &wdlTable{t: t, plain: make([]byte, n), valid: make([]bool, n), decided: make([]bool, n)}
	kk := egtb.KKPawnless
	if t.Mat.Pawns() > 0 {
		kk = egtb.KKPawns
	}
	w.radix = []int{2, kk}
	for _, p := range pieces[2:] {
		if p.Is(chess.Pawn) {
			w.radix = append(w.radix, 48)
		} else {
			w.radix = append(w.radix, 64)
		}
	}
	var wg sync.WaitGroup
	chunk := (n + workers - 1) / workers
	for wk := 0; wk < workers; wk++ {
		lo, hi := wk*chunk, min(n, (wk+1)*chunk)
		wg.Add(1)
		go func() {
			defer wg.Done()
			sq := make([]chess.Pos, len(pieces))
			var buf chess.MoveBuffer
			for idx := lo; idx < hi; idx++ {
				whiteMove := t.Decode(idx, sq)
				if !distinct(sq) || t.Index(whiteMove, sq) != idx {
					continue
				}
				b := bitboard.FromPieces(whiteMove, sq, pieces)
				if b.OpponentInCheck() {
					continue
				}
				w.valid[idx] = true
				own := wdlSymbol(t.Values[idx])
				w.plain[idx] = own
				best := byte(0)
				hasCapture := false
				for _, m := range buf[:b.GenMoves(&buf)] {
					if m.Capture == chess.None {
						continue
					}
					c := b
					c.DoMove(m)
					v, ok := set.Lookup(&c)
					if !ok {
						panic("deriveWdl: no table after " + c.FEN())
					}
					hasCapture = true
					best = max(best, 2-wdlSymbol(v)) // the child's value is the opponent's
				}
				w.decided[idx] = hasCapture && best == own
			}
		}()
	}
	wg.Wait()
	return w
}

// stream lays the symbols out in the given digit order (a permutation of
// the radix positions, slowest first; "" = the table's own order) and fills
// the invalid and, with dontCare, the capture-decided positions with their
// predecessor in that order.
func (w *wdlTable) stream(order string, dontCare bool) []byte {
	d := len(w.radix)
	perm := orderDigits(order, d)
	n := len(w.plain)
	out := make([]byte, n)
	digits := make([]int, d)
	prev := byte(1)
	for pos := 0; pos < n; pos++ {
		// decompose pos in the permuted radix, slowest digit first
		p := pos
		for i := d - 1; i >= 0; i-- {
			r := w.radix[perm[i]]
			digits[perm[i]] = p % r
			p /= r
		}
		idx := 0
		for i := 0; i < d; i++ {
			idx = idx*w.radix[i] + digits[i]
		}
		if !w.valid[idx] || (dontCare && w.decided[idx]) {
			out[pos] = prev
		} else {
			out[pos] = w.plain[idx]
			prev = out[pos]
		}
	}
	return out
}

// orderDigits is the digit order of a named layout, slowest first, for a
// table with d digits (side, king pair, pieces): "n" the table's own order
// (the last piece fastest), "r" the pieces reversed, "k" the king pair
// fastest (the pieces slowest), "K" the king pair fastest and the pieces
// reversed.
func orderDigits(name string, d int) []int {
	perm := make([]int, d)
	for i := range perm {
		perm[i] = i
	}
	pieces := perm[2:]
	switch name {
	case "n", "":
	case "r":
		for i, j := 0, len(pieces)-1; i < j; i, j = i+1, j-1 {
			pieces[i], pieces[j] = pieces[j], pieces[i]
		}
	case "k", "K":
		perm = append([]int{0}, append(append([]int(nil), pieces...), 1)...)
		if name == "K" {
			p := perm[1 : d-1]
			for i, j := 0, len(p)-1; i < j; i, j = i+1, j-1 {
				p[i], p[j] = p[j], p[i]
			}
		}
	default:
		panic("unknown order " + name)
	}
	return perm
}

// egtbWdlSize derives the WDL of every four-piece table from the exact base
// and measures how it packs, against the Syzygy WDL file of the same material
// in syzygyDefaultDir. Per table and digit order (orders: semicolon
// separated names, see orderDigits; "n" = the table's own order) the
// capture-decided positions are filled like the invalid ones (Syzygy's
// "don't care": the probe resolves captures before it reads the table, so
// the stored value only matters when it beats the best capture) and the
// stream is packed with package pairs; the first order also with lz (four
// symbols per byte) for reference and dumped to dumpDir for external packers.
func egtbWdlSize(workers int, orders string, dumpDir string, opt pairs.Options, verbose bool) {
	set := egtbLoadOrGenerate(workers)
	if dumpDir != "" {
		if err := os.MkdirAll(dumpDir, 0o755); err != nil {
			panic(err)
		}
	}
	orderList := strings.Split(orders, ";")
	fmt.Printf("%-6s %11s %6s | %10s %10s", "table", "legal", "cap%", "lz 2-bit", "syzygy")
	for _, o := range orderList {
		fmt.Printf(" | %7s %5s", "pairs "+o, "ratio")
	}
	fmt.Println()
	sums := make([]int64, len(orderList))
	var sumLz, sumSyzygy int64
	start := time.Now()
	for _, t := range set.Base() {
		w := deriveWdl(set, t, workers)
		legal, captures := 0, 0
		for idx := range w.valid {
			if w.valid[idx] {
				legal++
				if w.decided[idx] {
					captures++
				}
			}
		}
		syzygy := syzygyFileSize(t.Mat)
		sumSyzygy += syzygy
		var lzLen int
		var best pairs.Stats
		bestOrder := ""
		for i, o := range orderList {
			s := w.stream(o, true)
			if i == 0 {
				bits := make([]byte, (len(s)+3)/4)
				for idx, v := range s {
					bits[idx>>2] |= v << (2 * uint(idx&3))
				}
				lzLen = len(lz.Pack(bits, workers))
				sumLz += int64(lzLen)
				if dumpDir != "" {
					if err := os.WriteFile(filepath.Join(dumpDir, t.Mat.Name()+".wdl"), s, 0o644); err != nil {
						panic(err)
					}
				}
				fmt.Printf("%-6s %11d %5.1f%% | %10d %10d", t.Mat.Name(), legal, 100*float64(captures)/float64(max(legal, 1)), lzLen, syzygy)
			}
			_, st := pairs.PackStats(s, 3, opt)
			sums[i] += int64(st.Bytes)
			if bestOrder == "" || st.Bytes < best.Bytes {
				best, bestOrder = st, o
			}
			ratio := "-"
			if syzygy > 0 {
				ratio = fmt.Sprintf("%.2f", float64(st.Bytes)/float64(syzygy))
			}
			fmt.Printf(" | %7d %5s", st.Bytes, ratio)
		}
		fmt.Println()
		if verbose {
			fmt.Printf("       best %s: %s\n", bestOrder, best)
		}
	}
	fmt.Printf("%-6s %11s %6s | %10d %10d", "total", "", "", sumLz, sumSyzygy)
	for _, s := range sums {
		fmt.Printf(" | %7d %5.2f", s, float64(s)/float64(max(sumSyzygy, 1)))
	}
	fmt.Printf("   (%.1f s)\n", time.Since(start).Seconds())
}

// syzygyDtzSize is the size of the Syzygy DTZ file of a material, 0 when missing.
func syzygyDtzSize(m egtb.Material) int64 {
	name := m.Name()
	name = name[:1] + strings.Replace(name[1:], "K", "vK", 1)
	info, err := os.Stat(filepath.Join(syzygyDefaultDir, name+".rtbz"))
	if err != nil {
		return 0
	}
	return info.Size()
}

// syzygyFileSize is the size of the Syzygy WDL file of a material in the
// author's folder ("KQKR" -> "KQvKR.rtbw"), 0 when missing.
func syzygyFileSize(m egtb.Material) int64 {
	name := m.Name()
	name = name[:1] + strings.Replace(name[1:], "K", "vK", 1)
	info, err := os.Stat(filepath.Join(syzygyDefaultDir, name+".rtbw"))
	if err != nil {
		return 0
	}
	return info.Size()
}
