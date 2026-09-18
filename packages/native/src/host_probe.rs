//! Bounded test evidence. Timestamps and frame samples are collected on the UI thread.
use serde_json::{Value, json};
use std::cell::RefCell;
use std::time::Instant;

struct Probe {
    start: Instant,
    records: Vec<Value>,
}
thread_local! { static PROBE: RefCell<Option<Probe>> = const { RefCell::new(None) }; }
thread_local! { static TASK: RefCell<Option<gpui::Task<()>>> = const { RefCell::new(None) }; }

pub fn keep(task: gpui::Task<()>) {
    TASK.with(|slot| *slot.borrow_mut() = Some(task));
}
pub fn stop() {
    TASK.with(|slot| slot.borrow_mut().take());
}

pub fn start() {
    PROBE.with(|p| {
        *p.borrow_mut() = Some(Probe {
            start: Instant::now(),
            records: Vec::new(),
        })
    });
}

pub fn record(kind: &str, data: Value) {
    PROBE.with(|p| {
        if let Some(p) = p.borrow_mut().as_mut() {
            if p.records.len() < 4096 {
                p.records.push(json!({"kind": kind, "atMs": p.start.elapsed().as_secs_f64() * 1000.0, "data": data}));
            }
        }
    });
}

pub fn take() -> String {
    stop();
    PROBE.with(|p| json!(p.borrow_mut().take().map(|p| p.records)).to_string())
}
