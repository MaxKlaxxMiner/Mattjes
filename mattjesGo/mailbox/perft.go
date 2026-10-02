package mailbox

import (
	"fmt"
	"sync"
	"sync/atomic"
	"unsafe"
)

// PerftRecursive is the classic recursive perft with make/unmake on a single board.
// Leaf nodes are bulk-counted: at depth 1 the number of legal moves is returned
// without playing them.
func PerftRecursive(b *Board, depth int) uint64 {
	var buf MoveBuffer
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

// frame is one ply of the explicit search stack used by PerftIterative.
type frame struct {
	board Board
	moves MoveBuffer
	count int
	next  int
}

// FrameSize is the memory per ply of PerftIterative in bytes.
const FrameSize = int(unsafe.Sizeof(frame{}))

// PerftIterative is a list-based depth-first perft without recursion and
// without UndoMove: every ply owns a copy of the board ("copy-make"), the
// child position is created by copying the parent and playing one move.
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
			total += uint64(child.count) // bulk count, same as the recursive version
		} else {
			d++
		}
	}
}

// BoardSize is the memory per stored position in bytes.
const BoardSize = int(unsafe.Sizeof(Board{}))

// PerftBreadth is a breadth-first perft: every ply is a complete list of all
// positions of that ply. It needs memory proportional to the number of nodes
// of the second-to-last ply and refuses to exceed maxBytes. It exists to
// measure what storing whole position lists costs.
func PerftBreadth(root *Board, depth int, maxBytes int) (uint64, error) {
	level := []Board{*root}
	var buf MoveBuffer

	for ply := 1; ply < depth; ply++ {
		// first pass: count the children so the next level can be allocated exactly once
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

// PerftParallel splits the root moves over `workers` goroutines, each running
// PerftRecursive on its own board copy. The order of root moves does not matter.
func PerftParallel(root *Board, depth int, workers int) uint64 {
	var buf MoveBuffer
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

// PerftDivide prints the node count below every root move. This is the standard
// tool to locate move generator bugs by comparing with another engine.
func PerftDivide(b *Board, depth int) uint64 {
	var buf MoveBuffer
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
