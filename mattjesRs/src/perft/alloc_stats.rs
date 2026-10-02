//! A counting global allocator. Rust's standard library has no equivalent of
//! Go's `runtime.MemStats`, but the global allocator can be replaced by any
//! type implementing `GlobalAlloc`. This one forwards to the system allocator
//! and sums up every allocation, which gives the same "total allocated" number
//! the Go perft runner prints.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

pub struct CountingAlloc;

static TOTAL: AtomicU64 = AtomicU64::new(0);

// `unsafe impl` because the trait has safety requirements the compiler cannot
// check: we promise to hand out valid memory. Forwarding to `System` keeps that promise.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        TOTAL.fetch_add(layout.size() as u64, Relaxed);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        TOTAL.fetch_add(new_size as u64, Relaxed);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// Total bytes allocated since program start.
pub fn total_allocated() -> u64 {
    TOTAL.load(Relaxed)
}
