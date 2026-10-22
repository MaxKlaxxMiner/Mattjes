//! A read-only memory mapping of a whole file, without a crate: the two
//! system calls Windows and Unix need, declared by hand.

use std::fs::File;
use std::io;
use std::path::Path;

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileMappingW(file: *mut c_void, attrs: *mut c_void, protect: u32, size_high: u32, size_low: u32, name: *const u16) -> *mut c_void;
        fn MapViewOfFile(mapping: *mut c_void, access: u32, offset_high: u32, offset_low: u32, bytes: usize) -> *mut c_void;
        fn UnmapViewOfFile(addr: *const c_void) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    const PAGE_READONLY: u32 = 0x02;
    const FILE_MAP_READ: u32 = 0x0004;

    pub struct Mapping {
        addr: *mut c_void,
        len: usize,
        _file: File,
    }
    // the view is read-only memory outside the heap, shared between threads
    unsafe impl Send for Mapping {}
    unsafe impl Sync for Mapping {}

    impl Mapping {
        pub fn open(path: &Path) -> io::Result<Mapping> {
            let file = File::open(path)?;
            let len = file.metadata()?.len() as usize;
            if len == 0 {
                return Ok(Mapping { addr: std::ptr::null_mut(), len: 0, _file: file });
            }
            unsafe {
                let mapping = CreateFileMappingW(file.as_raw_handle(), std::ptr::null_mut(), PAGE_READONLY, (len >> 32) as u32, len as u32, std::ptr::null());
                if mapping.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let addr = MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, len);
                CloseHandle(mapping); // the view keeps the mapping alive
                if addr.is_null() {
                    return Err(io::Error::last_os_error());
                }
                Ok(Mapping { addr, len, _file: file })
            }
        }

        pub fn bytes(&self) -> &[u8] {
            if self.len == 0 {
                return &[];
            }
            unsafe { std::slice::from_raw_parts(self.addr as *const u8, self.len) }
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            if !self.addr.is_null() {
                unsafe {
                    UnmapViewOfFile(self.addr);
                }
            }
        }
    }
}

#[cfg(unix)]
mod imp {
    use super::*;
    use std::ffi::c_void;
    use std::os::unix::io::AsRawFd;

    extern "C" {
        fn mmap(addr: *mut c_void, len: usize, prot: i32, flags: i32, fd: i32, offset: i64) -> *mut c_void;
        fn munmap(addr: *mut c_void, len: usize) -> i32;
    }
    const PROT_READ: i32 = 1;
    const MAP_SHARED: i32 = 1;

    pub struct Mapping {
        addr: *mut c_void,
        len: usize,
    }
    unsafe impl Send for Mapping {}
    unsafe impl Sync for Mapping {}

    impl Mapping {
        pub fn open(path: &Path) -> io::Result<Mapping> {
            let file = File::open(path)?;
            let len = file.metadata()?.len() as usize;
            if len == 0 {
                return Ok(Mapping { addr: std::ptr::null_mut(), len: 0 });
            }
            let addr = unsafe { mmap(std::ptr::null_mut(), len, PROT_READ, MAP_SHARED, file.as_raw_fd(), 0) };
            if addr as isize == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(Mapping { addr, len })
        }

        pub fn bytes(&self) -> &[u8] {
            if self.len == 0 {
                return &[];
            }
            unsafe { std::slice::from_raw_parts(self.addr as *const u8, self.len) }
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            if !self.addr.is_null() {
                unsafe {
                    munmap(self.addr, self.len);
                }
            }
        }
    }
}

pub use imp::Mapping;
