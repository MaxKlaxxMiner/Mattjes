package main

import (
	"bufio"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/nnue"
)

// --- Patricia's network, ported (package nnue) ---

// nnueVerify prints, for every FEN of a file ("<name> | <fen>" or a bare
// FEN, '#' comments skipped), the line the Patricia reference build prints
// for "eval" (tests/patricia-eval.sh <file>), so that the two outputs can
// be diffed: the raw output of all three nets for the side to move, the
// material, the phase and Patricia's eval() at the root.
func nnueVerify(fenfile string) {
	f, err := os.Open(fenfile)
	if err != nil {
		fmt.Println(err)
		return
	}
	defer f.Close()
	start := time.Now()
	n := 0
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		fen := line
		if i := strings.IndexByte(line, '|'); i >= 0 {
			fen = strings.TrimSpace(line[i+1:])
		}
		b, err := bitboard.FromFEN(fen)
		if err != nil {
			fmt.Printf("%s | error: %v\n", fen, err)
			continue
		}
		fmt.Printf("%s | eval mat %d phase %d mg %d eg %d sac %d eval %d\n", fen, nnue.Material(&b), nnue.PhaseOf(&b),
			nnue.Raw(&b, nnue.Middlegame), nnue.Raw(&b, nnue.Endgame), nnue.Raw(&b, nnue.Sacrifice), nnue.Evaluate(&b))
		n++
	}
	fmt.Fprintf(os.Stderr, "--- nnue: %d positions evaluated in %s\n", n, fmtMs(time.Since(start)))
}

// nnueIncremental walks the move trees of the perft positions to the given
// depth and compares the incrementally updated accumulator with a fresh
// one after every move (every delta kind: captures, en passant, promotions,
// castling), for the sacrifice net.
func nnueIncremental(depth int) {
	net := nnue.Load(nnue.Sacrifice)
	start := time.Now()
	nodes, mismatches := 0, 0
	var walk func(b *bitboard.Board, acc *nnue.Accumulator, d int)
	walk = func(b *bitboard.Board, acc *nnue.Accumulator, d int) {
		if d == 0 {
			return
		}
		var buf chess.MoveBuffer
		n := b.GenMoves(&buf)
		for _, m := range buf[:n] {
			delta := nnue.DeltaOf(b, m)
			c := *b
			c.DoMove(m)
			var next, fresh nnue.Accumulator
			next.Update(net, acc, &delta)
			fresh.Refresh(net, &c)
			nodes++
			if next != fresh {
				mismatches++
				if mismatches <= 5 {
					fmt.Printf("  MISMATCH after %s in %s\n", m.UCI(), b.FEN())
				}
			}
			walk(&c, &next, d-1)
		}
	}
	for _, p := range chess.PerftPositions {
		b, err := bitboard.FromFEN(p.FEN)
		if err != nil {
			panic(err)
		}
		var acc nnue.Accumulator
		acc.Refresh(net, &b)
		walk(&b, &acc, depth)
	}
	status := "ok"
	if mismatches > 0 {
		status = fmt.Sprintf("%d MISMATCHES", mismatches)
	}
	fmt.Printf("--- nnue incremental: %d moves checked to depth %d, %s, %s\n", nodes, depth, status, fmtMs(time.Since(start)))
}

// nnueBench measures the evaluation throughput over the move trees of the
// perft positions: a fresh accumulator per node (refresh and evaluate) and
// the incremental path (update and evaluate), sacrifice net; the checksum
// of the scores keeps the work honest.
func nnueBench(depth int) {
	net := nnue.Load(nnue.Sacrifice)
	fmt.Println("nnue bench: scalar")
	var nodes int
	var sum int64
	var walk func(b *bitboard.Board, acc *nnue.Accumulator, d int, incremental bool)
	walk = func(b *bitboard.Board, acc *nnue.Accumulator, d int, incremental bool) {
		if d == 0 {
			return
		}
		var buf chess.MoveBuffer
		n := b.GenMoves(&buf)
		for _, m := range buf[:n] {
			c := *b
			var next nnue.Accumulator
			if incremental {
				delta := nnue.DeltaOf(b, m)
				c.DoMove(m)
				next.Update(net, acc, &delta)
			} else {
				c.DoMove(m)
				next.Refresh(net, &c)
			}
			sum += int64(next.Evaluate(net, c.WhiteMove))
			nodes++
			walk(&c, &next, d-1, incremental)
		}
	}
	for _, run := range []struct {
		incremental bool
		name        string
	}{{false, "refresh + evaluate"}, {true, "update + evaluate "}} {
		start := time.Now()
		nodes, sum = 0, 0
		for _, p := range chess.PerftPositions {
			b, err := bitboard.FromFEN(p.FEN)
			if err != nil {
				panic(err)
			}
			var acc nnue.Accumulator
			acc.Refresh(net, &b)
			walk(&b, &acc, depth, run.incremental)
		}
		el := time.Since(start)
		fmt.Printf("    %s: %d evaluations in %s, %.0f ns each, %.2f M/s, checksum %d\n", run.name, nodes, fmtMs(el), float64(el.Nanoseconds())/float64(nodes), float64(nodes)/el.Seconds()/1e6, sum)
	}
}
