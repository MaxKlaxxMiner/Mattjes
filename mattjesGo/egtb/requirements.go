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
	Seconds int
	FileMB  int
	Machine string
}

// Requirements holds the measured materials beyond the base.
var Requirements = map[string]Requirement{
	// five pieces, work machine (i5, 12 threads), base loaded
	"KBBBK": {616, 4, 11, "i5"},
	"KBNKQ": {744, 49, 109, "i5"},
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
