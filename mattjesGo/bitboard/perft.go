package bitboard

import (
	"fmt"
	"sync"
	"sync/atomic"
	"unsafe"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// The perft variants mirror package mailbox exactly, see there for the descriptions.

func PerftRecursive(b *Board, depth int) uint64 {
	var buf chess.MoveBuffer
	n := b.GenMoves(&buf)
	if depth <= 1 {
		return uint64(n)
	}
	var total uint64
	s := b.State()
	for i := 0; i < n; i++ {
		b.DoMove(buf[i])
		total += PerftRecursive(b, depth-1)
		b.UndoMove(buf[i], s)
	}
	return total
}

type frame struct {
	board Board
	moves chess.MoveBuffer
	count int
	next  int
}

const FrameSize = int(unsafe.Sizeof(frame{}))
const BoardSize = int(unsafe.Sizeof(Board{}))

func PerftIterative(root *Board, depth int) uint64 {
	stack := make([]frame, depth)
	stack[0].board = *root
	stack[0].count = stack[0].board.GenMoves(&stack[0].moves)
	if depth <= 1 {
		return uint64(stack[0].count)
	}

	var total uint64
	d := 0
	for {
		f := &stack[d]
		if f.next >= f.count {
			if d == 0 {
				return total
			}
			d--
			continue
		}
		m := f.moves[f.next]
		f.next++

		child := &stack[d+1]
		child.board = f.board
		child.board.DoMove(m)
		child.count = child.board.GenMoves(&child.moves)
		child.next = 0
		if d+1 == depth-1 {
			total += uint64(child.count)
		} else {
			d++
		}
	}
}

func PerftBreadth(root *Board, depth int, maxBytes int) (uint64, error) {
	level := []Board{*root}
	var buf chess.MoveBuffer

	for ply := 1; ply < depth; ply++ {
		var childCount int
		for i := range level {
			childCount += level[i].GenMoves(&buf)
		}
		if need := childCount * BoardSize; need > maxBytes {
			return 0, fmt.Errorf("ply %d needs %d positions = %d MB, limit is %d MB", ply, childCount, need>>20, maxBytes>>20)
		}
		next := make([]Board, 0, childCount)
		for i := range level {
			n := level[i].GenMoves(&buf)
			for j := 0; j < n; j++ {
				child := level[i]
				child.DoMove(buf[j])
				next = append(next, child)
			}
		}
		level = next
	}

	var total uint64
	for i := range level {
		total += uint64(level[i].GenMoves(&buf))
	}
	return total, nil
}

// Codec is a compact position encoding for PerftBreadthEncoded.
type Codec struct {
	Name     string
	MaxBytes int
	Append   func(b Board, dst []byte) []byte // by value, see AppendFastFen
	Decode   func(src []byte) (Board, int)
}

var FastFenCodec = Codec{"fastfen", MaxFastFenBytes, Board.AppendFastFen, DecodeFastFen}
var PackedCodec = Codec{"packed", MaxPackedBytes, Board.AppendPacked, DecodePacked}

// PerftBreadthEncoded is PerftBreadth with every ply stored as one byte stream
// of compactly encoded positions instead of a slice of 184-byte boards.
// Positions are decoded, expanded and the children encoded into the next stream.
// Prints the size of the largest ply.
func PerftBreadthEncoded(root *Board, depth int, maxBytes int, codec Codec) (uint64, error) {
	level := codec.Append(*root, nil)
	levelCount := 1
	peakBytes, peakCount := len(level), 1
	var buf chess.MoveBuffer

	for ply := 1; ply < depth; ply++ {
		// first pass: count children, estimate the stream size from the current average record size
		childCount := 0
		for i := 0; i < len(level); {
			b, n := codec.Decode(level[i:])
			i += n
			childCount += b.GenMoves(&buf)
		}
		estimate := childCount*len(level)/levelCount + childCount
		if estimate > maxBytes {
			return 0, fmt.Errorf("ply %d needs %d positions ≈ %d MB, limit is %d MB", ply, childCount, estimate>>20, maxBytes>>20)
		}
		next := make([]byte, 0, estimate)
		for i := 0; i < len(level); {
			b, n := codec.Decode(level[i:])
			i += n
			moves := b.GenMoves(&buf)
			for j := 0; j < moves; j++ {
				child := b
				child.DoMove(buf[j])
				next = codec.Append(child, next)
			}
		}
		if len(next) > maxBytes {
			return 0, fmt.Errorf("ply %d needs %d MB, limit is %d MB", ply, len(next)>>20, maxBytes>>20)
		}
		level, levelCount = next, childCount
		if len(level) > peakBytes {
			peakBytes, peakCount = len(level), levelCount
		}
	}

	var total uint64
	for i := 0; i < len(level); {
		b, n := codec.Decode(level[i:])
		i += n
		total += uint64(b.GenMoves(&buf))
	}
	if peakCount > 1000 {
		fmt.Printf("             %s: largest ply %d positions in %.1f MB = %.1f bytes/position\n", codec.Name, peakCount, float64(peakBytes)/(1<<20), float64(peakBytes)/float64(peakCount))
	}
	return total, nil
}

func PerftParallel(root *Board, depth int, workers int) uint64 {
	var buf chess.MoveBuffer
	n := root.GenMoves(&buf)
	if depth <= 1 {
		return uint64(n)
	}
	if workers < 1 {
		workers = 1
	}

	var total atomic.Uint64
	var nextMove atomic.Int32
	var wg sync.WaitGroup
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			var sum uint64
			for {
				i := int(nextMove.Add(1)) - 1
				if i >= n {
					break
				}
				b := *root
				b.DoMove(buf[i])
				sum += PerftRecursive(&b, depth-1)
			}
			total.Add(sum)
		}()
	}
	wg.Wait()
	return total.Load()
}

func PerftDivide(b *Board, depth int) uint64 {
	var buf chess.MoveBuffer
	n := b.GenMoves(&buf)
	var total uint64
	s := b.State()
	for i := 0; i < n; i++ {
		b.DoMove(buf[i])
		var sub uint64 = 1
		if depth > 1 {
			sub = PerftRecursive(b, depth-1)
		}
		b.UndoMove(buf[i], s)
		fmt.Printf("  %-6s %12d\n", buf[i].UCI(), sub)
		total += sub
	}
	fmt.Printf("  total  %12d (%d moves)\n", total, n)
	return total
}
