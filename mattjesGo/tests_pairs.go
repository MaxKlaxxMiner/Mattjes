package main

import (
	"fmt"
	"math/rand/v2"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/pairs"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/syzygy"
)

// --- package pairs: Syzygy's compression scheme on our WDL symbol streams ---

// pairsSyzygy packs the decoded streams of Syzygy tables ("KBNvK;KQvKR")
// with our packer and prints our size next to the bytes the stream takes in
// the Syzygy file: the same data, the same don't-care fill, so the
// difference is the packer alone.
func pairsSyzygy(dir string, names string, opt pairs.Options) {
	tb, err := syzygy.Open(dir)
	if err != nil {
		fmt.Printf("syzygy: %v\n", err)
		return
	}
	fmt.Printf("=== pairs on Syzygy's own streams: blocks %d bytes, %d pairs per pass, min count %d, %d symbols, symbol length %d ===\n", opt.BlockBytes, opt.PairsPerPass, opt.MinCount, opt.MaxSymbols, opt.MaxSymbolLen)
	var sumOurs, sumTheirs int64
	for _, name := range strings.Split(names, ";") {
		name = strings.TrimSpace(name)
		values, bytes := tb.Streams(name)
		if values == nil {
			fmt.Printf("%s: no table\n", name)
			continue
		}
		for i, v := range values {
			packed, st := pairs.PackStats(v, 5, opt)
			t, err := pairs.Open(packed)
			if err != nil {
				panic(err)
			}
			for j := 0; j < len(v); j += 997 {
				if t.Value(j) != v[j] {
					panic(fmt.Sprintf("%s stream %d: position %d decodes wrong", name, i, j))
				}
			}
			fmt.Printf("%-6s stream %d: %9d positions, ours %8d, syzygy %8d, ratio %.2f | %s\n", name, i, len(v), st.Bytes, bytes[i], float64(st.Bytes)/float64(bytes[i]), st)
			sumOurs += int64(st.Bytes)
			sumTheirs += int64(bytes[i])
		}
	}
	fmt.Printf("total: ours %d, syzygy %d, ratio %.2f\n\n", sumOurs, sumTheirs, float64(sumOurs)/float64(max(sumTheirs, 1)))
}

// pairsMeasure packs every symbol stream dir/<material>.wdl (as egtbWdlSize
// dumps them: one byte per position, values 0..2) with the pairs packer,
// checks every position against the stream, times random probes and prints
// the size next to the Syzygy WDL file of the material. names: semicolon
// separated materials, "" = all files in the folder.
func pairsMeasure(dir string, names string, opt pairs.Options, verify bool) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		panic(err)
	}
	var files []string
	for _, e := range entries {
		if strings.HasSuffix(e.Name(), ".wdl") {
			name := strings.TrimSuffix(e.Name(), ".wdl")
			if names == "" || strings.Contains(";"+names+";", ";"+name+";") {
				files = append(files, name)
			}
		}
	}
	sort.Strings(files)
	fmt.Printf("=== pairs: blocks %d bytes, index 2^%d (0 = auto), %d pairs per pass, min count %d, %d symbols, symbol length %d ===\n", opt.BlockBytes, opt.IndexBits, opt.PairsPerPass, opt.MinCount, opt.MaxSymbols, opt.MaxSymbolLen)
	fmt.Printf("%-6s %11s %9s %9s %6s | %9s %5s %7s %4s %3s %4s | %6s %6s\n", "table", "positions", "packed", "syzygy", "ratio", "tokens", "syms", "blocks", "bits", "idx", "pass", "pack s", "probe")
	var sumPacked, sumSyzygy int64
	var sumPack time.Duration
	ok := true
	for _, name := range files {
		values, err := os.ReadFile(filepath.Join(dir, name+".wdl"))
		if err != nil {
			panic(err)
		}
		start := time.Now()
		packed, st := pairs.PackStats(values, 3, opt)
		packTime := time.Since(start)
		t, err := pairs.Open(packed)
		if err != nil {
			panic(err)
		}
		if verify {
			var wg sync.WaitGroup
			var bad [64]int
			workers := max(1, opt.Workers)
			chunk := (len(values) + workers - 1) / workers
			for w := 0; w < workers; w++ {
				lo, hi := w*chunk, min(len(values), (w+1)*chunk)
				wg.Add(1)
				go func(w, lo, hi int) {
					defer wg.Done()
					for i := lo; i < hi; i++ {
						if t.Value(i) != values[i] {
							bad[w%64]++
						}
					}
				}(w, lo, hi)
			}
			wg.Wait()
			total := 0
			for _, b := range bad {
				total += b
			}
			if total > 0 {
				fmt.Printf("  %s: %d of %d positions decode wrong\n", name, total, len(values))
				ok = false
			}
		}
		// random probes, single thread
		rng := rand.New(rand.NewPCG(1, 2))
		probes := 1 << 20
		start = time.Now()
		sink := 0
		for i := 0; i < probes; i++ {
			sink += int(t.Value(rng.IntN(len(values))))
		}
		probe := time.Since(start) / time.Duration(probes)
		m, err := egtb.Parse(name)
		if err != nil {
			panic(err)
		}
		syzygy := syzygyFileSize(m)
		ratio := "-"
		if syzygy > 0 {
			ratio = fmt.Sprintf("%.2f", float64(st.Bytes)/float64(syzygy))
		}
		fmt.Printf("%-6s %11d %9d %9d %6s | %9d %5d %7d %4d %3d %4d | %6.2f %4d ns%s\n", name, st.Positions, st.Bytes, syzygy, ratio, st.Tokens, st.Symbols, st.Blocks, st.MaxCodeLen, st.IndexBits, st.Passes, packTime.Seconds(), probe.Nanoseconds(), map[bool]string{true: "", false: " (sink)"}[sink >= 0])
		sumPacked += int64(st.Bytes)
		sumSyzygy += syzygy
		sumPack += packTime
	}
	verdict := "all ok"
	if !ok {
		verdict = "FAILURES"
	}
	fmt.Printf("%-6s %11s %9d %9d %6.2f | packed in %.1f s, %s\n\n", "total", "", sumPacked, sumSyzygy, float64(sumPacked)/float64(max(sumSyzygy, 1)), sumPack.Seconds(), verdict)
}
