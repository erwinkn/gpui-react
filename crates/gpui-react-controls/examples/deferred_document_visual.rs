#[allow(dead_code)]
mod support;
use gpui::{prelude::*, *};
use gpui_react::{Component, ReactChildren, ReactQueries, ReactView};
use gpui_react_controls::{Document, Text, TextProps, document::DocumentSnapshot, document_text};
use serde_json::{Value, json};
use support::*;

struct FloatingText {
    inner: Entity<Document>,
}
impl Render for FloatingText {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        deferred(
            anchored().position(point(px(40.), px(80.))).child(
                div()
                    .w(px(180.))
                    .bg(rgb(0x202020))
                    .child(document_text("floating", "floating token"))
                    .child(
                        deferred(
                            anchored()
                                .position(point(px(40.), px(130.)))
                                .child(document_text("nested", "nested token")),
                        )
                        .with_priority(3),
                    )
                    .child(
                        deferred(
                            anchored()
                                .position(point(px(230.), px(180.)))
                                .child(self.inner.clone()),
                        )
                        .with_priority(2),
                    ),
            ),
        )
        .with_priority(1)
    }
}
impl ReactView for FloatingText {
    type Props = ();
    fn create(_: (), window: &mut Window, cx: &mut Context<Self>) -> Self {
        let inner = cx.new(|cx| {
            Document::new(
                serde_json::from_value(json!({
                    "style":{"width":160,"height":28},"search":{"query":"token"}
                }))
                .unwrap(),
                cx,
            )
        });
        let text = cx.new(|_| {
            Text::new(TextProps {
                text: "inner token".into(),
                text_key: Some("inner".into()),
                ..Default::default()
            })
        });
        inner.update(cx, |doc, cx| {
            doc.set_children(vec![text.into()], window, cx)
        });
        Self { inner }
    }
    fn set_props(&mut self, _: (), _: &mut Window, _: &mut Context<Self>) {}
    fn unmounting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.inner.update(cx, |doc, cx| doc.unmounting(window, cx));
    }
}
impl ReactQueries for FloatingText {
    type Query = ();
    type Reply = DocumentSnapshot;
    fn query(
        &mut self,
        _: (),
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<Self::Reply> {
        Ok(self.inner.read(cx).snapshot())
    }
}

fn main() {
    let mut h = Harness::with_components(400., 260., |registry| {
        registry
            .register(Component::<FloatingText>::new("floating").queries())
            .unwrap();
    });
    h.apply(json!([
        create(1,"document",json!({"style":{"width":120,"height":28,"fontSize":16,"lineHeight":24,"color":"white"},"search":{"query":"token"}})),
        place(1,None,None), {"op":"listen","id":1,"subscription":1},
        create(2,"text",json!({"text":"normal token","textKey":"normal"})),place(2,Some(1),None),
        create(3,"floating",Value::Null),place(3,Some(1),None)
    ]));
    h.draw();
    let snapshot = h.query(1);
    let keys: Vec<_> = snapshot["text"]
        .as_array()
        .unwrap()
        .iter()
        .map(|text| text["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["normal", "floating", "nested"]);
    assert_eq!(snapshot["matchCount"], 3);
    assert!(
        h.take_events()
            .iter()
            .any(|event| event["type"] == "search" && event["count"] == 3)
    );
    let inner = h.query(3);
    assert_eq!(inner["matchCount"], 1);
    assert_eq!(inner["text"][0]["key"], "inner");
    assert_eq!(inner["frame"]["root"], snapshot["frame"]["root"]);
    assert_eq!(inner["frame"]["frame"], snapshot["frame"]["frame"]);
    let bounds = &snapshot["text"][1]["bounds"];
    let position = point(
        px(bounds["x"].as_f64().unwrap() as f32 + 8.),
        px(bounds["y"].as_f64().unwrap() as f32 + 12.),
    );
    assert!(
        position.y > px(28.),
        "the floating text must escape its document's layout box"
    );
    h.cx.simulate_mouse_move(h.window.into(), position, None, Modifiers::default());
    h.draw();
    h.cx.update_window(h.window.into(), |_, window, cx| {
        window.dispatch_event(
            MouseDownEvent {
                position,
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
        position,
        MouseButton::Left,
        Modifiers::default(),
    );
    h.draw();
    let selected = h.query(1);
    assert_eq!(selected["selection"], "floating");
    assert_eq!(selected["ranges"][0]["key"], "floating");
    assert_eq!(h.query(3)["selection"], Value::Null);
    h.command(1, json!({"type":"copy"}));
    assert_eq!(
        h.cx.read_from_clipboard().unwrap().text().unwrap(),
        "floating"
    );
    let image = h.cx.capture_screenshot(h.window.into()).unwrap();
    assert!(
        image
            .pixels()
            .filter(|pixel| i16::from(pixel[2]) > i16::from(pixel[0]) + 30)
            .count()
            > 100,
        "native selection must reach the GPU"
    );
    image.save("/tmp/bridge-deferred-document.png").unwrap();
    h.apply(json!([{"op":"remove","id":3}]));
    h.draw();
    let removed = h.query(1);
    assert_eq!(removed["text"].as_array().unwrap().len(), 1);
    assert_eq!(removed["matchCount"], 1);
    assert!(removed["ranges"].as_array().unwrap().is_empty());
    assert!(
        removed["contentRevision"].as_u64().unwrap()
            > snapshot["contentRevision"].as_u64().unwrap()
    );
    println!(
        "PASS deferred text paint, nested document isolation, frame tags, native hit testing outside parent bounds, selection pixels, clipboard, and removal"
    );
}
