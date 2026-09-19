use gpui_react::gpui::{
    AppContext, Context, Empty, EventEmitter, IntoElement, Render, TestAppContext, Window,
};
use gpui_react::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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

fn tx(sequence: u64, operations: Value) -> protocol::Transaction {
    gpui_react::protocol::Transaction::from_json(&json!({"version":1,"sequence":sequence,"operations":operations}))
        .unwrap()
}

#[test]
fn registration_checks_names_duplicates_and_schemas() {
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
    assert!(!registry.capabilities("counter").unwrap().events);
    assert!(registry.capabilities("missing").is_err());
    let mut decoder = Decoder::new(registry);
    let create = |props: Value| {
        json!({"version":1,"sequence":1,"operations":[{"op":"create","id":1,"component":"counter","props":props}]}).to_string()
    };
    assert!(decoder.parse(&create(json!({"step": -1}))).is_err());
    assert!(
        decoder
            .parse(&create(json!({"step": 1, "typo": true})))
            .is_err()
    );
    assert!(decoder.parse(&create(json!({"step": 1}))).is_ok());
}

#[gpui::test]
fn wrapped_view_preserves_native_state_and_delivers_typed_events(cx: &mut TestAppContext) {
    let received = Arc::new(Mutex::new(Vec::new()));
    let host = |received: &Arc<Mutex<Vec<Emission>>>| {
        let sink = received.clone();
        let mut registry = Registry::default();
        registry
            .register(
                Component::<Counter>::new("counter")
                    .events()
                    .commands()
                    .queries(),
            )
            .unwrap();
        Host::new(registry, Arc::new(move |event| sink.lock().unwrap().push(event)))
    };
    let window = cx.add_window(|_, _| host(&received));
    let reply = window
        .update(cx, |host, window, cx| {
            host.apply(tx(1, json!([
                {"op":"create","id":7,"component":"counter","props":{"step":2},"subscription":19},
                {"op":"place","parent":null,"child":7,"before":null},
                {"op":"command","id":7,"component":"counter","request":1,"value":"increment"},
                {"op":"props","id":7,"component":"counter","props":{"step":5}},
                {"op":"command","id":7,"component":"counter","request":2,"value":"increment"},
                {"op":"query","id":7,"component":"counter","request":3,"value":null},
                {"op":"command","id":7,"component":"counter","request":4,"value":"fail"},
                {"op":"command","id":7,"component":"counter","request":5,"value":"unknown"},
                {"op":"place","parent":7,"child":7,"before":null}
            ])), window, cx)
        })
        .unwrap();
    assert!(reply.is_err(), "a counter does not accept children");
    // A failed transaction ends its session; continue on a fresh host.
    window
        .update(cx, |host, window, cx| host.clear(window, cx))
        .unwrap();
    cx.run_until_parked();
    received.lock().unwrap().clear();
    let window = cx.add_window(|_, _| host(&received));
    let reply = window
        .update(cx, |host, window, cx| {
            host.apply(tx(1, json!([
                {"op":"create","id":7,"component":"counter","props":{"step":2},"subscription":19},
                {"op":"place","parent":null,"child":7,"before":null},
                {"op":"command","id":7,"component":"counter","request":1,"value":"increment"},
                {"op":"props","id":7,"component":"counter","props":{"step":5}},
                {"op":"command","id":7,"component":"counter","request":2,"value":"increment"},
                {"op":"query","id":7,"component":"counter","request":3,"value":null},
                {"op":"command","id":7,"component":"counter","request":4,"value":"fail"},
                {"op":"command","id":7,"component":"counter","request":5,"value":"unknown"}
            ])), window, cx)
        })
        .unwrap()
        .unwrap();
    assert_eq!(reply.results[2].value, Some(json!(7)));
    assert!(reply.results[3].error.as_ref().unwrap().contains("rejected"));
    assert!(reply.results[4].error.is_some(), "schema failures are call errors");
    cx.run_until_parked();
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
    window
        .update(cx, |host, window, cx| {
            let reply = host
                .apply(tx(2, json!([{"op":"remove","id":7}])), window, cx)
                .unwrap();
            assert_eq!(reply.retired, vec![19]);
            assert!(host.is_empty());
        })
        .unwrap();
}

#[gpui::test]
fn dropping_the_host_stops_events_even_if_the_view_is_still_retained(cx: &mut TestAppContext) {
    let received = Arc::new(Mutex::new(Vec::new()));
    let output = received.clone();
    let window = cx.add_window(|_, _| Empty);
    let view = cx
        .update_window(window.into(), |_, window, cx| {
            let mut registry = Registry::default();
            registry
                .register(Component::<Counter>::new("counter").events())
                .unwrap();
            let host = cx.new(|_| {
                Host::new(registry, Arc::new(move |event| output.lock().unwrap().push(event)))
            });
            let view = host.update(cx, |host, cx| {
                host.apply(tx(1, json!([
                    {"op":"create","id":1,"component":"counter","props":{"step":1},"subscription":1},
                    {"op":"place","parent":null,"child":1,"before":null}
                ])), window, cx).unwrap();
                host.view(1).unwrap().downcast::<Counter>().unwrap()
            });
            drop(host);
            view
        })
        .unwrap();
    cx.update_window(window.into(), |_, _, cx| {
        view.update(cx, |_, cx| cx.emit(Changed { value: 3 }))
    })
    .unwrap();
    cx.run_until_parked();
    assert!(received.lock().unwrap().is_empty());
}

#[gpui::test]
fn pending_gpui_events_keep_their_subscription_before_replacement_and_unmount(
    cx: &mut TestAppContext,
) {
    let received = Arc::new(Mutex::new(Vec::new()));
    let output = received.clone();
    let window = cx.add_window(|_, _| {
        let mut registry = Registry::default();
        registry
            .register(Component::<Counter>::new("counter").events().commands())
            .unwrap();
        Host::new(registry, Arc::new(move |e| output.lock().unwrap().push(e)))
    });
    window
        .update(cx, |host, window, cx| {
            host.apply(tx(1, json!([
                {"op":"create","id":1,"component":"counter","props":{"step":1},"subscription":10},
                {"op":"place","parent":null,"child":1,"before":null},
                {"op":"command","id":1,"component":"counter","request":1,"value":"increment"},
                {"op":"listen","id":1,"subscription":11},
                {"op":"command","id":1,"component":"counter","request":2,"value":"increment"},
                {"op":"remove","id":1}
            ])), window, cx).unwrap();
        })
        .unwrap();
    cx.run_until_parked();
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
