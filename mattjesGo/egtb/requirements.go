package egtb

import (
	"fmt"
	"os"
	"path/filepath"
)

// Requirement is what a measured generation of a material cost: the peak
// committed memory of the process, the time on the machine noted, and the
// compressed file size. Recorded by hand from measure.log, like the checksums,
// so that the engine can tell before a generation whether the machine can
// take it.
type Requirement struct {
	PeakMB  int // peak committed memory of the whole process
	Seconds int // generation only (the WDL and DTM files take longer to pack)
	FileMB  int // the DTM file (five pieces); the old quantised .bin for six
	Machine string
}

// Requirements holds the measured materials beyond the base.
var Requirements = map[string]Requirement{
	// five pieces, all 110: the sweep of 2026-10-23 on the home machine (Core Ultra 9 285H,
	// 14 workers, Rust, measure.log). The peak is the process peak since its start, which
	// only grows along the sweep (1.3 GB over the pawnless materials, 3.4 GB with one pawn,
	// 7.4 GB with two), so for a material it is an upper bound; the first material of every
	// plateau measured it.
	"KBBBK": {707, 2, 2, "285H"}, "KBBKB": {914, 3, 1, "285H"}, "KBBKN": {1024, 6, 10, "285H"},
	"KBBKQ": {1080, 13, 11, "285H"}, "KBBKR": {1082, 3, 1, "285H"}, "KBBNK": {1082, 7, 7, "285H"},
	"KBNKB": {1082, 9, 2, "285H"}, "KBNKN": {1204, 10, 5, "285H"}, "KBNKQ": {1241, 28, 25, "285H"},
	"KBNKR": {1328, 10, 2, "285H"}, "KBNNK": {1328, 6, 4, "285H"}, "KNNKB": {1328, 1, 0, "285H"},
	"KNNKN": {1328, 1, 0, "285H"}, "KNNKQ": {1328, 13, 10, "285H"}, "KNNKR": {1328, 2, 1, "285H"},
	"KNNNK": {1328, 2, 1, "285H"}, "KQBBK": {1328, 6, 1, "285H"}, "KQBKB": {1328, 19, 7, "285H"},
	"KQBKN": {1328, 19, 6, "285H"}, "KQBKQ": {1328, 20, 4, "285H"}, "KQBKR": {1328, 20, 11, "285H"},
	"KQBNK": {1328, 12, 1, "285H"}, "KQNKB": {1328, 18, 8, "285H"}, "KQNKN": {1328, 18, 8, "285H"},
	"KQNKQ": {1328, 20, 4, "285H"}, "KQNKR": {1328, 20, 11, "285H"}, "KQNNK": {1328, 5, 0, "285H"},
	"KQQBK": {1328, 6, 0, "285H"}, "KQQKB": {1328, 12, 2, "285H"}, "KQQKN": {1328, 11, 2, "285H"},
	"KQQKQ": {1328, 10, 3, "285H"}, "KQQKR": {1328, 12, 4, "285H"}, "KQQNK": {1328, 6, 0, "285H"},
	"KQQQK": {1328, 2, 0, "285H"}, "KQQRK": {1328, 6, 1, "285H"}, "KQRBK": {1328, 12, 1, "285H"},
	"KQRKB": {1328, 21, 3, "285H"}, "KQRKN": {1328, 21, 4, "285H"}, "KQRKQ": {1328, 25, 16, "285H"},
	"KQRKR": {1328, 22, 6, "285H"}, "KQRNK": {1328, 12, 1, "285H"}, "KQRRK": {1328, 6, 0, "285H"},
	"KRBBK": {1328, 6, 3, "285H"}, "KRBKB": {1328, 18, 13, "285H"}, "KRBKN": {1328, 18, 14, "285H"},
	"KRBKQ": {1328, 27, 18, "285H"}, "KRBKR": {1328, 12, 3, "285H"}, "KRBNK": {1328, 13, 6, "285H"},
	"KRNKB": {1328, 18, 13, "285H"}, "KRNKN": {1328, 18, 13, "285H"}, "KRNKQ": {1328, 28, 22, "285H"},
	"KRNKR": {1333, 12, 2, "285H"}, "KRNNK": {1333, 6, 3, "285H"}, "KRRBK": {1333, 6, 1, "285H"},
	"KRRKB": {1333, 10, 6, "285H"}, "KRRKN": {1333, 10, 5, "285H"}, "KRRKQ": {1333, 13, 8, "285H"},
	"KRRKR": {1333, 10, 7, "285H"}, "KRRNK": {1333, 6, 1, "285H"}, "KRRRK": {1333, 2, 0, "285H"},
	"KBBKP": {2483, 19, 18, "285H"}, "KBBPK": {3050, 22, 12, "285H"}, "KBNKP": {3050, 57, 65, "285H"},
	"KBNPK": {3413, 46, 25, "285H"}, "KBPKB": {3413, 37, 18, "285H"}, "KBPKN": {3413, 43, 32, "285H"},
	"KBPKQ": {3413, 76, 36, "285H"}, "KBPKR": {3413, 50, 19, "285H"}, "KNNKP": {3413, 16, 24, "285H"},
	"KNNPK": {3413, 20, 11, "285H"}, "KNPKB": {3413, 35, 17, "285H"}, "KNPKN": {3413, 39, 28, "285H"},
	"KNPKQ": {3413, 74, 48, "285H"}, "KNPKR": {3413, 50, 25, "285H"}, "KQBKP": {3413, 55, 6, "285H"},
	"KQBPK": {3413, 40, 3, "285H"}, "KQNKP": {3413, 53, 13, "285H"}, "KQNPK": {3413, 40, 3, "285H"},
	"KQPKB": {3413, 61, 33, "285H"}, "KQPKN": {3413, 61, 35, "285H"}, "KQPKQ": {3413, 74, 48, "285H"},
	"KQPKR": {3413, 79, 54, "285H"}, "KQQKP": {3413, 32, 4, "285H"}, "KQQPK": {3413, 19, 1, "285H"},
	"KQRKP": {3413, 65, 11, "285H"}, "KQRPK": {3413, 39, 3, "285H"}, "KRBKP": {3413, 53, 27, "285H"},
	"KRBPK": {3413, 43, 19, "285H"}, "KRNKP": {3413, 53, 28, "285H"}, "KRNPK": {3413, 43, 22, "285H"},
	"KRPKB": {3413, 60, 60, "285H"}, "KRPKN": {3418, 63, 57, "285H"}, "KRPKQ": {3418, 90, 88, "285H"},
	"KRPKR": {3432, 64, 47, "285H"}, "KRRKP": {3432, 32, 7, "285H"}, "KRRPK": {3432, 21, 2, "285H"},
	"KBPKP": {6963, 44, 30, "285H"}, "KBPPK": {7353, 19, 8, "285H"}, "KNPKP": {7353, 43, 35, "285H"},
	"KNPPK": {7357, 19, 8, "285H"}, "KPPKB": {7357, 18, 7, "285H"}, "KPPKN": {7357, 19, 11, "285H"},
	"KPPKQ": {7357, 28, 14, "285H"}, "KPPKR": {7357, 28, 20, "285H"}, "KQPKP": {7357, 48, 20, "285H"},
	"KQPPK": {7357, 18, 1, "285H"}, "KRPKP": {7357, 51, 27, "285H"}, "KRPPK": {7369, 19, 6, "285H"},
	"KPPKP": {7369, 20, 14, "285H"}, "KPPPK": {7369, 6, 2, "285H"},
	// six pieces, home machine (Core Ultra 9 285H, 14 workers), Rust, UCI mode
	"KRRKBN": {27450, 1348, 2427, "285H"},
}

// EstimateBytes returns the memory a generation of t needs: the measured peak
// when recorded, otherwise an estimate from the table size (table, four
// bitsets of 1/8, pending lists of about 1/4 as seen on KRRKBN) plus the
// direct dependencies that are not loaded yet.
func (s *Set) EstimateBytes(t *Table) (bytes int, measured bool) {
	if r, ok := Requirements[t.Mat.Name()]; ok {
		return r.PeakMB << 20, true
	}
	n := t.Size + t.Size/2 + t.Size/4
	for _, d := range t.Mat.Dependencies() {
		if dt := s.Find(d.Name()); dt == nil || dt.Values == nil {
			n += newTable(d).Size
		}
	}
	return n, false
}

// LogFileName is the measurement log in the cache directory, one line per
// generated table (egtb-measure and the UCI background generation).
const LogFileName = "measure.log"

// AppendLog appends one line to the measurement log of a cache directory.
func AppendLog(cacheDir, line string) error {
	if err := os.MkdirAll(cacheDir, 0o755); err != nil {
		return err
	}
	f, err := os.OpenFile(filepath.Join(cacheDir, LogFileName), os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	defer f.Close()
	_, err = fmt.Fprintln(f, line)
	return err
}

// LogLine formats the measurement log line of a generated table (the same
// fields from egtb-measure and from the UCI mode).
func (s *Set) LogLine(t *Table, st Stats, workers int, source string) string {
	peakCommit, peakWorking, _ := PeakMemory()
	return fmt.Sprintf("%s pieces=%d indices=%d legal=%d wins=%d losses=%d draws=%d longest_plies=%d levels=%d evaluations=%d seconds=%.1f workers=%d overflow=%v beyond=%d peak_commit_mb=%d peak_ws_mb=%d checksum=%016x %s",
		t.Mat.Name(), t.Mat.Pieces(), t.Size, st.Legal, st.Wins, st.Losses, st.Draws, st.MaxWin, st.Levels, st.Evaluations, st.Duration.Seconds(), workers, st.Overflow, st.Beyond, peakCommit>>20, peakWorking>>20, t.RawChecksum, source)
}
