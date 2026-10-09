//! Peak memory of the process from the operating system, for the measurement
//! lines (no crates: a direct call into kernel32 on Windows, /proc/self/status
//! on Linux). The author watches the committed size in the task manager; the
//! working set can be a few GB lower, so both are reported.

/// (peak committed bytes, peak working set bytes), None when unavailable.
#[cfg(windows)]
pub fn peak_memory() -> Option<(u64, u64)> {
    use std::ffi::c_void;

    /// PROCESS_MEMORY_COUNTERS from psapi.h, field order matters.
    #[repr(C)]
    #[derive(Default)]
    #[allow(dead_code)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn K32GetProcessMemoryInfo(process: *mut c_void, counters: *mut ProcessMemoryCounters, cb: u32) -> i32;
    }

    let mut c = ProcessMemoryCounters { cb: std::mem::size_of::<ProcessMemoryCounters>() as u32, ..Default::default() };
    // SAFETY: the struct mirrors PROCESS_MEMORY_COUNTERS and cb is its size;
    // the pseudo handle of the current process needs no closing.
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    if ok == 0 {
        return None;
    }
    Some((c.peak_pagefile_usage as u64, c.peak_working_set_size as u64))
}

/// (physical memory of the machine, the part of it that is free right now).
#[cfg(windows)]
pub fn available_memory() -> Option<(u64, u64)> {
    /// MEMORYSTATUSEX from sysinfoapi.h.
    #[repr(C)]
    #[derive(Default)]
    #[allow(dead_code)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(status: *mut MemoryStatusEx) -> i32;
    }

    let mut m = MemoryStatusEx { length: std::mem::size_of::<MemoryStatusEx>() as u32, ..Default::default() };
    // SAFETY: the struct mirrors MEMORYSTATUSEX and length is its size.
    let ok = unsafe { GlobalMemoryStatusEx(&mut m) };
    if ok == 0 {
        return None;
    }
    Some((m.total_phys, m.avail_phys))
}

/// (peak virtual size, peak resident set) from /proc/self/status; Linux has
/// no committed-size counter, so VmPeak stands in for it.
#[cfg(not(windows))]
pub fn peak_memory() -> Option<(u64, u64)> {
    Some((proc_value("/proc/self/status", "VmPeak:")?, proc_value("/proc/self/status", "VmHWM:")?))
}

/// (physical memory of the machine, the part of it that is free right now)
/// from /proc/meminfo.
#[cfg(not(windows))]
pub fn available_memory() -> Option<(u64, u64)> {
    Some((proc_value("/proc/meminfo", "MemTotal:")?, proc_value("/proc/meminfo", "MemAvailable:")?))
}

/// Reads a "key: value kB" line of a proc file as bytes.
#[cfg(not(windows))]
fn proc_value(path: &str, key: &str) -> Option<u64> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse::<u64>().ok()).map(|v| v * 1024)
}

/// "35.1 GiB" or "346 MB".
pub fn format_bytes(b: u64) -> String {
    if b >= 1 << 30 {
        format!("{:.1} GiB", b as f64 / (1u64 << 30) as f64)
    } else {
        format!("{} MB", b >> 20)
    }
}
