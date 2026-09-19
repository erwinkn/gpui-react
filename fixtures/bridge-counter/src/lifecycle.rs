//! Test-only cleanup probe, including teardown while JavaScript cannot respond.
use gpui::{prelude::*, *};
use gpui_react_host::gpui_react::*;
use serde::Deserialize;
use std::{io::Write, time::Duration};

#[derive(Deserialize)]
struct Props {
    record_path: String,
}
struct Probe {
    path: String,
    task: Option<Task<()>>,
}
fn record(path: &str, message: &str) {
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .unwrap();
    writeln!(file, "{message}").unwrap();
}
impl Drop for Probe {
    fn drop(&mut self) {
        record(&self.path, "drop");
    }
}
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}
impl ReactView for Probe {
    type Props = Props;
    fn create(props: Props, _: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            path: props.record_path,
            task: None,
        }
    }
    fn set_props(&mut self, _: Props, _: &mut Window, _: &mut Context<Self>) {}
    fn mounted(&mut self, _: &mut Window, _: &mut Context<Self>) {
        record(&self.path, "mounted");
    }
    fn unmounting(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.task = None;
        record(&self.path, "unmount");
    }
}
impl EventEmitter<u32> for Probe {}
impl ReactEvents for Probe {
    type Event = u32;
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Command {
    Close,
    Overflow,
}
impl ReactCommands for Probe {
    type Command = Command;
    fn command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.task = Some(cx.spawn_in(window, async move |probe, cx| {
            while !super::interaction::worker_stalled() {
                cx.background_executor()
                    .timer(Duration::from_millis(1))
                    .await;
            }
            match command {
                Command::Close => {
                    let _ = cx.update(|window, _| window.remove_window());
                }
                Command::Overflow => {
                    let _ = probe.update(cx, |_, cx| {
                        for value in 0..10_000 {
                            cx.emit(value);
                        }
                    });
                }
            }
        }));
        Ok(())
    }
}
pub fn register(registry: &mut Registry) -> anyhow::Result<()> {
    registry.register(
        Component::<Probe>::new("lifecycle-probe")
            .events()
            .commands(),
    )
}
