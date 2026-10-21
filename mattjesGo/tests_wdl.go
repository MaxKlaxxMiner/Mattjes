package main

import (
	"fmt"
	"strings"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/mateab"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/matepn"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/tt"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/wdlpn"
)

// --- milestone 7: WDL search ---

// wdlpnSolve runs the WDL test positions (chess.WdlPositions): first "the
// side to move wins" (root = attacker), if that is disproven "the opponent
// wins" (root = defender); both disproven = draw. Tables are off unless
// asked for, so that the search itself is tested on small material.
//
// names selects positions by name (semicolon separated, "" = all). Options are
// letters as in matepn: m = mobility initialisation, e/E/x/X = epsilon 1/8,
// 1/2, 1, 2, b = bucket table with priority classes (the UCI layout),
// T = the four-piece base as oracle (otherwise only dead material).
func wdlpnSolve(names string, sizeMB int, codecName string, options string) {
	has := func(o byte) bool { return strings.IndexByte(options, o) >= 0 }
	selected := func(p chess.WdlPosition) bool {
		switch names {
		case "":
			return true
		case "wins":
			return p.Result != chess.Draw
		case "draws":
			return p.Result == chess.Draw
		}
		for _, n := range strings.Split(names, ";") {
			if strings.TrimSpace(n) == p.Name {
				return true
			}
		}
		return false
	}
	mobility := has('m')
	epsilon := 0
	for _, o := range []struct {
		letter byte
		eps    int
	}{{'e', 1}, {'E', 4}, {'x', 8}, {'X', 16}} {
		if has(o.letter) {
			epsilon = o.eps
		}
	}
	var table pnTable
	layout := "direct-mapped TT"
	if has('b') {
		bt := tt.NewBuckets(sizeMB)
		bt.SetPriorityBits(wdlpn.PriorityBits)
		table = bt
		layout = "bucket TT with priority replacement"
	} else {
		table = tt.New(sizeMB)
	}
	var codec wdlpn.Codec
	switch codecName {
	case "sat":
		codec = matepn.Saturating{Bits: wdlpn.CodecBits(table)}
	case "float":
		codec = matepn.Float{Bits: wdlpn.CodecBits(table)}
	default:
		panic("codec sat or float")
	}
	var oracle mateab.Oracle = mateab.Material{}
	title := "dead material only"
	if has('T') {
		oracle = mateab.Tables{Set: egtbLoadOrGenerate(12)}
		title = "four-piece tables as oracle"
	}
	fmt.Printf("=== wdlpn, %s %d MB (%d value bits, %d for the codec), codec %s, mobility %v, epsilon %d/8, %s ===\n", layout, sizeMB, table.ValueBits(), wdlpn.CodecBits(table), codec.Name(), mobility, epsilon, title)
	ok := true
	var totalNodes, totalStores uint64
	var totalTime time.Duration
	for i, p := range chess.WdlPositions {
		if !selected(p) {
			continue
		}
		fmt.Printf("[%d] %s  %s  expected %s (%d/%d/%d)\n", i+1, p.Name, p.FEN, p.Result, p.Wins, p.Draws, p.Losses)
		b, err := bitboard.FromFEN(p.FEN)
		if err != nil {
			panic(err)
		}
		table.Clear()
		table.ResetStats()
		s := wdlpn.New(oracle, table, codec, mobility)
		s.Epsilon = epsilon
		start := time.Now()
		qStart := start
		s.Progress = func(nodes uint64) {
			st := table.Counters()
			leaves, hits, condHits, condMiss, condSize, capped := s.Counters()
			fmt.Printf("              ... %s nodes, %s, %s leaves, %s hits, %s/%s cond hits/misses (%s kept), %s capped, tt %s stores, %s replaced, %s used\n",
				perftGroup(nodes), fmtMs(time.Since(qStart)), perftGroup(leaves), perftGroup(hits), perftGroup(condHits), perftGroup(condMiss), perftGroup(condSize), perftGroup(capped), perftGroup(st.Stores), perftGroup(st.Replaced), perftGroup(uint64(table.Used())))
		}
		var nodes uint64
		got := chess.Draw
		var pv []chess.Move
		for q, attacks := range []bool{true, false} {
			question := "side to move wins"
			if !attacks {
				question = "opponent wins"
			}
			qStart = time.Now()
			r := s.Solve(&b, attacks)
			nodes += r.Nodes
			verdict := fmt.Sprintf("undecided (pn %d, dn %d)", r.Pn, r.Dn)
			switch {
			case r.Aborted:
				verdict = "aborted"
			case r.Proven:
				verdict = "proven"
			case r.Disproven:
				verdict = "disproven"
			}
			fmt.Printf("    q%d %-18s %-10s %12s nodes, %s leaves, %s tt hits, %s/%s conditional hits/misses (%s kept), %s capped, %s\n", q+1, question+":", verdict, perftGroup(r.Nodes), perftGroup(r.Leaves), perftGroup(r.TTHits), perftGroup(r.CondHits), perftGroup(r.CondMiss), perftGroup(uint64(r.CondSize)), perftGroup(r.Capped), fmtMs(time.Since(qStart)))
			if r.Proven {
				got, pv = chess.Win, r.PV
				if !attacks {
					got = chess.Loss
				}
				break
			}
			if !r.Disproven {
				got = 99 // undecided
				break
			}
		}
		elapsed := time.Since(start)
		st := table.Counters()
		totalNodes += nodes
		totalStores += st.Stores
		totalTime += elapsed
		verdict := "ok"
		if got != p.Result {
			verdict = "FAIL"
			ok = false
		}
		result := got.String()
		if got == 99 {
			result = "undecided"
		}
		fmt.Printf("    result %s (%s): %s nodes in %s, %s stores, %s replaced, %s dropped, %s entries used (%.1f%%)\n",
			result, verdict, perftGroup(nodes), fmtMs(elapsed), perftGroup(st.Stores), perftGroup(st.Replaced), perftGroup(st.Dropped), perftGroup(uint64(table.Used())), 100*float64(table.Used())/float64(table.Slots()))
		if len(pv) > 0 {
			fmt.Printf("    line (%d plies): %s\n", len(pv), pvString(pv))
		}
	}
	fmt.Printf("--- total: %s nodes, %s stores in %s", perftGroup(totalNodes), perftGroup(totalStores), fmtMs(totalTime))
	if ok {
		fmt.Println("  [all ok]")
	} else {
		fmt.Println("  [FAILURES]")
	}
	fmt.Println()
}
