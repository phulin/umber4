//! Allocation diagnostics; build separately from ordinary CLI/resume timings.
mod layout_support;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
struct Counted;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static REQUESTED: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn allocated(n: usize) {
    CALLS.fetch_add(1, Relaxed);
    REQUESTED.fetch_add(n, Relaxed);
    PEAK.fetch_max(LIVE.fetch_add(n, Relaxed) + n, Relaxed);
}
// SAFETY: layouts and ownership pass unchanged to System; counters inspect no memory.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: delegated with the caller's valid layout.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() && ACTIVE.load(Relaxed) {
            allocated(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        if ACTIVE.load(Relaxed) {
            LIVE.fetch_sub(layout.size(), Relaxed);
        }
        // SAFETY: delegated with the original pointer and layout.
        unsafe { System.dealloc(p, layout) };
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, n: usize) -> *mut u8 {
        // SAFETY: delegated with the original layout and requested size.
        let result = unsafe { System.realloc(p, layout, n) };
        if !result.is_null() && ACTIVE.load(Relaxed) {
            LIVE.fetch_sub(layout.size(), Relaxed);
            allocated(n);
        }
        result
    }
}
#[global_allocator]
static ALLOCATOR: Counted = Counted;
fn main() {
    let mut stages = Vec::with_capacity(16);
    layout_support::run(|label| {
        if label == "begin" {
            ACTIVE.store(true, Relaxed);
        }
        stages.push((
            label,
            CALLS.load(Relaxed),
            REQUESTED.load(Relaxed),
            LIVE.load(Relaxed),
            PEAK.load(Relaxed),
        ));
        if label == "all_engine_owners_released" {
            ACTIVE.store(false, Relaxed);
        }
    });
    println!("{{\"stages\":[");
    for (i, (label, calls, requested, live, peak)) in stages.iter().enumerate() {
        if i != 0 {
            println!(",");
        }
        print!(
            "{{\"stage\":\"{label}\",\"calls\":{calls},\"requested_bytes\":{requested},\"live_bytes\":{live},\"peak_bytes\":{peak}}}"
        );
    }
    println!("]}}");
}
