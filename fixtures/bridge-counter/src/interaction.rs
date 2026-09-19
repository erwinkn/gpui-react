//! Test-only native driver. No input synthesis or shared test flags enter the bridge crates.
use anyhow::{Result, ensure};
use gpui::{prelude::*, *};
use gpui_react_controls::{Input, VirtualList};
use gpui_react_host::gpui_react::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

// These flags synchronize the test, not the UI. They prove every native action
// happens inside the interval in which the worker cannot run React or callbacks.
static BUSY: AtomicBool = AtomicBool::new(false);
static DONE: AtomicBool = AtomicBool::new(false);
#[napi_derive::napi]
pub fn begin_worker_stall() {
    DONE.store(false, Ordering::SeqCst);
    BUSY.store(true, Ordering::SeqCst);
}
#[napi_derive::napi]
pub fn native_probe_done() -> bool {
    DONE.load(Ordering::SeqCst)
}
#[napi_derive::napi]
pub fn finish_worker_stall() {
    BUSY.store(false, Ordering::SeqCst);
}
fn stalled() -> Result<()> {
    ensure!(
        BUSY.load(Ordering::SeqCst),
        "JavaScript resumed before the native action"
    );
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Props {}
struct Driver {
    children: Vec<AnyView>,
    task: Option<Task<()>>,
    report: Option<Value>,
    animation: Rc<Cell<f32>>,
    hover_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}
impl Render for Driver {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let value = self.animation.clone();
        let bounds = self.hover_bounds.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x101010))
            .children(self.children.clone())
            .child(
                div()
                    .id("hover-probe")
                    .h(px(20.))
                    .w_full()
                    .flex_shrink_0()
                    .bg(rgb(0x0000ff))
                    .hover(|style| style.bg(rgb(0xff00ff)))
                    .on_painted(move |area, _, _| bounds.set(Some(area))),
            )
            .child(
                div()
                    .h(px(10.))
                    .bg(rgb(0xff8000))
                    .flex_shrink_0()
                    .with_animation(
                        "native-animation",
                        Animation::new(Duration::from_secs(4)),
                        move |el, phase| {
                            value.set(phase);
                            el.w(px(10. + phase * 100.))
                        },
                    ),
            )
    }
}
impl ReactView for Driver {
    type Props = Props;
    fn create(_: Props, _: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            children: vec![],
            task: None,
            report: None,
            animation: Rc::new(Cell::new(0.)),
            hover_bounds: Rc::new(Cell::new(None)),
        }
    }
    fn set_props(&mut self, _: Props, _: &mut Window, _: &mut Context<Self>) {}
    fn unmounting(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.task = None;
    }
}
impl ReactChildren for Driver {
    fn set_children(&mut self, children: Vec<AnyView>, _: &mut Window, cx: &mut Context<Self>) {
        self.children = children;
        cx.notify();
    }
}
impl ReactCommands for Driver {
    type Command = ();
    fn command(&mut self, _: (), window: &mut Window, cx: &mut Context<Self>) -> Result<()> {
        ensure!(self.task.is_none(), "driver already started");
        let input = self
            .children
            .first()
            .unwrap()
            .clone()
            .downcast::<Input>()
            .ok()
            .unwrap();
        let list = self
            .children
            .get(1)
            .unwrap()
            .clone()
            .downcast::<VirtualList>()
            .ok()
            .unwrap();
        let animation = self.animation.clone();
        let bounds = self.hover_bounds.clone();
        self.task = Some(cx.spawn_in(window, async move |driver, cx| {
            let report = run(input, list, animation, bounds, cx).await;
            let report = match report {
                Ok(value) => value,
                Err(error) => json!({"error":format!("{error:#}")}),
            };
            let _ = driver.update(cx, |driver, _| driver.report = Some(report));
            DONE.store(true, Ordering::SeqCst);
        }));
        Ok(())
    }
}
impl ReactQueries for Driver {
    type Query = ();
    type Reply = Option<Value>;
    fn query(&mut self, _: (), _: &mut Window, _: &mut Context<Self>) -> Result<Self::Reply> {
        Ok(self.report.clone())
    }
}
pub fn register(registry: &mut Registry) -> Result<()> {
    registry.register(
        Component::<Driver>::new("interaction-driver")
            .children()
            .commands()
            .queries(),
    )
}

fn draw(cx: &mut AsyncWindowContext) -> Result<()> {
    stalled()?;
    ensure!(
        cx.update(|window, _| window.is_window_active())?,
        "test window lost logical focus"
    );
    cx.update(|window, cx| window.draw(cx).clear(cx))?;
    Ok(())
}
fn pixels(cx: &mut AsyncWindowContext, color: &str) -> Result<usize> {
    stalled()?;
    let image = cx.update(|window, _| window.render_to_image())??;
    Ok(image
        .pixels()
        .filter(|p| match color {
            "green" => p[0] < 40 && p[1] > 210 && p[2] < 40,
            "magenta" => p[0] > 210 && p[1] < 40 && p[2] > 210,
            "orange" => p[0] > 210 && p[1] > 80 && p[1] < 180 && p[2] < 40,
            _ => false,
        })
        .count())
}
async fn run(
    input: Entity<Input>,
    list: Entity<VirtualList>,
    animation: Rc<Cell<f32>>,
    hover: Rc<Cell<Option<Bounds<Pixels>>>>,
    cx: &mut AsyncWindowContext,
) -> Result<Value> {
    let waiting = Instant::now();
    while !BUSY.load(Ordering::SeqCst) {
        ensure!(
            waiting.elapsed() < Duration::from_secs(5),
            "worker did not enter the stall"
        );
        cx.background_executor()
            .timer(Duration::from_millis(1))
            .await;
    }
    let started = Instant::now();
    cx.update(|window, _| window.set_logical_active_for_tests(true))?;
    draw(cx)?;
    let initial = input.read_with(cx, |input, _| input.current_snapshot());
    let commit = initial.painted.as_ref().unwrap().frame.unwrap().commit;
    for key in ["a", "b", "left", "shift-left", "backspace"] {
        stalled()?;
        cx.update(|window, cx| window.dispatch_keystroke(Keystroke::parse(key).unwrap(), cx))?;
        draw(cx)?;
    }
    ensure!(
        input.read_with(cx, |input, _| input.current_snapshot().value) == "b",
        "native selection deletion failed"
    );
    cx.update(|window, cx| window.dispatch_keystroke(Keystroke::parse("cmd-z").unwrap(), cx))?;
    draw(cx)?;
    ensure!(
        input.read_with(cx, |input, _| input.current_snapshot().value) == "ab",
        "native undo failed"
    );

    // PlatformInputHandler re-enters App. Call it outside cx.update's borrow.
    let mut ime = cx.update(|window, _| window.take_input_handler_for_tests().unwrap())?;
    stalled()?;
    ime.replace_and_mark_text_in_range(Some(0..2), "ni", Some(2..2));
    ensure!(
        ime.marked_text_range() == Some(0..2),
        "IME did not mark its range"
    );
    ime.replace_and_mark_text_in_range(None, "你", Some(1..1));
    ime.replace_text_in_range(None, "你");
    ensure!(ime.marked_text_range().is_none(), "IME did not commit");
    cx.update(|window, _| window.restore_input_handler_for_tests(ime))?;
    draw(cx)?;
    for key in ["end", "c", "d"] {
        stalled()?;
        cx.update(|window, cx| window.dispatch_keystroke(Keystroke::parse(key).unwrap(), cx))?;
        draw(cx)?;
    }
    let after_input = input.read_with(cx, |input, _| input.current_snapshot());
    ensure!(
        after_input.value == "你cd",
        "native text: {}",
        after_input.value
    );
    ensure!(
        after_input.selection.start == 3 && after_input.selection.end == 3,
        "native caret did not follow text"
    );
    let caret_on = pixels(cx, "green")?;
    ensure!(caret_on > 5, "caret did not reach GPU pixels");
    let animation_before = animation.get();
    let animation_pixels_before = pixels(cx, "orange")?;
    let first_frame = after_input.painted.as_ref().unwrap().frame.unwrap().frame;
    let list_before = list.read_with(cx, |list, _| list.snapshot());
    let area = list_before.painted.unwrap().bounds;
    stalled()?;
    let result = cx.update(|window, cx| {
        window.dispatch_event(
            ScrollWheelEvent {
                position: point(px(area.x + 20.), px(area.y + 20.)),
                delta: ScrollDelta::Pixels(point(px(0.), px(-60.))),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        )
    })?;
    draw(cx)?;
    ensure!(
        result.propagate && result.default_prevented,
        "native scroll consumption lost propagation"
    );
    let after_scroll = list.read_with(cx, |list, _| list.snapshot());
    ensure!(
        after_scroll.anchor.index > list_before.anchor.index,
        "wheel did not move the native list"
    );
    let bounds = hover.get().unwrap();
    stalled()?;
    cx.update(|window, cx| {
        window.dispatch_event(
            MouseMoveEvent {
                position: bounds.center(),
                pressed_button: None,
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            cx,
        )
    })?;
    draw(cx)?;
    let hovered = pixels(cx, "magenta")?;
    ensure!(hovered > 100, "native hover style did not reach GPU pixels");

    let notifications = Rc::new(Cell::new(0));
    let count = notifications.clone();
    let _subscription =
        cx.update(|_, cx| cx.observe(&input, move |_, _| count.set(count.get() + 1)))?;
    // Observe a full blink transition instead of depending on one sampled
    // phase. A slow GPU readback can cross the fixed 500 ms phase boundary.
    let blink_started = Instant::now();
    let caret_off = loop {
        cx.background_executor()
            .timer(Duration::from_millis(25))
            .await;
        draw(cx)?;
        let count = pixels(cx, "green")?;
        if count == 0 && notifications.get() > 0 {
            break count;
        }
        ensure!(
            blink_started.elapsed() < Duration::from_secs(2),
            "caret did not blink and receive its native timer notification"
        );
    };
    ensure!(
        notifications.get() > 0,
        "native caret timer did not invalidate the input"
    );
    ensure!(
        animation.get() > animation_before + 0.05,
        "native animation did not advance"
    );
    let animation_pixels_after = pixels(cx, "orange")?;
    ensure!(
        animation_pixels_after > animation_pixels_before + 20,
        "native animation did not change GPU pixels"
    );
    let end = input.read_with(cx, |input, _| input.current_snapshot());
    let frame = end.painted.unwrap().frame.unwrap();
    ensure!(
        frame.commit == commit && frame.frame > first_frame,
        "native paint must advance without a React commit"
    );
    stalled()?;
    Ok(
        json!({"value":end.value,"revision":end.revision,"commit":commit,"nativeFrames":frame.frame-first_frame,"elapsedMs":started.elapsed().as_millis(),"scrollIndex":after_scroll.anchor.index,"caretOnPixels":caret_on,"caretOffPixels":caret_off,"hoverPixels":hovered,"animationBefore":animation_before,"animationAfter":animation.get(),"animationPixelsBefore":animation_pixels_before,"animationPixelsAfter":animation_pixels_after,"caretNotifications":notifications.get()}),
    )
}
