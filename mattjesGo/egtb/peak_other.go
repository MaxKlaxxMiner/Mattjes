//go:build !windows

package egtb

import (
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

// PhysicalCores returns the number of processor cores of the machine, SMT
// siblings counted once (a generation beside the search takes cores - 1
// workers, so the search keeps a core of its own): the distinct
// (package, core) pairs of the sysfs topology. 0 when unknown.
func PhysicalCores() int {
	paths, _ := filepath.Glob("/sys/devices/system/cpu/cpu[0-9]*/topology/core_id")
	seen := map[string]bool{}
	for _, p := range paths {
		core, err1 := os.ReadFile(p)
		pkg, err2 := os.ReadFile(filepath.Join(filepath.Dir(p), "physical_package_id"))
		if err1 != nil || err2 != nil {
			continue
		}
		seen[strings.TrimSpace(string(pkg))+":"+strings.TrimSpace(string(core))] = true
	}
	return len(seen)
}

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
