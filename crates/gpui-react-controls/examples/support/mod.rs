use gpui::{prelude::*, *};
use gpui_react::{Emission, Host};
use serde_json::{Value, json};
use std::{
    rc::Rc,
    sync::{Arc, Mutex},
};

pub struct Harness {
    pub cx: VisualTestAppContext,
    pub window: WindowHandle<Host>,
    pub events: Arc<Mutex<Vec<Emission>>>,
    sequence: u64,
    request: u64,
}
impl Harness {
    pub fn new(width: f32, height: f32) -> Self {
        let mut cx = VisualTestAppContext::new(Rc::new(gpui_macos::MacPlatform::new(false)));
        let mut registry = gpui_react::Registry::default();
        gpui_react_controls::register(&mut registry).unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let window = cx
            .open_offscreen_window(size(px(width), px(height)), |_, cx| {
                cx.new(|_| Host::new(registry, Arc::new(move |e| sink.lock().unwrap().push(e))))
            })
            .unwrap();
        Self {
            cx,
            window,
            events,
            sequence: 0,
            request: 0,
        }
    }
    pub fn apply(&mut self, operations: Value) -> gpui_react::protocol::Reply {
        self.sequence += 1;
        let tx = serde_json::from_value(
            json!({"version":1,"sequence":self.sequence,"operations":operations}),
        )
        .unwrap();
        self.window
            .update(&mut self.cx, |host, window, cx| {
                assert!(
                    gpui_react::current_frame(window, cx).is_none(),
                    "paint scope leaked into native transaction application"
                );
                host.apply(tx, window, cx)
            })
            .unwrap()
            .unwrap()
    }
    pub fn query(&mut self, id: u64) -> Value {
        self.request += 1;
        let request = self.request;
        self.apply(json!([{"op":"query","id":id,"request":request,"value":null}]))
            .results
            .remove(0)
            .value
            .unwrap()
    }
    pub fn command(&mut self, id: u64, value: Value) {
        let operation = self.command_op(id, value);
        let reply = self.apply(json!([operation]));
        assert!(
            reply.results[0].error.is_none(),
            "{:?}",
            reply.results[0].error
        );
    }
    pub fn command_op(&mut self, id: u64, value: Value) -> Value {
        self.request += 1;
        json!({"op":"command","id":id,"request":self.request,"value":value})
    }
    pub fn draw(&mut self) {
        self.cx.run_until_parked();
        self.cx
            .update_window(self.window.into(), |_, window, cx| {
                let _ = window.draw(cx);
            })
            .unwrap();
        self.cx.run_until_parked();
    }
    pub fn wheel(&mut self, x: f32, y: f32, dx: f32, dy: f32) -> DispatchEventResult {
        self.cx.simulate_mouse_move(
            self.window.into(),
            point(px(x), px(y)),
            None,
            Modifiers::default(),
        );
        self.draw();
        self.cx
            .update_window(self.window.into(), |_, window, cx| {
                window.dispatch_event(
                    ScrollWheelEvent {
                        position: point(px(x), px(y)),
                        delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
                        ..Default::default()
                    }
                    .to_platform_input(),
                    cx,
                )
            })
            .unwrap()
    }
    pub fn take_events(&self) -> Vec<Value> {
        self.events
            .lock()
            .unwrap()
            .drain(..)
            .map(|e| e.payload.unwrap())
            .collect()
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.window
            .update(&mut self.cx, |host, window, cx| host.clear(window, cx))
            .ok();
        self.cx
            .update_window(self.window.into(), |_, window, _| window.remove_window())
            .ok();
        self.cx.run_until_parked();
    }
}
pub fn create(id: u64, component: &str, props: Value) -> Value {
    json!({"op":"create","id":id,"component":component,"props":props})
}
pub fn place(id: u64, parent: Option<u64>, before: Option<u64>) -> Value {
    json!({"op":"place","child":id,"parent":parent,"before":before})
}
pub fn row(id: u64, index: usize) -> Value {
    create(
        id,
        "text",
        json!({"text":format!("row {index}"),"style":{"height":20,"lineHeight":20,"fontSize":14,"color":"white"}}),
    )
}
