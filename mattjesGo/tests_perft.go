package main

import (
	"fmt"
	"runtime"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/perft"
)

func workersOrCPUs(workers int) int {
	if workers <= 0 {
		return runtime.NumCPU()
	}
	return workers
}

func bitboardInfo() {
	fmt.Printf("bitboard tables: %d magic entries (%d KB), init %s, zobrist %d bit, board %d bytes\n",
		bitboard.AttackTableSize, bitboard.AttackTableSize*8>>10, bitboard.InitDuration, bitboard.KeyWords*64, bitboard.BoardSize)
}

func bitboardPerftRecursive(maxNodes uint64) {
	bitboardInfo()
	perft.Run("bitboard / recursive (make-unmake)", func(fen string, depth int) (uint64, error) {
		b, err := bitboard.FromFEN(fen)
		if err != nil {
			return 0, err
		}
		return bitboard.PerftRecursive(&b, depth), nil
	}, maxNodes)
}

func bitboardPerftIterative(maxNodes uint64) {
	bitboardInfo()
	fmt.Printf("frame size per ply: %d bytes\n", bitboard.FrameSize)
	perft.Run("bitboard / iterative (explicit stack, copy-make)", func(fen string, depth int) (uint64, error) {
		b, err := bitboard.FromFEN(fen)
		if err != nil {
			return 0, err
		}
		return bitboard.PerftIterative(&b, depth), nil
	}, maxNodes)
}

func bitboardPerftBreadth(maxNodes uint64, maxMB int) {
	bitboardInfo()
	fmt.Printf("memory limit: %d MB\n", maxMB)
	perft.Run("bitboard / breadth-first (full position lists per ply)", func(fen string, depth int) (uint64, error) {
		b, err := bitboard.FromFEN(fen)
		if err != nil {
			return 0, err
		}
		return bitboard.PerftBreadth(&b, depth, maxMB<<20)
	}, maxNodes)
}

func bitboardPerftBreadthEncoded(maxNodes uint64, maxMB int, codec bitboard.Codec) {
	bitboardInfo()
	fmt.Printf("codec %s: max %d bytes/position, memory limit: %d MB\n", codec.Name, codec.MaxBytes, maxMB)
	perft.Run("bitboard / breadth-first, "+codec.Name+" encoded position streams", func(fen string, depth int) (uint64, error) {
		b, err := bitboard.FromFEN(fen)
		if err != nil {
			return 0, err
		}
		return bitboard.PerftBreadthEncoded(&b, depth, maxMB<<20, codec)
	}, maxNodes)
}

func bitboardPerftBreadthPacked(maxNodes uint64, maxMB int) {
	bitboardPerftBreadthEncoded(maxNodes, maxMB, bitboard.PackedCodec)
}

func bitboardPerftBreadthPackedFixed(maxNodes uint64, maxMB int) {
	bitboardPerftBreadthEncoded(maxNodes, maxMB, bitboard.PackedFixedCodec)
}

// bitboardEncodeRoundtrip checks both codecs (including the Zobrist key) on all
// reference positions one move deep.
func bitboardEncodeRoundtrip() {
	for _, p := range chess.PerftPositions {
		root, err := bitboard.FromFEN(p.FEN)
		if err != nil {
			panic(err)
		}
		for _, m := range root.Moves() {
			b := root
			b.DoMove(m)
			for _, codec := range []bitboard.Codec{bitboard.PackedCodec, bitboard.PackedFixedCodec} {
				enc := codec.Append(b, nil)
				dec, n := codec.Decode(enc)
				if n != len(enc) || dec != b {
					panic(fmt.Sprintf("%s roundtrip failed for %s after %s", codec.Name, p.FEN, m))
				}
			}
		}
		fmt.Printf("%-36s packed %2d bytes\n", p.Name, len(root.AppendPacked(nil)))
	}
	fmt.Println("roundtrip ok")
}

func bitboardPerftParallel(maxNodes uint64, workers int) {
	bitboardInfo()
	workers = workersOrCPUs(workers)
	perft.Run(fmt.Sprintf("bitboard / parallel root split (%d workers)", workers), func(fen string, depth int) (uint64, error) {
		b, err := bitboard.FromFEN(fen)
		if err != nil {
			return 0, err
		}
		return bitboard.PerftParallel(&b, depth, workers), nil
	}, maxNodes)
}

func bitboardDivide(fen string, depth int) {
	b, err := bitboard.FromFEN(fen)
	if err != nil {
		panic(err)
	}
	fmt.Println(b.String())
	fmt.Printf("\ndivide depth %d:\n", depth)
	bitboard.PerftDivide(&b, depth)
}
