#[allow(dead_code)]
mod support;
use gpui::{prelude::*, *};
use gpui_react::{Component, FrameInfo, ReactEvents, ReactQueries, ReactView};
use gpui_react_controls::{
    document_text,
    geometry::{Offset, Rect},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};
use support::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Props {
    width: f32,
    card_width: f32,
    gap: f32,
}
#[derive(Serialize)]
struct Selected {
    count: usize,
}
#[derive(Serialize)]
struct Drawn {
    frame: FrameInfo,
    trigger: Rect,
    left: Rect,
    right: Rect,
    from: Offset,
    to: Offset,
    was_open: bool,
    menu: Option<Rect>,
}
struct GeometryPanel {
    props: Props,
    open: bool,
    selected: usize,
    trigger_focus: FocusHandle,
    menu_focus: FocusHandle,
    drawn: Option<Drawn>,
    checked_frames: usize,
}
impl GeometryPanel {
    fn choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected += 1;
        cx.emit(Selected {
            count: self.selected,
        });
        self.close(window, cx);
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        self.trigger_focus.focus(window, cx);
        cx.notify();
    }
}
impl Render for GeometryPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let trigger_bounds = Rc::new(Cell::new(None));
        let left_bounds = Rc::new(Cell::new(None));
        let right_bounds = Rc::new(Cell::new(None));
        let trigger_record = trigger_bounds.clone();
        let left_record = left_bounds.clone();
        let right_record = right_bounds.clone();
        let menu_owner = cx.weak_entity();
        let owner = cx.weak_entity();
        let was_open = self.open;
        let trigger = div()
            .id("trigger")
            .relative()
            .w_full()
            .h(px(32.))
            .bg(rgb(0x205080))
            .role(accesskit::Role::Button)
            .track_focus(&self.trigger_focus)
            .child(document_text("trigger-label", "Open menu"))
            .on_click(cx.listener(|view, _, window, cx| {
                view.open = true;
                view.menu_focus.focus(window, cx);
                cx.notify();
            }))
            .on_painted(move |bounds, _, _| trigger_record.set(Some(bounds)))
            .when(self.open, |trigger| {
                trigger.child(
                    div().absolute().left_0().bottom_0().w_full().h_0().child(
                        deferred(
                            anchored().match_parent_width().child(
                                div()
                                    .id("menu")
                                    .w_full()
                                    .h(px(64.))
                                    .bg(rgb(0x205d40))
                                    .track_focus(&self.menu_focus)
                                    .role(accesskit::Role::Menu)
                                    .child(document_text("menu-label", "Choose this item"))
                                    .on_key_down(cx.listener(
                                        |view, event: &KeyDownEvent, window, cx| {
                                            match event.keystroke.key.as_str() {
                                                "escape" => view.close(window, cx),
                                                "enter" | "space" => view.choose(window, cx),
                                                _ => return,
                                            }
                                            cx.stop_propagation();
                                        },
                                    ))
                                    .on_mouse_down_out(
                                        cx.listener(|view, _, window, cx| view.close(window, cx)),
                                    )
                                    .on_click(
                                        cx.listener(|view, _, window, cx| view.choose(window, cx)),
                                    )
                                    .on_painted(move |bounds, window, cx| {
                                        let frame = gpui_react::current_frame(window, cx).unwrap();
                                        menu_owner
                                            .update(cx, |view, _| {
                                                let drawn = view.drawn.as_mut().unwrap();
                                                assert_eq!(frame.frame, drawn.frame.frame);
                                                assert_eq!(frame.root, drawn.frame.root);
                                                drawn.menu = Some(bounds.into());
                                            })
                                            .unwrap();
                                    }),
                            ),
                        )
                        .with_priority(1),
                    ),
                )
            });
        div()
            .relative()
            .w(px(self.props.width))
            .h(px(260.))
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .text_color(rgb(0xffffff))
            .text_size(px(14.))
            .child(trigger)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(self.props.gap))
                    .w_full()
                    .h(px(110.))
                    .child(
                        div()
                            .w(px(self.props.card_width))
                            .h(px(64.))
                            .flex_shrink_0()
                            .bg(rgb(0x303030))
                            .child(document_text("left-label", "A"))
                            .on_painted(move |bounds, _, _| left_record.set(Some(bounds))),
                    )
                    .child(
                        div()
                            .flex_grow(1.)
                            .h(px(88.))
                            .bg(rgb(0x454545))
                            .child(document_text("right-label", "B"))
                            .on_painted(move |bounds, _, _| right_record.set(Some(bounds))),
                    ),
            )
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, cx| {
                        // These are the boxes painted earlier in this same draw. The
                        // connector adds graphics; GPUI still lays out both cards.
                        let left: Bounds<Pixels> = left_bounds.get().unwrap();
                        let right: Bounds<Pixels> = right_bounds.get().unwrap();
                        let from = point(left.right(), left.center().y);
                        let to = point(right.left(), right.center().y);
                        let mut line = PathBuilder::stroke(px(2.));
                        line.move_to(from);
                        line.line_to(to);
                        window.paint_path(line.build().unwrap(), rgb(0x00ffcc));
                        let frame = gpui_react::current_frame(window, cx).unwrap();
                        owner
                            .update(cx, |view, _| {
                                view.drawn = Some(Drawn {
                                    frame,
                                    trigger: trigger_bounds.get().unwrap().into(),
                                    left: left.into(),
                                    right: right.into(),
                                    from: from.into(),
                                    to: to.into(),
                                    was_open,
                                    menu: None,
                                })
                            })
                            .unwrap();
                        window.on_draw_complete(move |_, cx| {
                            owner
                                .update(cx, |view, _| {
                                    let drawn = view.drawn.as_ref().unwrap();
                                    near(drawn.from.x, drawn.left.x + drawn.left.width);
                                    near(drawn.from.y, drawn.left.y + drawn.left.height / 2.);
                                    near(drawn.to.x, drawn.right.x);
                                    near(drawn.to.y, drawn.right.y + drawn.right.height / 2.);
                                    if drawn.was_open {
                                        let menu = drawn.menu.unwrap();
                                        near(menu.width, drawn.trigger.width);
                                        near(menu.x, drawn.trigger.x);
                                        near(menu.y, drawn.trigger.y + drawn.trigger.height);
                                    } else {
                                        assert!(drawn.menu.is_none());
                                    }
                                    view.checked_frames += 1;
                                })
                                .unwrap();
                        });
                    },
                )
                .absolute()
                .left_0()
                .top_0()
                .size_full(),
            )
    }
}
impl ReactView for GeometryPanel {
    type Props = Props;
    fn create(props: Props, _: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            props,
            open: false,
            selected: 0,
            trigger_focus: cx.focus_handle(),
            menu_focus: cx.focus_handle(),
            drawn: None,
            checked_frames: 0,
        }
    }
    fn set_props(&mut self, props: Props, _: &mut Window, cx: &mut Context<Self>) {
        self.props = props;
        cx.notify();
    }
}
impl EventEmitter<Selected> for GeometryPanel {}
impl ReactEvents for GeometryPanel {
    type Event = Selected;
}
impl ReactQueries for GeometryPanel {
    type Query = ();
    type Reply = Value;
    fn query(
        &mut self,
        _: (),
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> anyhow::Result<Value> {
        Ok(
            json!({"open":self.open,"selected":self.selected,"drawn":self.drawn,"checkedFrames":self.checked_frames,
            "triggerFocused":self.trigger_focus.is_focused(window),"menuFocused":self.menu_focus.is_focused(window)}),
        )
    }
}
fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.75, "{actual} != {expected}");
}
fn props(width: f32, card_width: f32, gap: f32) -> Value {
    json!({"width":width,"cardWidth":card_width,"gap":gap})
}
fn click(h: &mut Harness, x: f32, y: f32) {
    let position = point(px(x), px(y));
    h.cx.simulate_mouse_move(h.window.into(), position, None, Modifiers::default());
    h.draw();
    h.cx.simulate_mouse_down(
        h.window.into(),
        position,
        MouseButton::Left,
        Modifiers::default(),
    );
    h.cx.simulate_mouse_up(
        h.window.into(),
        position,
        MouseButton::Left,
        Modifiers::default(),
    );
    h.draw();
}
fn main() {
    let mut h = Harness::with_components(500., 320., |registry| {
        registry
            .register(
                Component::<GeometryPanel>::new("geometry")
                    .events()
                    .queries(),
            )
            .unwrap();
    });
    h.apply(json!([create(1,"geometry",props(360.,120.,32.)),place(1,None,None),{"op":"listen","id":1,"subscription":1}]));
    h.draw();
    assert_eq!(h.query(1)["open"], false);
    click(&mut h, 32., 28.);
    assert_eq!(h.query(1)["menuFocused"], true);
    for (width, card, gap) in [(240., 70., 20.), (420., 140., 48.), (300., 90., 24.)] {
        h.apply(json!([{"op":"props","id":1,"props":props(width,card,gap)}]));
        h.draw();
        let state = h.query(1);
        assert_eq!(state["open"], true);
        assert_eq!(state["menuFocused"], true);
        near(
            state["drawn"]["trigger"]["width"].as_f64().unwrap() as f32,
            width - 32.,
        );
        near(
            state["drawn"]["left"]["width"].as_f64().unwrap() as f32,
            card,
        );
        near(
            (state["drawn"]["right"]["x"].as_f64().unwrap()
                - state["drawn"]["left"]["x"].as_f64().unwrap()
                - f64::from(card)) as f32,
            gap,
        );
    }
    h.cx.simulate_keystrokes(h.window.into(), "escape");
    h.draw();
    assert_eq!(h.query(1)["open"], false);
    assert_eq!(h.query(1)["triggerFocused"], true);
    click(&mut h, 32., 28.);
    click(&mut h, 32., 68.);
    assert_eq!(h.query(1)["selected"], 1);
    assert_eq!(h.query(1)["open"], false);
    assert_eq!(h.take_events(), vec![json!({"count":1})]);
    click(&mut h, 32., 28.);
    h.cx.simulate_keystrokes(h.window.into(), "enter");
    h.draw();
    assert_eq!(h.query(1)["selected"], 2);
    assert_eq!(h.query(1)["triggerFocused"], true);
    assert_eq!(h.take_events(), vec![json!({"count":2})]);
    click(&mut h, 32., 28.);
    click(&mut h, 480., 280.);
    assert_eq!(h.query(1)["open"], false);
    let state = h.query(1);
    assert!(state["checkedFrames"].as_u64().unwrap() > 10);
    let image = h.cx.capture_screenshot(h.window.into()).unwrap();
    assert!(
        image
            .pixels()
            .filter(|p| p[0] < 80 && p[1] > 180 && p[2] > 140)
            .count()
            > 20,
        "connector must reach GPU pixels"
    );
    let scale =
        f64::from(image.width()) / state["drawn"]["frame"]["viewportWidth"].as_f64().unwrap();
    let from = &state["drawn"]["from"];
    let to = &state["drawn"]["to"];
    for fraction in [0.2, 0.5, 0.8] {
        let x = ((from["x"].as_f64().unwrap() * (1. - fraction)
            + to["x"].as_f64().unwrap() * fraction)
            * scale)
            .round() as i32;
        let y = ((from["y"].as_f64().unwrap() * (1. - fraction)
            + to["y"].as_f64().unwrap() * fraction)
            * scale)
            .round() as i32;
        assert!(
            (-2..=2).any(|dx| (-2..=2).any(|dy| {
                let pixel = image.get_pixel((x + dx) as u32, (y + dy) as u32);
                pixel[0] < 80 && pixel[1] > 180 && pixel[2] > 140
            })),
            "connector pixels must follow the current card endpoints"
        );
    }
    image.save("/tmp/bridge-native-geometry.png").unwrap();
    click(&mut h, 32., 28.);
    h.cx.capture_screenshot(h.window.into())
        .unwrap()
        .save("/tmp/bridge-native-menu.png")
        .unwrap();
    println!(
        "PASS native trigger-width menu and card connectors: every-draw geometry, resize, focus, Escape, native selection events, outside dismissal, and GPU pixels"
    );
}
