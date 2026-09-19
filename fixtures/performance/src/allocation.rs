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
/// Live allocation counts by exact size up to 4 KiB, then by power of two.
#[cfg(feature = "allocation-counts")]
const BUCKETS: usize = 4096 + 64;
#[cfg(feature = "allocation-counts")]
static LIVE_BY_SIZE: [std::sync::atomic::AtomicI64; BUCKETS] =
    [const { std::sync::atomic::AtomicI64::new(0) }; BUCKETS];
#[cfg(feature = "allocation-counts")]
fn bucket(size: usize) -> usize {
    if size <= 4096 {
        size
    } else {
        4096 + (usize::BITS - size.leading_zeros()) as usize
    }
}
#[cfg(feature = "allocation-counts")]
fn allocated(size: usize) {
    ALLOCATED.fetch_add(size as u64, Relaxed);
    CALLS.fetch_add(1, Relaxed);
    let live = LIVE.fetch_add(size as u64, Relaxed) + size as u64;
    PEAK.fetch_max(live, Relaxed);
    LIVE_BY_SIZE[bucket(size)].fetch_add(1, Relaxed);
}
#[cfg(feature = "allocation-counts")]
fn freed(size: usize) {
    FREED.fetch_add(size as u64, Relaxed);
    LIVE.fetch_sub(size as u64, Relaxed);
    LIVE_BY_SIZE[bucket(size)].fetch_sub(1, Relaxed);
}
/// Backtrace tracking for one allocation size, enabled on demand.
#[cfg(feature = "allocation-counts")]
static TRACK_SIZE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(feature = "allocation-counts")]
static TRACKED: std::sync::Mutex<Option<std::collections::HashMap<usize, (usize, std::backtrace::Backtrace)>>> =
    std::sync::Mutex::new(None);
/// A caller-set phase label stored with each tracked allocation.
pub static PHASE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(feature = "allocation-counts")]
thread_local! { static IN_TRACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(feature = "allocation-counts")]
fn track(ptr: *mut u8, size: usize, add: bool) {
    if size != TRACK_SIZE.load(Relaxed) || size == 0 {
        return;
    }
    IN_TRACK.with(|flag| {
        if flag.get() {
            return;
        }
        flag.set(true);
        if let Ok(mut tracked) = TRACKED.lock() {
            let map = tracked.get_or_insert_with(Default::default);
            if add {
                map.insert(ptr as usize, (PHASE.load(Relaxed), std::backtrace::Backtrace::force_capture()));
            } else if let Some((phase, _)) = map.remove(&(ptr as usize))
                && phase != PHASE.load(Relaxed)
                && let Ok(mut late) = LATE_FREES.lock()
                && late.len() < 2
            {
                late.push((phase, PHASE.load(Relaxed), std::backtrace::Backtrace::force_capture()));
            }
        }
        flag.set(false);
    });
}
#[cfg(feature = "allocation-counts")]
static LATE_FREES: std::sync::Mutex<Vec<(usize, usize, std::backtrace::Backtrace)>> =
    std::sync::Mutex::new(Vec::new());
/// Backtraces of frees of tracked blocks that outlived their allocation phase.
#[allow(clippy::needless_return)]
pub fn late_frees() -> Vec<String> {
    #[cfg(feature = "allocation-counts")]
    {
        IN_TRACK.with(|flag| flag.set(true));
        let result = LATE_FREES
            .lock()
            .map(|late| {
                late.iter()
                    .map(|(from, to, trace)| {
                        let text = format!("{trace}");
                        let frames: Vec<&str> = text
                            .lines()
                            .filter(|line| line.contains("gpui") || line.contains("bridge") || line.contains("Root") || line.contains("core::ptr::drop") || line.contains("alloc::"))
                            .filter(|line| !line.contains("allocation::") && !line.contains(" at "))
                            .take(40)
                            .collect();
                        format!("allocated in phase {from}, freed in phase {to}\n{}", frames.join("\n"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        IN_TRACK.with(|flag| flag.set(false));
        return result;
    }
    #[cfg(not(feature = "allocation-counts"))]
    Vec::new()
}
/// Start recording backtraces for allocations of exactly this size.
pub fn track_size(size: usize) {
    #[cfg(feature = "allocation-counts")]
    TRACK_SIZE.store(size, Relaxed);
    let _ = size;
}
/// Distinct backtraces of tracked allocations still live, with counts.
#[allow(clippy::needless_return)]
pub fn tracked_live() -> Vec<(usize, String)> {
    #[cfg(feature = "allocation-counts")]
    {
        IN_TRACK.with(|flag| flag.set(true));
        let mut counts: std::collections::HashMap<String, usize> = Default::default();
        if let Ok(tracked) = TRACKED.lock()
            && let Some(map) = tracked.as_ref()
        {
            for (phase, trace) in map.values() {
                let text = format!("{trace}");
                let frames: Vec<&str> = text
                    .lines()
                    .filter(|line| line.contains("gpui") || line.contains("bridge") || line.contains("Root") || line.contains("Engine"))
                    .filter(|line| !line.contains("allocation::") && !line.contains(" at ") && !line.contains("taffy::"))
                    .skip(4)
                    .take(24)
                    .collect();
                *counts.entry(format!("phase {phase}\n{}", frames.join("\n"))).or_default() += 1;
            }
        }
        IN_TRACK.with(|flag| flag.set(false));
        let mut result: Vec<_> = counts.into_iter().map(|(k, v)| (v, k)).collect();
        result.sort_by_key(|(count, _)| std::cmp::Reverse(*count));
        return result;
    }
    #[cfg(not(feature = "allocation-counts"))]
    Vec::new()
}
/// Snapshot of live allocation counts per size bucket.
#[allow(clippy::needless_return)]
pub fn histogram() -> Vec<i64> {
    #[cfg(feature = "allocation-counts")]
    return LIVE_BY_SIZE.iter().map(|b| b.load(Relaxed)).collect();
    #[cfg(not(feature = "allocation-counts"))]
    Vec::new()
}
/// The size buckets whose live count grew most between two snapshots,
/// weighted by bytes, as `[size, countDelta]` pairs.
pub fn growth(before: &[i64], after: &[i64]) -> Vec<(String, i64)> {
    let mut deltas: Vec<(usize, i64)> = before
        .iter()
        .zip(after)
        .enumerate()
        .map(|(i, (b, a))| (i, a - b))
        .filter(|(_, d)| *d != 0)
        .collect();
    let size_of = |i: usize| if i <= 4096 { i as i64 } else { 1i64 << (i - 4096 - 1) };
    deltas.sort_by_key(|(i, d)| -(d.abs() * size_of(*i)));
    deltas
        .into_iter()
        .take(12)
        .map(|(i, d)| {
            let label = if i <= 4096 { format!("{i}") } else { format!(">={}", size_of(i)) };
            (label, d)
        })
        .collect()
}
#[cfg(feature = "allocation-counts")]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let result = unsafe { System.alloc(layout) };
        if !result.is_null() {
            allocated(layout.size());
            track(result, layout.size(), true);
        }
        result
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        freed(layout.size());
        track(ptr, layout.size(), false);
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
