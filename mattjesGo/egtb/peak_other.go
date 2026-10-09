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
	a, ok1 := procValue("/proc/self/status", "VmPeak:")
	b, ok2 := procValue("/proc/self/status", "VmHWM:")
	return a, b, ok1 && ok2
}

// AvailableMemory returns the physical memory of the machine and the part of
// it that is free right now, in bytes, from /proc/meminfo.
func AvailableMemory() (total, available uint64, ok bool) {
	a, ok1 := procValue("/proc/meminfo", "MemTotal:")
	b, ok2 := procValue("/proc/meminfo", "MemAvailable:")
	return a, b, ok1 && ok2
}

// procValue reads a "key: value kB" line of a proc file as bytes.
func procValue(path, key string) (uint64, bool) {
	data, err := os.ReadFile(path)
	if err != nil {
		return 0, false
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
	return kb(key)
}
