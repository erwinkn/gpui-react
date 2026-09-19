use gpui_react::{
    gpui::{self, Context, Empty, IntoElement, Render, TestAppContext, Window},
    *,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::Arc,
};

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
struct Counting;
#[global_allocator]
static ALLOCATOR: Counting = Counting;
fn allocated() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let result = unsafe { System.alloc(layout) };
        if !result.is_null() {
            allocated();
        }
        result
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(pointer, layout, size) };
        if !result.is_null() {
            allocated();
        }
        result
    }
}
struct View;
impl Render for View {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}
impl ReactView for View {
    type Props = ();
    fn create(_: (), _: &mut Window, _: &mut Context<Self>) -> Self {
        Self
    }
    fn set_props(&mut self, _: (), _: &mut Window, _: &mut Context<Self>) {}
}
#[gpui::test]
fn clearing_an_unsupported_subscription_does_no_heap_work(cx: &mut TestAppContext) {
    let mut registry = Registry::default();
    registry.register(Component::<View>::new("view")).unwrap();
    let window = cx.add_window(|_, _| Empty);
    window
        .update(cx, |_, window, cx| {
            let mut mounted = registry
                .mount(
                    "view",
                    registry
                        .prepare_props("view", serde_json::Value::Null)
                        .unwrap(),
                    MountOptions {
                        target: 1,
                        subscription: None,
                        events: Arc::new(|_| panic!("view cannot emit events")),
                    },
                    window,
                    cx,
                )
                .unwrap();
            ALLOCATIONS.with(|count| count.set(Some(0)));
            let result = mounted.set_subscription(None, cx);
            let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
            result.unwrap();
            assert_eq!(
                allocations, 0,
                "a component without events needs no queued routing work"
            );
            assert!(mounted.set_subscription(Some(1), cx).is_err());
            mounted.unmount(window, cx);
        })
        .unwrap();
}
