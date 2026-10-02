package main

import (
	"fmt"
	"runtime"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mailbox"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/perft"
)

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
	if workers <= 0 {
		workers = runtime.NumCPU()
	}
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
