use gpui::{AppContext, TestAppContext, WindowHandle, px};
use gpui_react::{Host, Registry};
use gpui_react_kit::VirtualList;
use serde_json::{Value, json};
use std::sync::Arc;

fn props(count: Option<usize>) -> Value {
    json!({"itemCount":count,"estimatedItemHeight":20,"style":{"width":100,"height":100}})
}
fn row(id: u64) -> [Value; 2] {
    [
        json!({"op":"create","id":id,"component":"container","props":{"style":{"width":"100%","height":40}}}),
        json!({"op":"place","parent":1,"child":id,"before":null}),
    ]
}
fn host(cx: &mut TestAppContext) -> WindowHandle<Host> {
    cx.add_window(|_, _| {
        let mut registry = Registry::default();
        gpui_react_kit::register_kit(&mut registry).unwrap();
        Host::new(registry, Arc::new(|_| {}))
    })
}
fn apply(cx: &mut TestAppContext, window: WindowHandle<Host>, sequence: u64, operations: Value) {
    window
        .update(cx, |host, window, cx| {
            host.apply(
                gpui_react::protocol::Transaction::from_json(&
                    json!({"version":1,"sequence":sequence,"operations":operations}),
                )
                .unwrap(),
                window,
                cx,
            )
            .unwrap();
        })
        .unwrap();
}
fn list(cx: &mut TestAppContext, window: WindowHandle<Host>) -> gpui::Entity<VirtualList> {
    window
        .update(cx, |host, _, _| host.view(1).unwrap().downcast().unwrap())
        .unwrap()
}
fn draw(cx: &mut TestAppContext, window: WindowHandle<Host>) {
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
}

#[gpui::test]
fn count_only_update_keeps_completed_measurements(cx: &mut TestAppContext) {
    let window = host(cx);
    let mut operations = vec![
        json!({"op":"create","id":1,"component":"list","props":props(Some(100_000))}),
        json!({"op":"place","parent":null,"child":1,"before":null}),
    ];
    for id in 2..7 {
        operations.extend(row(id));
    }
    apply(cx, window, 1, Value::Array(operations));
    draw(cx, window);
    let list = list(cx, window);
    let before = list.read_with(cx, |list, _| {
        list.list_state()
            .bounds_for_item(0)
            .expect("first row was measured")
    });
    assert_eq!(before.size.height, px(40.));
    apply(
        cx,
        window,
        2,
        json!([{"op":"props","id":1,"component":"list","props":props(Some(100_001))}]),
    );
    assert_eq!(
        list.read_with(cx, |list, _| list.list_state().bounds_for_item(0)),
        Some(before),
        "a count-only update must not mark supplied rows unmeasured"
    );
    // An unrelated transaction re-delivers no child list.
    apply(cx, window, 3, json!([{"op":"props","id":6,"component":"container","props":{"style":{"width":"100%","height":40}}}]));
    assert_eq!(
        list.read_with(cx, |list, _| list.list_state().bounds_for_item(0)),
        Some(before),
        "a change inside another row must preserve unrelated native cache state"
    );
}

#[gpui::test]
fn appended_child_keeps_measurements_of_existing_rows(cx: &mut TestAppContext) {
    let window = host(cx);
    let mut operations = vec![
        json!({"op":"create","id":1,"component":"list","props":props(None)}),
        json!({"op":"place","parent":null,"child":1,"before":null}),
    ];
    for id in 2..7 {
        operations.extend(row(id));
    }
    apply(cx, window, 1, Value::Array(operations));
    draw(cx, window);
    let list = list(cx, window);
    let before = list.read_with(cx, |list, _| list.list_state().bounds_for_item(0).unwrap());
    apply(cx, window, 2, Value::Array(row(7).to_vec()));
    assert_eq!(
        list.read_with(cx, |list, _| list.list_state().bounds_for_item(0)),
        Some(before)
    );
    assert_eq!(list.read_with(cx, |list, _| list.list_state().item_count()), 6);
}
