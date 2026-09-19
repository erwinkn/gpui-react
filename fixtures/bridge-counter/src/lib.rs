//! An ordinary GPUI view plus a small React binding implementation.
use gpui_react_host::gpui_react::{
    gpui::{self, Context, EventEmitter, Render, Task, Window, div, prelude::*},
    *,
};
pub use gpui_react_host::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[cfg(feature = "interaction-tests")]
mod interaction;
#[cfg(feature = "interaction-tests")]
pub use interaction::{begin_worker_stall, finish_worker_stall, native_probe_done};
#[cfg(feature = "interaction-tests")]
mod lifecycle;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Props {
    step: u32,
}

struct Counter {
    count: u32,
    step: u32,
    renders: u32,
    ticking: Option<Task<()>>,
}

#[derive(Serialize)]
struct Changed {
    value: u32,
}
impl EventEmitter<Changed> for Counter {}

impl Counter {
    fn increment(&mut self, cx: &mut Context<Self>) {
        self.count += self.step;
        cx.notify();
        cx.emit(Changed { value: self.count });
    }
}

impl Render for Counter {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders += 1;
        // This component uses normal GPUI text and input. The React binding
        // does not replace its rendering method or introduce a second state.
        div()
            .id("counter")
            .p_4()
            .bg(gpui::rgb(0x303030))
            .text_color(gpui::rgb(0xffffff))
            .child(format!("Count: {}", self.count))
            .on_click(cx.listener(|this, _, _, cx| this.increment(cx)))
    }
}

impl ReactView for Counter {
    type Props = Props;
    fn create(props: Props, _: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            count: 0,
            step: props.step,
            renders: 0,
            ticking: None,
        }
    }
    fn set_props(&mut self, props: Props, _: &mut Window, cx: &mut Context<Self>) {
        if self.step != props.step {
            self.step = props.step;
            cx.notify();
        }
    }
    fn unmounting(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.ticking = None;
    }
}
impl ReactEvents for Counter {
    type Event = Changed;
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Command {
    Increment,
    Start,
    Stop,
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
            Command::Increment => self.increment(cx),
            Command::Stop => self.ticking = None,
            Command::Start => {
                self.ticking = Some(cx.spawn(async |view, cx| {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(10))
                            .await;
                        if view.update(cx, |view, cx| view.increment(cx)).is_err() {
                            break;
                        }
                    }
                }));
            }
        }
        Ok(())
    }
}
impl ReactQueries for Counter {
    type Query = ();
    type Reply = serde_json::Value;
    fn query(
        &mut self,
        _: (),
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> anyhow::Result<Self::Reply> {
        Ok(serde_json::json!({"count":self.count,"step":self.step,"renders":self.renders}))
    }
}

#[napi_derive::module_init]
fn register() {
    register_components(|registry| {
        gpui_react_controls::register(registry)?;
        #[cfg(feature = "interaction-tests")]
        interaction::register(registry)?;
        #[cfg(feature = "interaction-tests")]
        lifecycle::register(registry)?;
        registry.register(
            Component::<Counter>::new("counter")
                .events()
                .commands()
                .queries(),
        )
    });
}
