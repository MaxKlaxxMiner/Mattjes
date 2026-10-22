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

// egtbWdlSize derives the WDL of every four-piece table from the exact base
// and measures how it packs, against the Syzygy WDL file of the same material
// in syzygyDefaultDir. Three variants per table, each packed with lz (4 MB
// blocks): the plain symbols with the dead and illegal indices filled by their
// predecessor (what the DTM file does), the same with every position whose
// best capture already achieves its value filled too (Syzygy's "don't care":
// the probe resolves captures before it reads the table, so the stored value
// only matters when it beats the best capture), and that one packed four
// symbols per byte. The filled symbol streams are dumped to dumpDir (the
// gitignored tests folder) for external packers (zstd, xz) as a bound.
func egtbWdlSize(workers int, dumpDir string) {
	set := egtbLoadOrGenerate(workers)
	if dumpDir != "" {
		if err := os.MkdirAll(dumpDir, 0o755); err != nil {
			panic(err)
		}
	}
	fmt.Printf("%-6s %11s %11s %11s %6s | %10s %10s %10s | %10s %7s\n", "table", "indices", "legal", "captures", "cap%", "plain lz", "dontcare", "2-bit lz", "syzygy", "ratio")
	var sumPlain, sumDont, sumBits, sumSyzygy int64
	start := time.Now()
	for _, t := range set.Base() {
		pieces := append([]chess.Piece{chess.WhiteKing, chess.BlackKing}, materialPieces(t.Mat)...)
		n := t.Size
		plain := make([]byte, n)
		dont := make([]byte, n)
		// valid: a legal, canonical index; decided: its best capture already
		// achieves its value (both computed in parallel, filled serially)
		valid := make([]bool, n)
		decided := make([]bool, n)
		var wg sync.WaitGroup
		chunk := (n + workers - 1) / workers
		for w := 0; w < workers; w++ {
			lo, hi := w*chunk, min(n, (w+1)*chunk)
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
					valid[idx] = true
					own := wdlSymbol(t.Values[idx])
					plain[idx] = own
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
							panic("egtbWdlSize: no table after " + c.FEN())
						}
						hasCapture = true
						best = max(best, 2-wdlSymbol(v)) // the child's value is the opponent's
					}
					decided[idx] = hasCapture && best == own
				}
			}()
		}
		wg.Wait()
		legal, captures := 0, 0
		prevPlain, prevDont := byte(1), byte(1)
		for idx := 0; idx < n; idx++ {
			if !valid[idx] {
				plain[idx], dont[idx] = prevPlain, prevDont
				continue
			}
			legal++
			prevPlain = plain[idx]
			if decided[idx] {
				captures++
				dont[idx] = prevDont
			} else {
				dont[idx] = plain[idx]
				prevDont = plain[idx]
			}
		}
		bits := make([]byte, (n+3)/4)
		for idx := 0; idx < n; idx++ {
			bits[idx>>2] |= dont[idx] << (2 * uint(idx&3))
		}
		plainLen := len(lz.Pack(plain, workers))
		dontLen := len(lz.Pack(dont, workers))
		bitsLen := len(lz.Pack(bits, workers))
		syzygy := syzygyFileSize(t.Mat)
		sumPlain += int64(plainLen)
		sumDont += int64(dontLen)
		sumBits += int64(bitsLen)
		sumSyzygy += syzygy
		ratio := "-"
		if syzygy > 0 {
			ratio = fmt.Sprintf("%.2f", float64(min(dontLen, bitsLen))/float64(syzygy))
		}
		fmt.Printf("%-6s %11d %11d %11d %5.1f%% | %10d %10d %10d | %10d %7s\n", t.Mat.Name(), n, legal, captures, 100*float64(captures)/float64(max(legal, 1)), plainLen, dontLen, bitsLen, syzygy, ratio)
		if dumpDir != "" {
			if err := os.WriteFile(filepath.Join(dumpDir, t.Mat.Name()+".wdl"), dont, 0o644); err != nil {
				panic(err)
			}
		}
	}
	fmt.Printf("%-6s %11s %11s %11s %6s | %10d %10d %10d | %10d %7.2f   (%.1f s)\n", "total", "", "", "", "", sumPlain, sumDont, sumBits, sumSyzygy, float64(min(sumDont, sumBits))/float64(max(sumSyzygy, 1)), time.Since(start).Seconds())
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
