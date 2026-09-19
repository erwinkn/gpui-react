#[allow(dead_code)]
#[path = "../../../crates/gpui-react-controls/examples/support/mod.rs"]
mod support;
use gpui::{Entity, Modifiers, point, px};
use gpui_react_texture_example::TextureView;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use support::*;

fn props(width: f32, height: f32) -> Value {
    json!({"initialColor":[2,0,0,0.25],"radius":8,"style":{"width":width,"height":height,"shrink":0,"color":"white"}})
}
fn texture(h: &mut Harness, id: u64) -> Entity<TextureView> {
    h.window
        .update(&mut h.cx, |host, _, _| {
            host.view(id)
                .unwrap()
                .clone()
                .downcast::<TextureView>()
                .unwrap()
        })
        .unwrap()
}
fn pixel(h: &mut Harness, x: f32, y: f32, expected: [u8; 4]) {
    let image = h.cx.capture_screenshot(h.window.into()).unwrap();
    let scale = image.width() as f32 / 128.;
    let actual = image
        .get_pixel(((x + 0.5) * scale) as u32, ((y + 0.5) * scale) as u32)
        .0;
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
        "pixel {x},{y}: {actual:?}, expected {expected:?}"
    );
}
fn tick(h: &mut Harness, ms: u64) {
    h.cx.advance_clock(Duration::from_millis(ms));
    h.cx.update_window(h.window.into(), |_, window, cx| {
        window.simulate_next_frame(cx);
    })
    .unwrap();
    h.draw();
}
fn main() {
    let mut h = Harness::with_components(128., 96., |r| {
        gpui_react_texture_example::register(r).unwrap()
    });
    h.apply(json!([
        create(1,"document",json!({"style":{"width":128,"height":96,"direction":"row","padding":8,"gap":24,"background":"blue"}})),place(1,None,None),
        create(2,"container",json!({"scroll":"both","style":{"width":48,"height":48,"shrink":0,"opacity":0.25}})),place(2,Some(1),None),
        create(3,"example-texture",props(64.,48.)),place(3,Some(2),None),{"op":"listen","id":3,"subscription":1},
        create(4,"example-texture",json!({"initialColor":[0,1,0,1],"style":{"width":32,"height":32,"shrink":0}})),place(4,Some(1),None)
    ]));
    h.draw();
    let first = h.query(3);
    assert!(first["error"].is_null(), "{first}");
    assert_eq!(
        first["painted"]["bounds"],
        json!({"x":8.,"y":8.,"width":64.,"height":48.})
    );
    assert!(first["painted"]["frame"]["root"].is_string());
    pixel(&mut h, 24., 40., [128, 0, 239, 255]);
    pixel(&mut h, 64., 40., [0, 0, 255, 255]);
    pixel(&mut h, 8., 8., [0, 0, 255, 255]);
    pixel(&mut h, 96., 24., [0, 255, 0, 255]);
    h.cx.capture_screenshot(h.window.into())
        .unwrap()
        .save("/tmp/bridge-gpu-initial.png")
        .unwrap();
    println!(
        "PASS shared queue, float RGB above alpha, inherited opacity, clip, corners, and independent textures"
    );
    h.cx.simulate_click(
        h.window.into(),
        point(px(24.), px(40.)),
        Modifiers::default(),
    );
    h.draw();
    assert!(h.take_events().iter().any(|e| e["type"] == "click"));
    let mut label = props(64., 48.);
    label["label"] = json!("external text");
    h.apply(json!([{"op":"props","id":3,"props":label}]));
    h.draw();
    assert!(
        h.query(1)["text"]
            .as_array()
            .unwrap()
            .iter()
            .any(|text| text["text"] == "external text")
    );
    assert_eq!(
        h.query(3)["painted"]["generation"],
        first["painted"]["generation"],
        "label changes must reuse the texture"
    );
    println!("PASS native click and document text through an external ordinary view");
    let entity = texture(&mut h, 3);
    let old = entity.read_with(&h.cx, |view, _| Arc::downgrade(view.texture().unwrap()));
    let old_frame =
        h.cx.update_window(h.window.into(), |_, window, _| window.painted_surfaces())
            .unwrap();
    h.command(
        3,
        json!({"type":"transition","to":[0.5,0,0,0.5],"durationMs":0}),
    );
    h.apply(json!([{"op":"props","id":3,"props":props(32.,32.)}]));
    h.draw();
    assert!(
        old.upgrade().is_some(),
        "the previous recorded frame must retain its texture"
    );
    pixel(&mut h, 24., 24., [32, 0, 223, 255]);
    pixel(&mut h, 48., 40., [0, 0, 255, 255]);
    drop(old_frame);
    h.draw();
    h.draw();
    assert!(
        old.upgrade().is_none(),
        "retired frames must release old texture handles"
    );
    assert_eq!(
        h.query(3)["color"],
        json!([0.5, 0., 0., 0.5]),
        "initialColor is construction-only"
    );
    println!("PASS resize replaces the resource while prior frames retain their pixels");

    h.command(
        3,
        json!({"type":"transition","to":[0,1,0,1],"durationMs":1000}),
    );
    h.draw();
    tick(&mut h, 250);
    let midway = h.query(3);
    let green = midway["painted"]["color"][1].as_f64().unwrap();
    assert!(
        (0.2..0.4).contains(&green),
        "native animation did not advance: {midway}"
    );
    let before = midway["color"].clone();
    h.command(
        3,
        json!({"type":"transition","to":[0,0,1,1],"durationMs":1000}),
    );
    assert_eq!(
        h.query(3)["color"],
        before,
        "retarget starts at the current native color"
    );
    tick(&mut h, 250);
    h.command(3, json!({"type":"cancel"}));
    let stopped = h.query(3)["color"].clone();
    tick(&mut h, 2000);
    assert_eq!(h.query(3)["color"], stopped);
    let mut reduced = props(32., 32.);
    reduced["reducedMotion"] = json!(true);
    h.apply(json!([{"op":"props","id":3,"props":reduced}]));
    h.command(
        3,
        json!({"type":"transition","to":[1,0,0,1],"durationMs":5000}),
    );
    h.draw();
    assert_eq!(h.query(3)["color"], json!([1., 0., 0., 1.]));
    assert_eq!(h.query(3)["animating"], false);
    tick(&mut h, 16);
    let pending =
        h.cx.update_window(h.window.into(), |_, window, cx| {
            window.simulate_next_frame(cx)
        })
        .unwrap();
    assert_eq!(
        pending, 0,
        "reduced motion must not schedule idle animation frames"
    );
    println!(
        "PASS native animation, retarget, cancellation, reduced motion, and idle frame requests"
    );

    h.apply(json!([{"op":"props","id":3,"props":props(32.,32.)}]));
    h.command(
        3,
        json!({"type":"transition","to":[0,0,1,1],"durationMs":5000}),
    );
    h.cx.update(|cx| cx.set_reduce_motion(true));
    h.draw();
    assert_eq!(
        h.query(3)["animating"],
        false,
        "GPUI's reduced-motion setting must stop decorative motion"
    );
    assert_eq!(h.query(3)["color"], json!([0., 0., 1., 1.]));
    h.cx.update(|cx| cx.set_reduce_motion(false));
    println!("PASS GPUI reduced-motion preference applies during an active transition");

    let last = entity.read_with(&h.cx, |view, _| Arc::downgrade(view.texture().unwrap()));
    drop(entity);
    let last_frame =
        h.cx.update_window(h.window.into(), |_, window, _| window.painted_surfaces())
            .unwrap();
    h.apply(json!([{"op":"remove","id":3},{"op":"remove","id":4}]));
    assert!(
        last.upgrade().is_some(),
        "a recorded frame must outlive the removed component"
    );
    h.draw();
    pixel(&mut h, 24., 24., [0, 0, 255, 255]);
    pixel(&mut h, 96., 24., [0, 0, 255, 255]);
    drop(last_frame);
    h.draw();
    h.draw();
    assert!(last.upgrade().is_none());
    println!("PASS removal releases native ownership and retires frame resources");
}
