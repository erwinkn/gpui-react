use gpui::{AppContext, Context, Empty, Render, TestAppContext, Window, div, prelude::*, px};
use gpui_react::{ReactChildren, ReactView};
use gpui_react_controls::{ListProps, VirtualList};
use serde_json::json;

struct Row;
impl Render for Row {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w_full().h(px(40.))
    }
}
fn props(count: Option<usize>) -> ListProps {
    serde_json::from_value(
        json!({"itemCount":count,"estimatedItemHeight":20,"style":{"width":100,"height":100}}),
    )
    .unwrap()
}

#[gpui::test]
fn count_only_update_keeps_completed_measurements(cx: &mut TestAppContext) {
    let window = cx.add_window(|window, cx| VirtualList::new(props(Some(100_000)), window, cx));
    let children = window
        .update(cx, |list, window, cx| {
            let children = (0..5).map(|_| cx.new(|_| Row).into()).collect::<Vec<_>>();
            list.set_children(children.clone(), window, cx);
            children
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    window
        .update(cx, |list, window, cx| {
            let before = list
                .list_state()
                .bounds_for_item(0)
                .expect("first row was measured");
            assert_eq!(before.size.height, px(40.));
            list.set_props(props(Some(100_001)), window, cx);
            assert_eq!(
                list.list_state().bounds_for_item(0),
                Some(before),
                "a count-only update must not mark supplied rows unmeasured"
            );
            list.set_children(children, window, cx);
            assert_eq!(
                list.list_state().bounds_for_item(0),
                Some(before),
                "an unchanged child list must preserve native cache state"
            );
        })
        .unwrap();
}

#[gpui::test]
fn appended_child_keeps_measurements_of_existing_rows(cx: &mut TestAppContext) {
    let window = cx.add_window(|window, cx| VirtualList::new(props(None), window, cx));
    let mut children = window
        .update(cx, |list, window, cx| {
            let children = (0..5).map(|_| cx.new(|_| Row).into()).collect::<Vec<_>>();
            list.set_children(children.clone(), window, cx);
            children
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    window
        .update(cx, |list, window, cx| {
            let before = list.list_state().bounds_for_item(0).unwrap();
            children.push(cx.new(|_| Empty).into());
            list.set_children(children, window, cx);
            assert_eq!(list.list_state().bounds_for_item(0), Some(before));
            assert_eq!(list.list_state().item_count(), 6);
        })
        .unwrap();
}
