#[allow(dead_code)] // The shared fixture helper also supports non-document scenarios.
mod support;
use gpui::{prelude::*, *};
use gpui_react::{Component, ReactView};
use gpui_react_kit::document_text;
use serde_json::{Value, json};
use support::*;

// An ordinary native component contributes styled text to the same document.
struct NativeBlock;
impl Render for NativeBlock {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let style = window.text_style();
        div().child(
            document_text("native", "native token")
                .with_runs(vec![style.to_run(7), style.to_run(5)]),
        )
    }
}
impl ReactView for NativeBlock {
    type Props = ();
    fn create(_: (), _: &mut Window, _: &mut Context<Self>) -> Self {
        Self
    }
    fn set_props(&mut self, _: (), _: &mut Window, _: &mut Context<Self>) {}
}
fn props(query: &str, active: Option<usize>) -> Value {
    json!({"style":{"width":240,"fontSize":16,"lineHeight":24,"color":"white","background":"#202020"},"search":{"query":query,"activeIndex":active,"color":"#ffff00","activeColor":"#ff8000"}})
}
fn text(id: u64, key: &str, value: &str) -> Value {
    create(id, "text", json!({"text":value,"textKey":key}))
}
fn key(h: &mut Harness, key: &str) {
    h.cx.simulate_keystrokes(h.window.into(), key);
    h.draw();
}
fn main() {
    let mut h = Harness::with_components(400., 260., |registry| {
        registry
            .register(Component::<NativeBlock>::new("native-block"))
            .unwrap()
    });
    h.apply(json!([
        create(1,"document",props("token",Some(0))),place(1,None,None),
        {"op":"listen","id":1,"subscription":1},
        text(2,"a","ab😀 token"),place(2,Some(1),None),
        create(3,"native-block",Value::Null),place(3,Some(1),None),
        create(4,"text",json!({"text":"chrome token","textKey":"chrome","selectable":false})),place(4,Some(1),None),
        text(5,"z","final token"),place(5,Some(1),None)
    ]));
    h.draw();
    let snapshot = h.query(1);
    assert_eq!(
        snapshot["text"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["key"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["a", "native", "chrome", "z"]
    );
    assert_eq!(snapshot["matchCount"], 4);
    assert_eq!(snapshot["highlights"][0]["active"], true);
    assert_eq!(
        snapshot["highlights"][0]["start"], 5,
        "search reports UTF-16 offsets"
    );
    assert_eq!(snapshot["highlights"][1]["key"], "native");
    let image = h.cx.capture_screenshot(h.window.into()).unwrap();
    assert!(
        image
            .pixels()
            .filter(|p| p[0] > 200 && p[1] > 200 && p[2] < 60)
            .count()
            > 100,
        "search washes must reach GPU pixels"
    );
    image.save("/tmp/bridge-document.png").unwrap();
    let revision = snapshot["contentRevision"].as_u64().unwrap();
    h.command(1,json!({"type":"select","start":{"key":"a","offset":2},"end":{"key":"a","offset":4},"expectedContentRevision":revision}));
    h.draw();
    let selected = h.query(1);
    assert_eq!(selected["selection"], "😀");
    assert_eq!(selected["ranges"][0]["start"], 2);
    assert_eq!(selected["ranges"][0]["end"], 4);
    assert!(selected["ranges"][0]["rects"][0]["width"].as_f64().unwrap() > 5.);
    key(&mut h, "cmd-c");
    assert_eq!(h.cx.read_from_clipboard().unwrap().text().unwrap(), "😀");
    let area = &selected["ranges"][0]["rects"][0];
    let position = point(
        px((area["x"].as_f64().unwrap() + area["width"].as_f64().unwrap() * 0.8) as f32),
        px((area["y"].as_f64().unwrap() + 12.) as f32),
    );
    h.command(1, json!({"type":"clear"}));
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
    assert_eq!(
        h.query(1)["selection"],
        "😀",
        "double click on the right half of a glyph must select that glyph"
    );
    key(&mut h, "cmd-a");
    assert_eq!(
        h.query(1)["selection"],
        "ab😀 token\nnative token\nfinal token"
    );
    println!(
        "PASS native and React text order, search, UTF-16 selection, clipboard, and chrome exclusion"
    );

    h.command(1, json!({"type":"clear"}));
    h.draw();
    h.take_events();
    h.apply(json!([{"op":"props","id":1,"component":"document","props":props("token",Some(2))}]));
    h.draw();
    assert_eq!(h.query(1)["contentRevision"], revision);
    assert!(
        h.take_events()
            .iter()
            .all(|event| event["type"] != "search"),
        "active match changes must not report new search results"
    );
    h.apply(json!([{"op":"props","id":1,"component":"document","props":props("native",Some(0))}]));
    h.draw();
    assert_eq!(h.query(1)["matchCount"], 1);
    assert!(
        h.take_events()
            .iter()
            .any(|event| event["type"] == "search")
    );
    h.apply(json!([{"op":"props","id":1,"component":"document","props":props("final",Some(0))}]));
    h.draw();
    assert_eq!(h.query(1)["matchCount"], 1);
    assert!(
        h.take_events()
            .iter()
            .any(|event| event["type"] == "search"),
        "same-count query change must report"
    );
    println!("PASS search events distinguish query changes from active cursor changes");

    // Native pointer capture keeps the drag alive outside the text hitbox.
    h.cx.simulate_mouse_move(
        h.window.into(),
        point(px(1.), px(12.)),
        None,
        Modifiers::default(),
    );
    h.draw();
    h.cx.simulate_mouse_down(
        h.window.into(),
        point(px(1.), px(12.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    h.cx.simulate_mouse_move(
        h.window.into(),
        point(px(360.), px(94.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    h.draw();
    h.cx.simulate_mouse_up(
        h.window.into(),
        point(px(360.), px(94.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    h.draw();
    assert_eq!(
        h.query(1)["selection"],
        "ab😀 token\nnative token\nfinal token"
    );
    h.cx.update_window(h.window.into(), |_, window, _| {
        assert!(window.captured_hitbox().is_none())
    })
    .unwrap();
    println!("PASS drag across native components and pointer release outside document bounds");
    h.apply(json!([
        {"op":"props","id":4,"component":"text","props":{"text":"chrome token","textKey":"chrome","selectable":false,"searchable":false}},
        {"op":"props","id":1,"component":"document","props":props("token",None)}
    ]));
    h.draw();
    assert_eq!(
        h.query(1)["matchCount"],
        3,
        "chrome can opt out of both selection and search"
    );

    // Nested scopes own independent selections and paint registries.
    h.apply(json!([
        create(6, "document", props("inner", None)),
        place(6, Some(1), Some(5)),
        text(7, "inner", "inner only"),
        place(7, Some(6), None)
    ]));
    h.draw();
    assert!(
        !h.query(1)["text"]
            .as_array()
            .unwrap()
            .iter()
            .any(|text| text["key"] == "inner")
    );
    assert_eq!(h.query(6)["text"][0]["text"], "inner only");
    h.command(6, json!({"type":"selectAll"}));
    h.draw();
    key(&mut h, "cmd-c");
    assert_eq!(
        h.cx.read_from_clipboard().unwrap().text().unwrap(),
        "inner only"
    );
    println!("PASS nested document isolation and focused clipboard ownership");

    // Wrapped geometry covers every continuation row, without a fixed row cap.
    h.apply(json!([{"op":"remove","id":1},create(8,"document",json!({"style":{"width":96,"fontSize":16,"lineHeight":24}})),place(8,None,None),text(9,"wrapped","one two three four five six seven eight nine ten"),place(9,Some(8),None)]));
    h.draw();
    h.command(8, json!({"type":"selectAll"}));
    h.draw();
    let wrapped = h.query(8);
    assert!(wrapped["ranges"][0]["rects"].as_array().unwrap().len() >= 3);
    let before = wrapped["ranges"][0]["rects"].clone();
    h.apply(json!([{"op":"props","id":8,"component":"document","props":{"style":{"width":220,"fontSize":20,"lineHeight":30}}}]));
    h.draw();
    let after = h.query(8);
    assert_eq!(after["selection"], wrapped["selection"]);
    assert_ne!(after["ranges"][0]["rects"], before);
    assert_eq!(after["contentRevision"], wrapped["contentRevision"]);
    assert!(
        after["frame"]["frame"].as_u64().unwrap() > wrapped["frame"]["frame"].as_u64().unwrap()
    );
    println!("PASS wrapped selection geometry follows native font and width changes");

    // Old native selection text remains available after virtualization removes it.
    h.apply(json!([{"op":"remove","id":9},text(10,"later","later page"),place(10,Some(8),None)]));
    h.draw();
    assert_eq!(h.query(8)["selection"], wrapped["selection"]);
    assert_eq!(h.query(8)["ranges"], json!([]));
    let stale=h.command_op(8,json!({"type":"select","start":{"key":"later","offset":0},"end":{"key":"later","offset":5},"expectedContentRevision":wrapped["contentRevision"]}));
    assert!(
        h.apply(json!([stale])).results[0]
            .error
            .as_ref()
            .unwrap()
            .contains("stale document")
    );
    println!("PASS selection snapshot survives unmount and stale selection commands reject");
    h.apply(json!([{"op":"remove","id":8}]));
    let mut ops = vec![
        create(11, "document", props("token", Some(20))),
        place(11, None, None),
        create(
            12,
            "list",
            json!({"estimatedItemHeight":20,"style":{"width":240,"height":80}}),
        ),
        place(12, Some(11), None),
    ];
    for i in 0..30 {
        ops.push(create(13+i,"text",json!({"text":format!("token {i}"),"textKey":format!("row-{i}"),"matchIndexOffset":i,"style":{"height":20,"lineHeight":20}})));
        ops.push(place(13 + i, Some(12), None));
    }
    h.apply(json!(ops));
    h.draw();
    h.wheel(20., 40., 0., -400.);
    h.draw();
    let document = h.query(11);
    assert!(
        document["highlights"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["key"] == "row-20"
                && entry["index"] == 20
                && entry["active"] == true),
        "native scrolling must retain global match indices without a React round trip: {document}"
    );
    println!("PASS virtualized global search cursor moves with native scrolling");
    h.apply(json!([
        create(63,"container",json!({"style":{"height":24}})),place(63,Some(11),None),
        {"op":"listen","id":63,"subscription":2},text(64,"button","clickable label"),place(64,Some(63),None)
    ]));
    h.draw();
    h.take_events();
    h.cx.simulate_click(
        h.window.into(),
        point(px(20.), px(92.)),
        Modifiers::default(),
    );
    h.draw();
    assert!(
        h.take_events().iter().any(|event| event["type"] == "click"),
        "a document must not consume a normal label click"
    );
    h.apply(json!([{"op":"remove","id":63}]));
    h.draw();
    h.command(12, json!({"type":"scrollTo","index":0}));
    h.draw();
    h.cx.simulate_mouse_move(
        h.window.into(),
        point(px(1.), px(10.)),
        None,
        Modifiers::default(),
    );
    h.draw();
    h.cx.simulate_mouse_down(
        h.window.into(),
        point(px(1.), px(10.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    h.cx.simulate_mouse_move(
        h.window.into(),
        point(px(200.), px(110.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    h.draw();
    for _ in 0..20 {
        h.cx.advance_clock(std::time::Duration::from_millis(20));
        h.draw();
    }
    assert!(
        h.query(12)["anchor"]["index"].as_u64().unwrap() >= 4,
        "native selection must autoscroll its viewport"
    );
    let selection = h.query(11)["selection"].as_str().unwrap().to_string();
    assert!(selection.starts_with("token 0\ntoken 1\n"));
    assert!(
        selection.contains("token 7"),
        "selection must extend after its anchor leaves the viewport: {selection}"
    );
    h.cx.simulate_mouse_up(
        h.window.into(),
        point(px(200.), px(110.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    h.draw();
    let anchor = h.query(12)["anchor"].clone();
    h.cx.advance_clock(std::time::Duration::from_secs(1));
    h.draw();
    assert_eq!(
        h.query(12)["anchor"],
        anchor,
        "release must cancel autoscroll"
    );
    println!(
        "PASS native drag autoscroll, virtualized selection continuity, and release cancellation"
    );
    h.apply(json!([
        {"op":"remove","id":11},
        create(65,"document",props("row",None)),place(65,None,None),
        create(66,"container",json!({"scroll":"y","style":{"height":80,"width":240}})),place(66,Some(65),None),
        create(67,"container",json!({"scroll":"y","style":{"height":60,"shrink":0}})),place(67,Some(66),None),
        create(68,"text",json!({"text":"row 0\nrow 1\nrow 2\nrow 3\nrow 4","textKey":"inner-rows","style":{"lineHeight":20,"shrink":0}})),place(68,Some(67),None),
        create(69,"text",json!({"text":"outer tail","style":{"height":200,"shrink":0}})),place(69,Some(66),None),
        create(70,"container",json!({"scroll":"y","style":{"height":40,"width":240}})),place(70,Some(65),None),
        create(71,"text",json!({"text":"unrelated sibling","style":{"height":200,"shrink":0}})),place(71,Some(70),None)
    ]));
    h.draw();
    h.cx.simulate_mouse_move(
        h.window.into(),
        point(px(1.), px(10.)),
        None,
        Modifiers::default(),
    );
    h.draw();
    h.cx.simulate_mouse_down(
        h.window.into(),
        point(px(1.), px(10.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    h.cx.simulate_mouse_move(
        h.window.into(),
        point(px(160.), px(150.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    h.draw();
    h.cx.advance_clock(std::time::Duration::from_millis(20));
    h.draw();
    assert!(
        h.query(67)["offset"]["y"].as_f64().unwrap() > 0.,
        "the inner container must scroll first"
    );
    assert_eq!(h.query(66)["offset"]["y"], 0.);
    for _ in 0..30 {
        h.cx.advance_clock(std::time::Duration::from_millis(20));
        h.draw();
    }
    assert!(
        h.query(66)["offset"]["y"].as_f64().unwrap() > 0.,
        "autoscroll must reach the parent at the inner boundary"
    );
    h.cx.simulate_mouse_up(
        h.window.into(),
        point(px(160.), px(150.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    h.draw();
    assert_eq!(
        h.query(70)["offset"]["y"],
        0.,
        "a selection drag must not scroll an unrelated sibling"
    );
    println!(
        "PASS nested container selection autoscroll, boundary chaining, and sibling isolation"
    );
}
