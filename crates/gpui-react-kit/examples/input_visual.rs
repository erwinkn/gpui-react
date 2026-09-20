//! GPU-backed checks. Run on the main OS thread; all windows stay off screen.
use gpui::{prelude::*, *};
use gpui_react_kit::{Input, InputCommand, InputProps};
use std::{cell::Cell, rc::Rc};

struct Panel {
    input: Entity<Input>,
    scroll: ScrollHandle,
    wheels: Rc<Cell<usize>>,
}
impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let wheels = self.wheels.clone();
        div()
            .id("outer")
            .size_full()
            .bg(rgb(0x202020))
            .text_color(rgb(0xffffff))
            .text_size(px(16.))
            .overflow_y_scroll()
            .restrict_scroll_to_axis()
            .track_scroll(&self.scroll)
            .on_scroll_wheel(move |_, _, _| wheels.set(wheels.get() + 1))
            .child(self.input.clone())
            .child(div().h(px(1000.)).w_full())
    }
}
fn draw(cx: &mut VisualTestAppContext, window: AnyWindowHandle) {
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        let _ = window.draw(cx);
    })
    .unwrap();
}
fn main() -> anyhow::Result<()> {
    let platform = Rc::new(gpui_macos::MacPlatform::new(false));
    let mut cx = VisualTestAppContext::new(platform);
    let wheels = Rc::new(Cell::new(0));
    let window = cx.open_offscreen_window(size(px(400.), px(180.)), |_, cx| {
        cx.new(|cx| Panel {
            input: cx.new(|cx| {
                Input::new(
                    InputProps {
                        initial_multiline: true,
                        min_rows: Some(2),
                        max_rows: Some(2),
                        ..Default::default()
                    },
                    cx,
                )
            }),
            scroll: ScrollHandle::new(),
            wheels: wheels.clone(),
        })
    })?;
    let input = window.update(&mut cx, |panel, window, cx| {
        window.set_logical_active_for_tests(true);
        window.set_a11y_active_for_tests(true);
        panel
            .input
            .update(cx, |input, cx| {
                input.apply_command(InputCommand::Focus, window, cx)
            })
            .unwrap();
        panel.input.clone()
    })?;
    draw(&mut cx, window.into());
    cx.simulate_input(window.into(), "hello");
    draw(&mut cx, window.into());
    assert_eq!(
        input.read_with(&cx, |input, _| input.current_snapshot().value),
        "hello"
    );
    cx.simulate_keystrokes(window.into(), "left shift-left backspace");
    draw(&mut cx, window.into());
    assert_eq!(
        input.read_with(&cx, |input, _| input.current_snapshot().value),
        "helo"
    );
    cx.simulate_keystrokes(window.into(), "cmd-z");
    draw(&mut cx, window.into());
    assert_eq!(
        input.read_with(&cx, |input, _| input.current_snapshot().value),
        "hello"
    );

    // Exercise the real platform input handler, including UTF-16 marked ranges.
    let mut handler = cx.update_window(window.into(), |_, window, _| {
        window
            .take_input_handler_for_tests()
            .expect("focused input handler")
    })?;
    handler.replace_and_mark_text_in_range(Some(0..5), "ni", Some(2..2));
    assert_eq!(handler.marked_text_range(), Some(0..2));
    handler.replace_and_mark_text_in_range(None, "你", Some(1..1));
    handler.replace_text_in_range(None, "你");
    assert_eq!(handler.marked_text_range(), None);
    cx.update_window(window.into(), |_, window, _| {
        window.restore_input_handler_for_tests(handler)
    })?;
    draw(&mut cx, window.into());
    assert_eq!(
        input.read_with(&cx, |input, _| input.current_snapshot().value),
        "你"
    );
    let a11y = cx.update_window(window.into(), |_, window, _| {
        window.debug_a11y_tree_json().unwrap()
    })?;
    assert!(
        a11y.contains("MultilineTextInput") && a11y.contains("你"),
        "input accessibility must expose its current native value: {a11y}"
    );
    let screenshot = cx.capture_screenshot(window.into())?;
    screenshot.save("/tmp/bridge-input.png")?;
    assert!(
        screenshot
            .pixels()
            .filter(|p| p[0] > 180 && p[1] > 180 && p[2] > 180)
            .count()
            > 10,
        "text must reach the GPU image"
    );

    window.update(&mut cx, |panel, window, cx| {
        panel.input.update(cx, |input, cx| {
            input.apply_command(
                InputCommand::Replace {
                    value: (0..20).map(|i| format!("line {i}\n")).collect(),
                    expected_revision: input.current_snapshot().revision,
                },
                window,
                cx,
            )
        })
    })??;
    draw(&mut cx, window.into());
    // Replacement follows the caret to the last line: a downward wheel must chain.
    let bounds = input.read_with(&cx, |input, _| input.current_snapshot().painted.unwrap());
    cx.simulate_mouse_move(
        window.into(),
        point(px(bounds.x + 20.), px(bounds.y + 20.)),
        None,
        Modifiers::default(),
    );
    draw(&mut cx, window.into());
    let result = cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_event(
            ScrollWheelEvent {
                position: point(px(bounds.x + 20.), px(bounds.y + 20.)),
                delta: ScrollDelta::Pixels(point(px(0.), px(-20.))),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        )
    })?;
    assert!(
        result.propagate,
        "an input at its scroll boundary must leave wheel callbacks reachable"
    );
    assert!(wheels.get() > 0, "ancestor wheel callback was lost");
    let offset = window.update(&mut cx, |panel, _, _| panel.scroll.offset())?;
    assert!(
        offset.y < px(0.),
        "outer scroll should consume the boundary wheel: {offset:?}"
    );
    window.update(&mut cx, |panel, _, cx| {
        panel.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    })?;
    draw(&mut cx, window.into());
    let count = wheels.get();
    let result = cx.update_window(window.into(), |_, window, cx| {
        window.dispatch_event(
            ScrollWheelEvent {
                position: point(px(bounds.x + 20.), px(bounds.y + 20.)),
                delta: ScrollDelta::Pixels(point(px(0.), px(20.))),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        )
    })?;
    assert!(
        result.propagate && result.default_prevented,
        "a consumed input wheel must preserve callback propagation"
    );
    assert!(
        wheels.get() > count,
        "consumption lost the ancestor callback"
    );
    assert_eq!(
        window
            .update(&mut cx, |panel, _, _| panel.scroll.offset())?
            .y,
        px(0.),
        "parent must not double-consume the wheel"
    );
    println!("GPU input: typing, selection, undo, IME, pixels, and boundary scrolling passed");
    cx.update_window(window.into(), |_, window, _| window.remove_window())?;
    cx.run_until_parked();
    Ok(())
}
