//! Native commits and painted hitboxes have separate lifetimes.
use gpui_react::{
    gpui::{
        self, App, AppContext, Context, EventEmitter, InputEvent, Modifiers, MouseButton,
        MouseDownEvent, MouseMoveEvent, MouseUpEvent, TestAppContext, Window, div, point,
        prelude::*, px,
    },
    *,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Deserialize, gpui_react::ComponentProps)]
struct Props {
    revision: u64,
    width: f32,
}
#[derive(Serialize)]
struct Press {
    current_revision: u64,
    painted_revision: u64,
}
struct Button {
    props: Props,
}
impl EventEmitter<Press> for Button {}
impl Render for Button {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let painted_revision = self.props.revision;
        div()
            .id("button")
            .w(px(self.props.width))
            .h(px(40.))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    cx.emit(Press {
                        current_revision: this.props.revision,
                        painted_revision,
                    });
                }),
            )
    }
}
impl ReactView for Button {
    type Props = Props;
    fn create(props: Props, _: &mut Window, _: &mut Context<Self>) -> Self {
        Self { props }
    }
    fn set_props(&mut self, props: Props, _: &mut Window, cx: &mut Context<Self>) {
        self.props = props;
        cx.notify();
    }
}
impl ReactEvents for Button {
    type Event = Press;
}
fn apply(
    host: &mut Host,
    sequence: u64,
    operations: serde_json::Value,
    window: &mut Window,
    cx: &mut Context<Host>,
) -> protocol::Reply {
    host.apply(
        gpui_react::protocol::Transaction::from_json(&json!({"version":1,"sequence":sequence,"operations":operations}))
            .unwrap(),
        window,
        cx,
    )
    .unwrap()
}
fn press(window: &mut Window, cx: &mut App, x: f32) {
    let position = point(px(x), px(10.));
    window.dispatch_event(
        MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers: Modifiers::default(),
        }
        .to_platform_input(),
        cx,
    );
    window.dispatch_event(
        MouseDownEvent {
            position,
            button: MouseButton::Left,
            click_count: 1,
            first_mouse: false,
            modifiers: Modifiers::default(),
        }
        .to_platform_input(),
        cx,
    );
    window.dispatch_event(
        MouseUpEvent {
            position,
            button: MouseButton::Left,
            click_count: 1,
            modifiers: Modifiers::default(),
        }
        .to_platform_input(),
        cx,
    );
}
#[gpui::test]
fn native_commit_routes_events_without_claiming_that_old_hitboxes_are_new_layout(
    cx: &mut TestAppContext,
) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let mut registry = Registry::default();
    registry
        .register(Component::<Button>::new("button").events())
        .unwrap();
    let handle = cx.add_window(|_, _| {
        Host::new(
            registry,
            Arc::new(move |event| sink.lock().unwrap().push(event)),
        )
    });
    handle.update(cx,|host,window,cx|{
        apply(host,1,json!([
            {"op":"create","id":1,"component":"button","props":{"revision":1,"width":80},"subscription":1},
            {"op":"place","child":1,"parent":null,"before":null}
        ]),window,cx);
    }).unwrap();
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    events.lock().unwrap().clear();
    // Keep the native update and input dispatch in one App update. TestContext
    // must not insert an automatic draw between them. Host::apply has returned
    // before the second press; no input runs in the middle of a transaction.
    handle
        .update(cx, |host, window, cx| {
            press(window, cx, 60.);
            let reply = apply(
                host,
                2,
                json!([
                    {"op":"props","id":1,"component":"button","props":{"revision":2,"width":20}},
                    {"op":"listen","id":1,"subscription":2}
                ]),
                window,
                cx,
            );
            assert_eq!(reply.retired, vec![1]);
            press(window, cx, 60.);
        })
        .unwrap();
    let received = std::mem::take(&mut *events.lock().unwrap());
    assert_eq!(received.len(), 2);
    assert_eq!(received[0].subscription, 1);
    assert_eq!(
        received[0].payload.as_ref().unwrap(),
        &json!({"current_revision":1,"painted_revision":1})
    );
    assert_eq!(received[1].subscription, 2);
    assert_eq!(
        received[1].payload.as_ref().unwrap(),
        &json!({"current_revision":2,"painted_revision":1})
    );
    cx.update_window(handle.into(), |_, window, cx| {
        window.draw(cx).clear(cx);
        press(window, cx, 60.);
        press(window, cx, 10.);
    })
    .unwrap();
    let received = std::mem::take(&mut *events.lock().unwrap());
    assert_eq!(
        received.len(),
        1,
        "the narrower painted hitbox rejects the old location"
    );
    assert_eq!(received[0].subscription, 2);
    assert_eq!(
        received[0].payload.as_ref().unwrap(),
        &json!({"current_revision":2,"painted_revision":2})
    );
    handle.update(cx,|host,window,cx|{
        let reply=apply(host,3,json!([
            {"op":"remove","id":1},
            {"op":"create","id":2,"component":"button","props":{"revision":3,"width":80},"subscription":3},
            {"op":"place","child":2,"parent":null,"before":null}
        ]),window,cx);
        assert_eq!(reply.retired,vec![2]);
        press(window,cx,10.);
    }).unwrap();
    assert!(
        events.lock().unwrap().is_empty(),
        "old frame input must not be routed to a replacement view"
    );
    cx.update_window(handle.into(), |_, window, cx| {
        window.draw(cx).clear(cx);
        press(window, cx, 10.);
    })
    .unwrap();
    let received = std::mem::take(&mut *events.lock().unwrap());
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].target, 2);
    assert_eq!(received[0].subscription, 3);
    assert_eq!(
        received[0].payload.as_ref().unwrap(),
        &json!({"current_revision":3,"painted_revision":3})
    );
    handle
        .update(cx, |host, window, cx| host.clear(window, cx))
        .unwrap();
}
