//go:build windows

package syzygy

import (
	"os"
	"syscall"
	"unsafe"
)

// mapFile maps a whole file read-only into memory; the returned closer
// unmaps it. The operating system pages the table in on demand and keeps
// it in its file cache, which is what a probe-heavy search wants.
func mapFile(path string) ([]byte, func(), error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, nil, err
	}
	fi, err := f.Stat()
	if err != nil {
		f.Close()
		return nil, nil, err
	}
	size := fi.Size()
	if size == 0 {
		f.Close()
		return nil, func() {}, nil
	}
	h, err := syscall.CreateFileMapping(syscall.Handle(f.Fd()), nil, syscall.PAGE_READONLY, uint32(size>>32), uint32(size), nil)
	if err != nil {
		f.Close()
		return nil, nil, err
	}
	addr, err := syscall.MapViewOfFile(h, syscall.FILE_MAP_READ, 0, 0, uintptr(size))
	if err != nil {
		syscall.CloseHandle(h)
		f.Close()
		return nil, nil, err
	}
	// the view lies outside the Go heap: unsafe.Add on a nil pointer is
	// the address, without vet's uintptr-to-pointer complaint
	data := unsafe.Slice((*byte)(unsafe.Add(unsafe.Pointer(nil), addr)), size)
	closer := func() {
		syscall.UnmapViewOfFile(addr)
		syscall.CloseHandle(h)
		f.Close()
	}
	return data, closer, nil
}
