//! Separate diagnostic binary: allocator counters never enter the normal CLI.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

struct Counted;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static REQUESTED: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn allocated(bytes: usize) {
    CALLS.fetch_add(1, Relaxed);
    REQUESTED.fetch_add(bytes, Relaxed);
    PEAK.fetch_max(LIVE.fetch_add(bytes, Relaxed) + bytes, Relaxed);
}

// SAFETY: all storage operations and layouts are delegated unchanged to System.
// Counters hold integers only and never inspect, retain, or move the allocation.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies GlobalAlloc's valid layout.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() && ACTIVE.load(Relaxed) {
            allocated(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if ACTIVE.load(Relaxed) {
            LIVE.fetch_sub(layout.size(), Relaxed);
        }
        // SAFETY: ownership and layout are passed straight through.
        unsafe { System.dealloc(pointer, layout) };
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: the caller supplies the original allocation and valid new size.
        let result = unsafe { System.realloc(pointer, layout, size) };
        if !result.is_null() && ACTIVE.load(Relaxed) {
            LIVE.fetch_sub(layout.size(), Relaxed);
            allocated(size);
        }
        result
    }
}

#[global_allocator]
static ALLOCATOR: Counted = Counted;

fn main() {
    let path = std::env::args_os()
        .nth(1)
        .expect("usage: allocations FILE.tex");
    let bytes = std::fs::read(path).expect("read source");
    // The immutable host input precedes this interval and is released after it.
    // The single-threaded engine owns all allocations made during the interval.
    ACTIVE.store(true, Relaxed);
    let result = umber::Engine::new(&bytes).run();
    ACTIVE.store(false, Relaxed);
    let outcome = result.expect("execute source");
    for diagnostic in outcome.diagnostics {
        if diagnostic.destination() == umber::DiagnosticDestination::TerminalAndLog {
            println!("{}", String::from_utf8_lossy(diagnostic.bytes()));
        }
    }
    println!(
        "allocations={} requested_bytes={} peak_bytes={}",
        CALLS.load(Relaxed),
        REQUESTED.load(Relaxed),
        PEAK.load(Relaxed)
    );
}
