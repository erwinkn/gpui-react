//! Counts requested Rust heap bytes, not Objective-C, driver, GPU, or RSS memory.
use serde::Serialize;
#[cfg(feature = "allocation-counts")]
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicU64, Ordering::Relaxed},
};

#[derive(Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mark {
    pub allocated: u64,
    pub freed: u64,
    pub calls: u64,
    pub live: i64,
    pub peak: u64,
}
#[cfg(feature = "allocation-counts")]
static ALLOCATED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "allocation-counts")]
static FREED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "allocation-counts")]
static CALLS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "allocation-counts")]
static LIVE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "allocation-counts")]
static PEAK: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "allocation-counts")]
struct Counting;
#[cfg(feature = "allocation-counts")]
#[global_allocator]
static GLOBAL: Counting = Counting;
#[cfg(feature = "allocation-counts")]
fn allocated(size: usize) {
    ALLOCATED.fetch_add(size as u64, Relaxed);
    CALLS.fetch_add(1, Relaxed);
    let live = LIVE.fetch_add(size as u64, Relaxed) + size as u64;
    PEAK.fetch_max(live, Relaxed);
}
#[cfg(feature = "allocation-counts")]
fn freed(size: usize) {
    FREED.fetch_add(size as u64, Relaxed);
    LIVE.fetch_sub(size as u64, Relaxed);
}
#[cfg(feature = "allocation-counts")]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let result = unsafe { System.alloc(layout) };
        if !result.is_null() {
            allocated(layout.size());
        }
        result
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        freed(layout.size());
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(ptr, layout, size) };
        if !result.is_null() {
            freed(layout.size());
            allocated(size);
        }
        result
    }
}
pub fn mark() -> Mark {
    #[cfg(feature = "allocation-counts")]
    return Mark {
        allocated: ALLOCATED.load(Relaxed),
        freed: FREED.load(Relaxed),
        calls: CALLS.load(Relaxed),
        live: LIVE.load(Relaxed) as i64,
        peak: PEAK.load(Relaxed),
    };
    #[cfg(not(feature = "allocation-counts"))]
    Mark::default()
}
pub fn reset_peak() {
    #[cfg(feature = "allocation-counts")]
    PEAK.store(LIVE.load(Relaxed), Relaxed);
}
impl Mark {
    pub fn since(self, before: Self) -> Self {
        Self {
            allocated: self.allocated - before.allocated,
            freed: self.freed - before.freed,
            calls: self.calls - before.calls,
            live: self.live - before.live,
            peak: self.peak.saturating_sub(before.live as u64),
        }
    }
}
