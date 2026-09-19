//! Manual release benchmark. Measures native mutation work, not layout or presentation.
use gpui::{AppContext, Empty, ListAlignment, ListState, TestAppContext, px};
use gpui_react::{ReactChildren, ReactView};
use gpui_react_controls::{ListProps, VirtualList};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
    time::Instant,
};

#[derive(Clone, Copy, Default)]
struct Allocation {
    active: bool,
    calls: u64,
    allocated: u64,
    freed: u64,
}
thread_local! {static ALLOC:Cell<Allocation>=const {Cell::new(Allocation {active:false,calls:0,allocated:0,freed:0})};}
struct Counting;
#[global_allocator]
static GLOBAL: Counting = Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        let _ = ALLOC.try_with(|slot| {
            let mut value = slot.get();
            if value.active {
                value.calls += 1;
                value.allocated += layout.size() as u64;
                slot.set(value);
            }
        });
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        let _ = ALLOC.try_with(|slot| {
            let mut value = slot.get();
            if value.active {
                value.freed += layout.size() as u64;
                slot.set(value);
            }
        });
        unsafe { System.dealloc(pointer, layout) };
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(pointer, layout, size) };
        let _ = ALLOC.try_with(|slot| {
            let mut value = slot.get();
            if value.active {
                value.calls += 1;
                value.allocated += size as u64;
                value.freed += layout.size() as u64;
                slot.set(value);
            }
        });
        next
    }
}
fn measure(label: &str, count: usize, mut operation: impl FnMut(usize)) {
    for i in 0..20 {
        operation(i);
    }
    let mut samples = Vec::new();
    let mut total = Allocation::default();
    for i in 0..200 {
        ALLOC.with(|slot| {
            slot.set(Allocation {
                active: true,
                ..Default::default()
            })
        });
        let start = Instant::now();
        operation(i);
        let micros = start.elapsed().as_secs_f64() * 1e6;
        let allocation = ALLOC.with(|slot| {
            let value = slot.get();
            slot.set(Allocation::default());
            value
        });
        samples.push(micros);
        total.calls += allocation.calls;
        total.allocated += allocation.allocated;
        total.freed += allocation.freed;
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({"label":label,"logicalRows":count,"samples":samples.len(),"p50Us":samples[100],"p95Us":samples[190],"maxUs":samples[199],"allocationsMean":total.calls/200,"allocatedBytesMean":total.allocated/200,"freedBytesMean":total.freed/200})
    );
}
fn props(count: usize) -> ListProps {
    ListProps {
        item_count: Some(count),
        window_start: 500,
        estimated_item_height: Some(20.),
        ..Default::default()
    }
}

#[gpui::test]
#[ignore = "manual release benchmark; run with --ignored --nocapture"]
fn list_count_update_cost(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, _| Empty);
    window
        .update(cx, |_, window, cx| {
            for count in [1_000, 100_000] {
                let state = ListState::new(count, ListAlignment::Top, px(0.))
                    .with_uniform_item_height(px(20.));
                measure("gpui-splice-unhinted-lower-bound", count, |i| {
                    if i % 2 == 0 {
                        state.splice(count..count, 1);
                    } else {
                        state.splice(count..count + 1, 0);
                    }
                    black_box(state.item_count());
                });
                let state = ListState::new(count, ListAlignment::Top, px(0.))
                    .with_uniform_item_height(px(20.));
                measure("gpui-splice-reseed-all", count, |i| {
                    if i % 2 == 0 {
                        state.splice(count..count, 1);
                    } else {
                        state.splice(count..count + 1, 0);
                    }
                    black_box(state.clone().with_uniform_item_height(px(20.)));
                });
                let state = ListState::new(0, ListAlignment::Top, px(0.));
                state.splice_with_uniform_height(0..0, count, px(20.));
                measure("gpui-splice-hinted", count, |i| {
                    if i % 2 == 0 {
                        state.splice_with_uniform_height(count..count, 1, px(20.));
                    } else {
                        state.splice_with_uniform_height(count..count + 1, 0, px(20.));
                    }
                    black_box(state.item_count());
                });
                let list = cx.new(|cx| VirtualList::new(props(count), window, cx));
                let rows = (0..60).map(|_| cx.new(|_| Empty).into()).collect();
                list.update(cx, |list, cx| list.set_children(rows, window, cx));
                measure("bridge-count-with-60-supplied-rows", count, |i| {
                    list.update(cx, |list, cx| {
                        list.set_props(props(count + usize::from(i % 2 == 0)), window, cx)
                    });
                    black_box(list.read(cx).list_state().item_count());
                });
                assert_eq!(list.read(cx).list_state().item_count(), count);
            }
        })
        .unwrap();
}

#[test]
fn hinted_splice_allocation_does_not_scale_with_the_whole_list() {
    let cost = |count| {
        let state =
            ListState::new(count, ListAlignment::Top, px(0.)).with_uniform_item_height(px(20.));
        ALLOC.with(|slot| {
            slot.set(Allocation {
                active: true,
                ..Default::default()
            })
        });
        state.splice_with_uniform_height(count..count, 1, px(20.));
        let allocated = ALLOC.with(|slot| {
            let value = slot.get();
            slot.set(Allocation::default());
            value.allocated
        });
        assert_eq!(state.item_count(), count + 1);
        allocated
    };
    let small = cost(1_000);
    let large = cost(100_000);
    assert!(
        large < small * 4,
        "100x more retained rows must not cause a full-index allocation: {small} vs {large} bytes"
    );
}
