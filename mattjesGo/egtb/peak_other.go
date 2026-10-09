//go:build !windows

package egtb

import (
	"os"
	"strconv"
	"strings"
)

// PeakMemory returns the peak virtual size and the peak resident set of the
// process in bytes from /proc/self/status (Linux has no committed-size
// counter, so VmPeak stands in for it).
func PeakMemory() (committed, workingSet uint64, ok bool) {
	data, err := os.ReadFile("/proc/self/status")
	if err != nil {
		return 0, 0, false
	}
	kb := func(key string) (uint64, bool) {
		for _, line := range strings.Split(string(data), "\n") {
			if strings.HasPrefix(line, key) {
				fields := strings.Fields(line)
				if len(fields) >= 2 {
					v, err := strconv.ParseUint(fields[1], 10, 64)
					return v * 1024, err == nil
				}
			}
		}
		return 0, false
	}
	var ok1, ok2 bool
	committed, ok1 = kb("VmPeak:")
	workingSet, ok2 = kb("VmHWM:")
	return committed, workingSet, ok1 && ok2
}
