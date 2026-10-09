//go:build windows

package egtb

import (
	"syscall"
	"unsafe"
)

// processMemoryCounters mirrors PROCESS_MEMORY_COUNTERS from psapi.h, field
// order matters.
type processMemoryCounters struct {
	cb                         uint32
	pageFaultCount             uint32
	peakWorkingSetSize         uintptr
	workingSetSize             uintptr
	quotaPeakPagedPoolUsage    uintptr
	quotaPagedPoolUsage        uintptr
	quotaPeakNonPagedPoolUsage uintptr
	quotaNonPagedPoolUsage     uintptr
	pagefileUsage              uintptr
	peakPagefileUsage          uintptr
}

var (
	kernel32              = syscall.NewLazyDLL("kernel32.dll")
	procGetCurrentProcess = kernel32.NewProc("GetCurrentProcess")
	procMemoryInfo        = kernel32.NewProc("K32GetProcessMemoryInfo")
)

// PeakMemory returns the peak committed size and the peak working set of the
// process in bytes (the committed size is what the task manager calls
// "zugesicherte Größe"; the working set can be a few GB lower).
func PeakMemory() (committed, workingSet uint64, ok bool) {
	var c processMemoryCounters
	c.cb = uint32(unsafe.Sizeof(c))
	h, _, _ := procGetCurrentProcess.Call()
	r, _, _ := procMemoryInfo.Call(h, uintptr(unsafe.Pointer(&c)), uintptr(c.cb))
	if r == 0 {
		return 0, 0, false
	}
	return uint64(c.peakPagefileUsage), uint64(c.peakWorkingSetSize), true
}
