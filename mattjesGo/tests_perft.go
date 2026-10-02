package main

import (
	"fmt"
	"runtime"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mailbox"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/perft"
)

func workersOrCPUs(workers int) int {
	if workers <= 0 {
		return runtime.NumCPU()
	}
	return workers
}

// --- mailbox generator ---

func mailboxPerftRecursive(maxNodes uint64) {
	perft.Run("mailbox / recursive (make-unmake)", func(fen string, depth int) (uint64, error) {
		b, err := mailbox.FromFEN(fen)
		if err != nil {
			return 0, err
		}
		return mailbox.PerftRecursive(&b, depth), nil
	}, maxNodes)
}

func mailboxPerftIterative(maxNodes uint64) {
	fmt.Printf("frame size per ply: %d bytes (board %d bytes)\n", mailbox.FrameSize, mailbox.BoardSize)
	perft.Run("mailbox / iterative (explicit stack, copy-make)", func(fen string, depth int) (uint64, error) {
		b, err := mailbox.FromFEN(fen)
		if err != nil {
			return 0, err
		}
		return mailbox.PerftIterative(&b, depth), nil
	}, maxNodes)
}

func mailboxPerftBreadth(maxNodes uint64, maxMB int) {
	fmt.Printf("board size: %d bytes, memory limit: %d MB\n", mailbox.BoardSize, maxMB)
	perft.Run("mailbox / breadth-first (full position lists per ply)", func(fen string, depth int) (uint64, error) {
		b, err := mailbox.FromFEN(fen)
		if err != nil {
			return 0, err
		}
		return mailbox.PerftBreadth(&b, depth, maxMB<<20)
	}, maxNodes)
}

func mailboxPerftParallel(maxNodes uint64, workers int) {
	workers = workersOrCPUs(workers)
	perft.Run(fmt.Sprintf("mailbox / parallel root split (%d workers)", workers), func(fen string, depth int) (uint64, error) {
		b, err := mailbox.FromFEN(fen)
		if err != nil {
			return 0, err
		}
		return mailbox.PerftParallel(&b, depth, workers), nil
	}, maxNodes)
}

func mailboxDivide(fen string, depth int) {
	b, err := mailbox.FromFEN(fen)
	if err != nil {
		panic(err)
	}
	fmt.Println(b.String())
	fmt.Printf("\ndivide depth %d:\n", depth)
	mailbox.PerftDivide(&b, depth)
}

// --- bitboard generator ---

func bitboardInfo() {
	fmt.Printf("bitboard tables: %d magic entries (%d KB), init %s\n", bitboard.AttackTableSize, bitboard.AttackTableSize*8>>10, bitboard.InitDuration)
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
	fmt.Printf("frame size per ply: %d bytes (board %d bytes)\n", bitboard.FrameSize, bitboard.BoardSize)
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
	fmt.Printf("board size: %d bytes, memory limit: %d MB\n", bitboard.BoardSize, maxMB)
	perft.Run("bitboard / breadth-first (full position lists per ply)", func(fen string, depth int) (uint64, error) {
		b, err := bitboard.FromFEN(fen)
		if err != nil {
			return 0, err
		}
		return bitboard.PerftBreadth(&b, depth, maxMB<<20)
	}, maxNodes)
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
