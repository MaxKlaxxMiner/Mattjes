// Package perft runs generator-independent perft verification and prints
// speed and memory numbers. Generators plug in via a function that takes a FEN.
package perft

import (
	"fmt"
	"runtime"
	"strconv"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// Func computes the perft node count of a FEN position at the given depth.
// It returns an error if the depth cannot be computed (e.g. memory limit).
type Func func(fen string, depth int) (uint64, error)

// Run verifies fn against all reference positions, skipping depths whose
// expected node count exceeds maxNodes. It returns false if any count was wrong.
func Run(title string, fn Func, maxNodes uint64) bool {
	return RunPrepared(title, nil, fn, maxNodes)
}

// RunPrepared is Run with a hook that runs before every (position, depth) outside
// the measured time, e.g. to clear a transposition table (a 1 GB memset would
// otherwise show up as perft time).
func RunPrepared(title string, prepare func(), fn Func, maxNodes uint64) bool {
	fmt.Printf("=== %s ===\n", title)
	ok := true
	var totalNodes uint64
	var totalTime time.Duration

	for i, p := range chess.PerftPositions {
		fmt.Printf("[%d] %s\n    %s\n", i+1, p.Name, p.FEN)
		for depth := 1; depth <= len(p.Nodes); depth++ {
			expected := p.Nodes[depth-1]
			if expected > maxNodes {
				break
			}
			if prepare != nil {
				prepare()
			}

			var before, after runtime.MemStats
			runtime.GC()
			runtime.ReadMemStats(&before)
			start := time.Now()
			nodes, err := fn(p.FEN, depth)
			elapsed := time.Since(start)
			runtime.ReadMemStats(&after)

			if err != nil {
				fmt.Printf("    depth %d: skipped (%v)\n", depth, err)
				break
			}
			totalNodes += nodes
			totalTime += elapsed

			status := "ok"
			if nodes != expected {
				status = fmt.Sprintf("FAIL expected %s", group(expected))
				ok = false
			}
			fmt.Printf("    depth %d: %16s %-28s %10s %8s  alloc %s\n",
				depth, group(nodes), status, fmtDuration(elapsed), mnps(nodes, elapsed),
				fmtBytes(after.TotalAlloc-before.TotalAlloc))
		}
	}

	fmt.Printf("--- total: %s nodes in %s = %s", group(totalNodes), fmtDuration(totalTime), mnps(totalNodes, totalTime))
	if ok {
		fmt.Println("  [all ok]")
	} else {
		fmt.Println("  [FAILURES]")
	}
	fmt.Println()
	return ok
}

// group formats an integer with thousands separators.
func group(n uint64) string {
	s := strconv.FormatUint(n, 10)
	out := make([]byte, 0, len(s)+len(s)/3)
	for i, c := range []byte(s) {
		if i > 0 && (len(s)-i)%3 == 0 {
			out = append(out, ',')
		}
		out = append(out, c)
	}
	return string(out)
}

func mnps(nodes uint64, d time.Duration) string {
	if d < time.Millisecond {
		return ""
	}
	return fmt.Sprintf("%.1f Mn/s", float64(nodes)/d.Seconds()/1e6)
}

func fmtDuration(d time.Duration) string {
	switch {
	case d < time.Millisecond:
		return fmt.Sprintf("%d µs", d.Microseconds())
	case d < time.Second:
		return fmt.Sprintf("%.1f ms", float64(d.Microseconds())/1000)
	default:
		return fmt.Sprintf("%.2f s", d.Seconds())
	}
}

func fmtBytes(n uint64) string {
	switch {
	case n < 1<<10:
		return fmt.Sprintf("%d B", n)
	case n < 1<<20:
		return fmt.Sprintf("%.1f KB", float64(n)/(1<<10))
	case n < 1<<30:
		return fmt.Sprintf("%.1f MB", float64(n)/(1<<20))
	default:
		return fmt.Sprintf("%.2f GB", float64(n)/(1<<30))
	}
}
