use gpui_react::gpui::{
    AppContext, Context, Empty, EventEmitter, IntoElement, Render, TestAppContext, Window,
};
use gpui_react::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Props {
    step: u32,
}

struct Counter {
    count: u32,
    step: u32,
}

#[derive(Serialize)]
struct Changed {
    value: u32,
}
impl EventEmitter<Changed> for Counter {}

impl Render for Counter {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // No GPUiX render trait or renderer context is involved.
        Empty
    }
}

impl ReactView for Counter {
    type Props = Props;
    fn create(props: Props, _: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            count: 0,
            step: props.step,
        }
    }
    fn set_props(&mut self, props: Props, _: &mut Window, cx: &mut Context<Self>) {
        self.step = props.step;
        cx.notify();
    }
}
impl ReactEvents for Counter {
    type Event = Changed;
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Command {
    Increment,
    Fail,
}
impl ReactCommands for Counter {
    type Command = Command;
    fn command(
        &mut self,
        command: Command,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        match command {
            Command::Increment => {
                self.count += self.step;
                cx.notify();
                cx.emit(Changed { value: self.count });
                Ok(())
            }
            Command::Fail => anyhow::bail!("counter rejected command"),
        }
    }
}
impl ReactQueries for Counter {
    type Query = ();
    type Reply = u32;
    fn query(&mut self, _: (), _: &mut Window, _: &mut Context<Self>) -> anyhow::Result<u32> {
        Ok(self.count)
    }
}

#[test]
fn registration_checks_names_duplicates_and_capabilities() {
    let mut registry = Registry::default();
    registry
        .register(Component::<Counter>::new("counter"))
        .unwrap();
    assert!(
        registry
            .register(Component::<Counter>::new("counter"))
            .is_err()
    );
    assert!(
        registry
            .register(Component::<Counter>::new("Bad Name"))
            .is_err()
    );
    assert!(!registry.supports("counter", "events").unwrap());
    assert!(registry.supports("missing", "events").is_err());
    assert!(
        registry
            .prepare_props("counter", json!({"step": -1}))
            .is_err()
    );
    assert!(
        registry
            .prepare_props("counter", json!({"step": 1, "typo": true}))
            .is_err()
    );
}

#[gpui::test]
fn wrapped_view_preserves_native_state_and_delivers_typed_events(cx: &mut TestAppContext) {
    let mut registry = Registry::default();
    registry
        .register(
            Component::<Counter>::new("counter")
                .events()
                .commands()
                .queries(),
        )
        .unwrap();
    let received = Arc::new(Mutex::new(Vec::new()));
    let received_sink = received.clone();
    let sink: EventSink = Arc::new(move |event| received_sink.lock().unwrap().push(event));
    let window = cx.add_window(|_, _| Empty);
    let mut mounted = cx
        .update_window(window.into(), |_, window, cx| {
            registry
                .mount(
                    "counter",
                    registry
                        .prepare_props("counter", json!({"step": 2}))
                        .unwrap(),
                    MountOptions {
                        target: 7,
                        subscription: Some(19),
                        events: sink,
                    },
                    window,
                    cx,
                )
                .unwrap()
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        mounted
            .apply(
                "command",
                mounted.prepare("command", json!("increment")).unwrap(),
                window,
                cx,
            )
            .unwrap();
        mounted
            .apply(
                "props",
                mounted.prepare("props", json!({"step": 5})).unwrap(),
                window,
                cx,
            )
            .unwrap();
        mounted
            .apply(
                "command",
                mounted.prepare("command", json!("increment")).unwrap(),
                window,
                cx,
            )
            .unwrap();
        assert_eq!(
            mounted
                .apply(
                    "query",
                    mounted.prepare("query", json!(null)).unwrap(),
                    window,
                    cx
                )
                .unwrap(),
            json!(7)
        );
        assert!(
            mounted
                .apply(
                    "command",
                    mounted.prepare("command", json!("fail")).unwrap(),
                    window,
                    cx
                )
                .is_err()
        );
        assert!(mounted.prepare("command", json!("unknown")).is_err());
        assert!(mounted.set_children(vec![], window, cx).is_err());
    })
    .unwrap();
    let events = received.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].payload.as_ref().unwrap(), &json!({"value":2}));
    assert_eq!(events[1].payload.as_ref().unwrap(), &json!({"value":7}));
    assert!(
        events
            .iter()
            .all(|event| event.target == 7 && event.subscription == 19)
    );
    drop(events);
    cx.update_window(window.into(), |_, window, cx| {
        mounted.unmount(window, cx);
        mounted.unmount(window, cx);
        assert!(mounted.prepare("query", json!(null)).is_err());
        assert!(mounted.set_subscription(Some(20), cx).is_err());
    })
    .unwrap();
}

#[gpui::test]
fn dropping_a_binding_stops_events_even_if_the_view_is_still_retained(cx: &mut TestAppContext) {
    let mut registry = Registry::default();
    registry
        .register(Component::<Counter>::new("counter").events())
        .unwrap();
    let received = Arc::new(Mutex::new(Vec::new()));
    let output = received.clone();
    let window = cx.add_window(|_, _| Empty);
    let view = cx
        .update_window(window.into(), |_, window, cx| {
            let mounted = registry
                .mount(
                    "counter",
                    registry
                        .prepare_props("counter", json!({"step": 1}))
                        .unwrap(),
                    MountOptions {
                        target: 1,
                        subscription: Some(1),
                        events: Arc::new(move |event| output.lock().unwrap().push(event)),
                    },
                    window,
                    cx,
                )
                .unwrap();
            let view = mounted.view().clone().downcast::<Counter>().unwrap();
            drop(mounted);
            view
        })
        .unwrap();
    cx.update_window(window.into(), |_, _, cx| {
        view.update(cx, |_, cx| cx.emit(Changed { value: 3 }))
    })
    .unwrap();
    assert!(received.lock().unwrap().is_empty());
}

#[gpui::test]
fn pending_gpui_events_keep_their_subscription_before_replacement_and_unmount(
    cx: &mut TestAppContext,
) {
    let mut registry = Registry::default();
    registry
        .register(Component::<Counter>::new("counter").events().commands())
        .unwrap();
    let received = Arc::new(Mutex::new(Vec::new()));
    let output = received.clone();
    let window = cx.add_window(|_, _| Empty);
    let mut mounted = cx
        .update_window(window.into(), |_, window, cx| {
            registry
                .mount(
                    "counter",
                    registry
                        .prepare_props("counter", json!({"step":1}))
                        .unwrap(),
                    MountOptions {
                        target: 1,
                        subscription: Some(10),
                        events: Arc::new(move |e| output.lock().unwrap().push(e)),
                    },
                    window,
                    cx,
                )
                .unwrap()
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        mounted
            .apply(
                "command",
                mounted.prepare("command", json!("increment")).unwrap(),
                window,
                cx,
            )
            .unwrap();
        mounted.set_subscription(Some(11), cx).unwrap();
        mounted
            .apply(
                "command",
                mounted.prepare("command", json!("increment")).unwrap(),
                window,
                cx,
            )
            .unwrap();
        mounted.unmount(window, cx);
        drop(mounted);
    })
    .unwrap();
    assert_eq!(
        received
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.subscription)
            .collect::<Vec<_>>(),
        vec![10, 11]
    );
}
