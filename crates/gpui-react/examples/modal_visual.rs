#[allow(dead_code)]
mod support;
use gpui::{prelude::*, *};
use gpui_react::{Component, ReactQueries, ReactView};
use gpui_react::{document_text, geometry::Rect};
use serde_json::{Value, json};
use support::*;

struct Modals {
    depth: usize,
    presses: usize,
    trigger: FocusHandle,
    dialogs: [FocusHandle; 2],
    bounds: [Option<Bounds<Pixels>>; 2],
}
impl Modals {
    fn open(&mut self, depth: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.depth = depth;
        self.dialogs[depth - 1].focus(window, cx);
        cx.notify();
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.depth -= 1;
        if self.depth == 0 {
            self.trigger.focus(window, cx);
        } else {
            self.dialogs[self.depth - 1].focus(window, cx);
        }
        cx.notify();
    }
    fn dialog(&self, level: usize, window: &Window, cx: &Context<Self>) -> AnyElement {
        let active = self.depth == level;
        let owner = cx.weak_entity();
        let mut panel = div()
            .id(format!("dialog-{level}"))
            .absolute()
            .left(px(40. + level as f32 * 20.))
            .top(px(40. + level as f32 * 20.))
            .w(px(260.))
            .h(px(160.))
            .rounded(px(12.))
            .overflow_hidden()
            .bg(if level == 1 {
                rgb(0x303a50)
            } else {
                rgb(0x205040)
            })
            .role(accesskit::Role::Dialog)
            .aria_label(format!("Dialog {level}"))
            .aria_modal(active)
            .aria_disabled(!active)
            .track_focus(&self.dialogs[level - 1])
            .child(document_text(
                format!("title-{level}"),
                if level == 1 {
                    "Enter opens child; Escape closes"
                } else {
                    "Child dialog; Escape returns"
                },
            ))
            .child(
                div()
                    .absolute()
                    .left(px(-20.))
                    .top(px(100.))
                    .w(px(300.))
                    .h(px(30.))
                    .bg(if level == 1 {
                        rgb(0xff8000)
                    } else {
                        rgb(0xff00c8)
                    }),
            )
            .on_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                if view.depth != level {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "escape" => view.close(window, cx),
                    "enter" if level == 1 => view.open(2, window, cx),
                    // This small fixture has one focus owner in each dialog.
                    "tab" => {}
                    _ => return,
                }
                cx.stop_propagation();
                window.prevent_default();
            }))
            .on_painted(move |bounds, _, cx| {
                owner
                    .update(cx, |view, _| view.bounds[level - 1] = Some(bounds))
                    .unwrap();
            });
        if level == 1 && self.depth == 2 {
            panel = panel.child(self.dialog(2, window, cx));
        }
        deferred(
            anchored().position(Point::default()).child(
                div()
                    .id(format!("backdrop-{level}"))
                    .relative()
                    .w(window.viewport_size().width)
                    .h(window.viewport_size().height)
                    .bg(rgba(0x00000066))
                    .occlude()
                    .child(panel),
            ),
        )
        .with_priority(level)
        .into_any_element()
    }
}
impl Render for Modals {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.bounds = [None, None];
        div()
            .relative()
            .size_full()
            .text_color(rgb(0xffffff))
            .text_size(px(14.))
            .line_height(px(20.))
            .child(
                div()
                    .id("trigger")
                    .absolute()
                    .left(px(16.))
                    .top(px(16.))
                    .w(px(140.))
                    .h(px(32.))
                    .bg(rgb(0x205080))
                    .role(accesskit::Role::Button)
                    .aria_label("Open dialog")
                    .aria_disabled(self.depth > 0)
                    .track_focus(&self.trigger)
                    .child(document_text("trigger-label", "Open dialog"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _, _, _| view.presses += 1),
                    )
                    .on_click(cx.listener(|view, _, window, cx| {
                        if view.depth == 0 {
                            view.open(1, window, cx);
                        }
                    })),
            )
            .when(self.depth > 0, |root| {
                root.child(self.dialog(1, window, cx))
            })
    }
}
impl ReactView for Modals {
    type Props = ();
    fn create(_: (), _: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            depth: 0,
            presses: 0,
            trigger: cx.focus_handle(),
            dialogs: [cx.focus_handle(), cx.focus_handle()],
            bounds: [None, None],
        }
    }
    fn set_props(&mut self, _: (), _: &mut Window, _: &mut Context<Self>) {}
}
impl ReactQueries for Modals {
    type Query = ();
    type Reply = Value;
    fn query(
        &mut self,
        _: (),
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> anyhow::Result<Value> {
        Ok(
            json!({"depth":self.depth,"presses":self.presses,"triggerFocused":self.trigger.is_focused(window),
            "outerFocused":self.dialogs[0].is_focused(window),"innerFocused":self.dialogs[1].is_focused(window),
            "bounds":self.bounds.map(|bounds|bounds.map(Rect::from))}),
        )
    }
}
fn click(h: &mut Harness, x: f32, y: f32) {
    let point = point(px(x), px(y));
    h.cx.simulate_mouse_move(h.window.into(), point, None, Modifiers::default());
    h.draw();
    h.cx.simulate_mouse_down(
        h.window.into(),
        point,
        MouseButton::Left,
        Modifiers::default(),
    );
    h.cx.simulate_mouse_up(
        h.window.into(),
        point,
        MouseButton::Left,
        Modifiers::default(),
    );
    h.draw();
}
fn key(h: &mut Harness, key: &str) {
    h.cx.simulate_keystrokes(h.window.into(), key);
    h.draw();
}
fn a11y(h: &mut Harness) -> Value {
    h.cx.update_window(h.window.into(), |_, window, _| {
        serde_json::from_str(&window.debug_a11y_tree_json().unwrap()).unwrap()
    })
    .unwrap()
}
fn main() {
    let mut h = Harness::with_components(440., 300., |registry| {
        registry
            .register(Component::<Modals>::new("modals").queries())
            .unwrap();
    });
    h.cx.update_window(h.window.into(), |_, window, _| {
        window.set_a11y_active_for_tests(true)
    })
    .unwrap();
    h.apply(json!([
        create(1, "modals", Value::Null),
        place(1, None, None)
    ]));
    h.draw();
    click(&mut h, 30., 28.);
    assert_eq!(h.query(1)["outerFocused"], true);
    key(&mut h, "enter");
    let state = h.query(1);
    assert_eq!(state["depth"], 2);
    assert_eq!(state["innerFocused"], true);
    let presses = state["presses"].clone();
    click(&mut h, 30., 28.);
    assert_eq!(
        h.query(1)["presses"],
        presses,
        "backdrop must block actual underlay mouse delivery"
    );
    key(&mut h, "tab");
    assert_eq!(h.query(1)["innerFocused"], true);
    let tree = a11y(&mut h);
    let nodes = tree["nodes"].as_object().unwrap();
    let (active_id, inner) = nodes
        .iter()
        .find(|(_, node)| node["aria"]["label"] == "Dialog 2")
        .unwrap();
    assert_eq!(inner["aria"]["role"], "Dialog");
    assert_eq!(inner["aria"]["modal"], true);
    assert_eq!(tree["gpui_focus"], active_id.as_str());
    let outer = nodes
        .values()
        .find(|node| node["aria"]["label"] == "Dialog 1")
        .unwrap();
    assert_eq!(outer["aria"]["disabled"], true);
    let image = h.cx.capture_screenshot(h.window.into()).unwrap();
    let viewport = tree["frame"]["viewport_size"]["width"].as_f64().unwrap();
    let scale = f64::from(image.width()) / viewport;
    let bounds = &state["bounds"][1];
    let x = bounds["x"].as_f64().unwrap();
    let y = bounds["y"].as_f64().unwrap();
    let pixel =
        |x: f64, y: f64| *image.get_pixel((x * scale).round() as u32, (y * scale).round() as u32);
    let inside = pixel(x + 8., y + 110.);
    let outside = pixel(x - 8., y + 110.);
    assert!(
        inside[0] > 200 && inside[1] < 40 && inside[2] > 140,
        "inner content must paint"
    );
    assert!(
        !(outside[0] > 200 && outside[1] < 40 && outside[2] > 140),
        "inner content must respect the dialog clip"
    );
    image.save("/tmp/bridge-native-modals.png").unwrap();
    key(&mut h, "escape");
    assert_eq!(h.query(1)["depth"], 1);
    assert_eq!(h.query(1)["outerFocused"], true);
    let tree = a11y(&mut h);
    assert!(
        !tree["nodes"]
            .as_object()
            .unwrap()
            .values()
            .any(|node| node["aria"]["label"] == "Dialog 2")
    );
    key(&mut h, "escape");
    assert_eq!(h.query(1)["depth"], 0);
    assert_eq!(h.query(1)["triggerFocused"], true);
    click(&mut h, 30., 28.);
    assert_eq!(
        h.query(1)["presses"].as_u64().unwrap(),
        presses.as_u64().unwrap() + 1
    );
    println!(
        "PASS nested native modals: deferred ordering, pointer blocking, keyboard focus, Escape restoration, clipping pixels, accessible modality, and accessible focus"
    );
}
