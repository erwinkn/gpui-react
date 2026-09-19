#[allow(dead_code)]
mod support;
use gpui::{prelude::*, *};
use gpui_react::{Component, FrameInfo, ReactCommands, ReactQueries, ReactView};
use gpui_react_controls::{
    Color, Document, DocumentCommand, DocumentProps, Length, Style, document_text,
    geometry::{Offset, Rect},
};
use serde::Deserialize;
use serde_json::{Value, json};
use support::*;

const SOURCE: &str = "alpha bravo charlie delta echo target";
const KEY: &str = "body";

struct Toolbar {
    document: WeakEntity<Document>,
    painted: Option<(Bounds<Pixels>, FrameInfo)>,
    clicks: usize,
}
impl Render for Toolbar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        div()
            .id("copy-toolbar")
            .w(px(96.))
            .h(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(14.))
            .line_height(px(20.))
            .role(accesskit::Role::Button)
            .bg(rgb(0x2360a0))
            .hover(|style| style.bg(rgb(0x3480c0)))
            .text_color(rgb(0xffffff))
            .child(
                document_text("toolbar-label", "Copy")
                    .selectable(false)
                    .searchable(false),
            )
            .on_click(cx.listener(|toolbar, _, window, cx| {
                toolbar
                    .document
                    .update(cx, |doc, cx| {
                        doc.apply_command(DocumentCommand::Copy, window, cx)
                    })
                    .unwrap()
                    .unwrap();
                toolbar.clicks += 1;
            }))
            .on_painted(move |bounds, window, cx| {
                let frame = gpui_react::current_frame(window, cx).unwrap();
                this.update(cx, |toolbar, _| toolbar.painted = Some((bounds, frame)))
                    .unwrap();
            })
    }
}

struct Paragraph {
    source: SharedString,
    document: WeakEntity<Document>,
    toolbar: Entity<Toolbar>,
    anchor: Option<Point<Pixels>>,
    frame: Option<FrameInfo>,
    checked_frames: usize,
}
impl Render for Paragraph {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let text = document_text(KEY, self.source.clone());
        let layout = text.layout().clone();
        let source = self.source.clone();
        let document = self.document.clone();
        let toolbar = self.toolbar.clone();
        let this = cx.weak_entity();
        div().relative().w_full().child(text).child(
            canvas(
                move |_, window, cx| {
                    let range = document.upgrade()?.read(cx).selected_range(&KEY.into(), &source)?;
                    // The preceding text has completed prepaint. This is GPUI's
                    // current shaped layout, not a saved JS measurement.
                    let anchor = layout.position_for_index(range.end)?
                        + point(px(0.), layout.line_height() + px(4.));
                    let mut popup = anchored()
                        .position(anchor)
                        .child(toolbar)
                        .into_any_element();
                    popup.layout_as_root(
                        size(AvailableSpace::MaxContent, AvailableSpace::MaxContent),
                        window,
                        cx,
                    );
                    window.defer_draw(popup, Point::default(), 1, None);
                    Some(anchor)
                },
                move |_, anchor, window, cx| {
                    let frame = gpui_react::current_frame(window, cx).unwrap();
                    this.update(cx, |paragraph, _| {
                        paragraph.anchor = anchor;
                        paragraph.frame = Some(frame);
                    })
                    .unwrap();
                    // Check every draw, including automatic test-platform draws,
                    // so a second draw cannot hide a stale first-frame position.
                    window.on_draw_complete(move |_, cx| {
                        this.update(cx, |paragraph, cx| paragraph.check_geometry(cx))
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
impl Paragraph {
    fn check_geometry(&mut self, cx: &App) {
        let Some(anchor) = self.anchor else { return };
        let document = self.document.upgrade().unwrap();
        let snapshot = document.read(cx).snapshot();
        let range = snapshot
            .ranges
            .iter()
            .find(|range| range.key == KEY)
            .unwrap();
        let last = range.rects.last().unwrap();
        near(anchor.x.into(), last.x + last.width);
        near(anchor.y.into(), last.y + last.height + 4.);
        let (bounds, frame) = self.toolbar.read(cx).painted.unwrap();
        assert_eq!(frame.frame, self.frame.unwrap().frame);
        assert_eq!(frame.root, self.frame.unwrap().root);
        near(bounds.left().into(), f32::from(anchor.x).round());
        near(bounds.top().into(), f32::from(anchor.y).round());
        self.checked_frames += 1;
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Props {
    width: f32,
    font_size: f32,
    source: String,
}
fn document_props(props: &Props) -> DocumentProps {
    DocumentProps {
        style: Style {
            width: Some(Length::Pixels(props.width)),
            height: Some(Length::Pixels(300.)),
            font_size: Some(props.font_size),
            line_height: Some(props.font_size + 8.),
            color: Some(Color(rgb(0xffffff).into())),
            ..Default::default()
        }
        .into(),
        ..Default::default()
    }
}
struct SelectionPanel {
    document: Entity<Document>,
    paragraph: Entity<Paragraph>,
}
impl Render for SelectionPanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.document.clone()
    }
}
impl ReactView for SelectionPanel {
    type Props = Props;
    fn create(props: Props, _: &mut Window, cx: &mut Context<Self>) -> Self {
        let document = cx.new(|cx| Document::new(document_props(&props), cx));
        let toolbar = cx.new(|_| Toolbar {
            document: document.downgrade(),
            painted: None,
            clicks: 0,
        });
        let paragraph = cx.new(|_| Paragraph {
            source: props.source.into(),
            document: document.downgrade(),
            toolbar,
            anchor: None,
            frame: None,
            checked_frames: 0,
        });
        document.update(cx, |doc, cx| {
            doc.set_native_children(vec![paragraph.clone().into()], cx)
        });
        Self {
            document,
            paragraph,
        }
    }
    fn set_props(&mut self, props: Props, window: &mut Window, cx: &mut Context<Self>) {
        self.document.update(cx, |doc, cx| {
            doc.set_props(document_props(&props), window, cx)
        });
        self.paragraph.update(cx, |paragraph, cx| {
            paragraph.source = props.source.into();
            cx.notify();
        });
    }
    fn unmounting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.document
            .update(cx, |doc, cx| doc.unmounting(window, cx));
    }
}
impl ReactCommands for SelectionPanel {
    type Command = DocumentCommand;
    fn command(
        &mut self,
        command: Self::Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.document
            .update(cx, |doc, cx| doc.apply_command(command, window, cx))
    }
}
impl ReactQueries for SelectionPanel {
    type Query = ();
    type Reply = Value;
    fn query(&mut self, _: (), _: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<Value> {
        let paragraph = self.paragraph.read(cx);
        let toolbar = paragraph.toolbar.read(cx);
        Ok(
            json!({"document": self.document.read(cx).snapshot(),"anchor":paragraph.anchor.map(Offset::from),
            "toolbar":paragraph.anchor.and(toolbar.painted).map(|(bounds,_)|Rect::from(bounds)),
            "clicks":toolbar.clicks,"checkedFrames":paragraph.checked_frames}),
        )
    }
}
fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.75, "{actual} != {expected}");
}
fn props(width: f32, font_size: f32, source: &str) -> Value {
    json!({"width":width,"fontSize":font_size,"source":source})
}
fn main() {
    let mut h = Harness::with_components(500., 400., |registry| {
        registry
            .register(
                Component::<SelectionPanel>::new("selection-panel")
                    .commands()
                    .queries(),
            )
            .unwrap();
    });
    h.apply(json!([
        create(1, "selection-panel", props(280., 16., SOURCE)),
        place(1, None, None)
    ]));
    h.draw();
    let snapshot = h.query(1);
    assert!(snapshot["toolbar"].is_null());
    h.command(1,json!({"type":"select","start":{"key":KEY,"offset":SOURCE.len()-6},"end":{"key":KEY,"offset":SOURCE.len()},"expectedContentRevision":snapshot["document"]["contentRevision"]}));
    h.draw();
    let before = h.query(1);
    assert_eq!(before["document"]["selection"], "target");
    assert!(before["checkedFrames"].as_u64().unwrap() > 0);
    for (width, font) in [(130., 20.), (240., 18.), (180., 24.)] {
        h.apply(json!([{"op":"props","id":1,"component":"selection-panel","props":props(width,font,SOURCE)}]));
        h.draw();
        let resized = h.query(1);
        assert_eq!(resized["document"]["selection"], "target");
        assert!(
            resized["checkedFrames"].as_u64().unwrap() > before["checkedFrames"].as_u64().unwrap()
        );
        assert_ne!(resized["anchor"], before["anchor"]);
    }
    let snapshot = h.query(1);
    let range = &snapshot["document"]["ranges"][0]["rects"][0];
    let selection_point = gpui::point(
        px(range["x"].as_f64().unwrap() as f32 + 4.),
        px(range["y"].as_f64().unwrap() as f32 + 10.),
    );
    h.command(1, json!({"type":"clear"}));
    h.draw();
    assert!(h.query(1)["toolbar"].is_null());
    h.cx.simulate_mouse_move(h.window.into(), selection_point, None, Modifiers::default());
    h.draw();
    h.cx.update_window(h.window.into(), |_, window, cx| {
        window.dispatch_event(
            MouseDownEvent {
                position: selection_point,
                button: MouseButton::Left,
                click_count: 2,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        )
    })
    .unwrap();
    h.cx.simulate_mouse_up(
        h.window.into(),
        selection_point,
        MouseButton::Left,
        Modifiers::default(),
    );
    h.draw();
    let snapshot = h.query(1);
    assert_eq!(snapshot["document"]["selection"], "target");
    let bounds = &snapshot["toolbar"];
    let point = gpui::point(
        px(bounds["x"].as_f64().unwrap() as f32 + 12.),
        px(bounds["y"].as_f64().unwrap() as f32 + 12.),
    );
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
    assert_eq!(h.query(1)["clicks"], 1);
    assert_eq!(
        h.cx.read_from_clipboard().unwrap().text().unwrap(),
        "target"
    );
    h.cx.capture_screenshot(h.window.into())
        .unwrap()
        .save("/tmp/bridge-selection-toolbar.png")
        .unwrap();
    h.apply(json!([{"op":"props","id":1,"component":"selection-panel","props":props(180.,24.,"alpha bravo charlie delta echo change")} ]));
    h.draw();
    assert!(
        h.query(1)["toolbar"].is_null(),
        "changed selected bytes must hide the toolbar"
    );
    h.command(1, json!({"type":"clear"}));
    h.draw();
    assert!(h.query(1)["anchor"].is_null());
    println!(
        "PASS native selection toolbar: current-frame wrapping/font geometry, frame identity, button hit testing, clipboard, stale-source rejection, and clear"
    );
}
