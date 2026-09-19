mod support;
use gpui::{Modifiers, point, px};
use serde_json::json;
use support::*;

fn main() {
    let mut h = Harness::new(400., 300.);
    h.apply(json!([
        create(1,"container",json!({"focusable":true,"style":{"width":120,"height":100,"padding":10,"borderWidth":2,"borderColor":"red","background":"#202020"}})),place(1,None,None),
        {"op":"listen","id":1,"subscription":1},
        row(2,0),place(2,Some(1),None)
    ]));
    h.draw();
    let parent = h.query(1);
    let child = h.query(2);
    assert_eq!(parent["painted"]["bounds"]["width"], 120.);
    assert_eq!(parent["painted"]["bounds"]["height"], 100.);
    assert_eq!(child["painted"]["bounds"]["x"], 12.);
    assert_eq!(child["painted"]["bounds"]["y"], 12.);
    h.cx.simulate_mouse_move(
        h.window.into(),
        point(px(20.), px(20.)),
        None,
        Modifiers::default(),
    );
    h.draw();
    h.cx.simulate_click(
        h.window.into(),
        point(px(20.), px(20.)),
        Modifiers::default(),
    );
    h.cx.run_until_parked();
    assert!(
        h.take_events().iter().any(|e| e["type"] == "click"),
        "text content must not swallow its container's click"
    );
    h.command(1, json!({"type":"focus"}));
    h.draw();
    assert_eq!(h.query(1)["focused"], true);
    let old = h.query(2);
    assert!(old["painted"]["frame"]["commit"].as_u64().unwrap() > 0);
    let mut query = h.command_op(2, json!(null));
    query["op"] = json!("query");
    let reply=h.apply(json!([
        {"op":"props","id":1,"props":{"focusable":true,"style":{"width":120,"height":100,"padding":20,"borderWidth":2,"borderColor":"red"}}},
        query
    ]));
    let unpainted = reply.results[0].value.as_ref().unwrap();
    assert_eq!(
        unpainted["painted"], old["painted"],
        "a query must retain the previous frame tag until paint"
    );
    h.draw();
    let latest = h.query(2);
    assert_eq!(latest["painted"]["bounds"]["x"], 22.);
    assert_eq!(
        latest["revision"], old["revision"],
        "inherited layout can change without changing the leaf's own props"
    );
    assert!(
        latest["painted"]["frame"]["commit"].as_u64().unwrap()
            > old["painted"]["frame"]["commit"].as_u64().unwrap()
    );
    println!(
        "PASS outer bounds, padding/border, click through text, focus, and frame-tagged geometry"
    );
    h.apply(json!([{"op":"remove","id":1}]));
    h.apply(json!([
        create(3,"container",json!({"scroll":"y","style":{"width":200,"height":160}})),place(3,None,None),{"op":"listen","id":3,"subscription":2},
        create(4,"container",json!({"scroll":"x","style":{"width":200,"height":60,"shrink":0}})),place(4,Some(3),None),{"op":"listen","id":4,"subscription":3},
        create(5,"container",json!({"style":{"width":1000,"height":40,"shrink":0,"background":"#303030"}})),place(5,Some(4),None),
        create(6,"container",json!({"style":{"width":100,"height":1000,"shrink":0}})),place(6,Some(3),None)
    ]));
    h.draw();
    let event = h.wheel(50., 30., 0., -20.);
    h.draw();
    assert!(event.propagate && event.default_prevented);
    assert_eq!(
        h.query(4)["offset"]["x"],
        0.,
        "vertical wheel must not become horizontal scrolling"
    );
    assert_eq!(h.query(3)["offset"]["y"], 20.);
    assert_eq!(
        h.take_events()
            .iter()
            .filter(|e| e["type"] == "wheel")
            .count(),
        2
    );
    h.command(3, json!({"type":"scrollTo","x":0,"y":0}));
    h.draw();
    h.wheel(50., 30., -30., 0.);
    h.draw();
    assert_eq!(h.query(4)["offset"]["x"], 30.);
    assert_eq!(h.query(3)["offset"]["y"], 0.);
    println!("PASS nested axes, native consumption, and both wheel callbacks");
    h.apply(json!([{"op":"remove","id":3}]));
    h.apply(json!([
        create(
            7,
            "container",
            json!({"scroll":"both","style":{"width":100,"height":100}})
        ),
        place(7, None, None),
        create(
            8,
            "container",
            json!({"style":{"width":500,"height":500,"shrink":0,"background":"#303030"}})
        ),
        place(8, Some(7), None)
    ]));
    h.draw();
    h.wheel(20., 20., -30., -20.);
    h.draw();
    assert_eq!(h.query(7)["offset"], json!({"x":30.,"y":20.}));
    println!("PASS two-axis diagonal scroll");
    h.apply(json!([{"op":"remove","id":7}]));
    h.apply(json!([
        create(9,"container",json!({"style":{"width":200,"height":180}})),place(9,None,None),
        create(10,"container",json!({"scroll":"x","scrollGroup":"table","style":{"width":200,"height":30,"shrink":0}})),place(10,Some(9),None),
        create(11,"container",json!({"style":{"width":800,"height":20,"shrink":0}})),place(11,Some(10),None),
        create(12,"container",json!({"scroll":"x","scrollGroup":"table","style":{"width":200,"height":100,"shrink":0}})),place(12,Some(9),None),
        create(13,"container",json!({"style":{"width":800,"height":80,"shrink":0}})),place(13,Some(12),None),
        create(14,"container",json!({"scroll":"x","scrollGroup":"other","style":{"width":200,"height":30,"shrink":0}})),place(14,Some(9),None),
        create(15,"container",json!({"style":{"width":800,"height":20,"shrink":0}})),place(15,Some(14),None)
    ]));
    h.draw();
    for _ in 0..72 {
        h.wheel(20., 15., -3.25, 0.);
        h.draw();
        assert_eq!(
            h.query(11)["painted"]["bounds"]["x"],
            h.query(13)["painted"]["bounds"]["x"],
            "linked header and body must agree within each native frame"
        );
        assert_eq!(h.query(15)["painted"]["bounds"]["x"], 0.);
    }
    assert_eq!(h.query(10)["offset"]["x"], 234.);
    h.apply(json!([{"op":"props","id":10,"props":{"scroll":"x","style":{"width":200,"height":30,"shrink":0}}}]));
    h.draw();
    assert_eq!(h.query(10)["offset"]["x"], 0.);
    assert_eq!(h.query(12)["offset"]["x"], 234.);
    println!("PASS 72 shared-scroll frames, fractional deltas, group isolation, and detach");
    h.apply(json!([{"op":"remove","id":9}]));
    h.apply(json!([
        create(16,"container",json!({"style":{"width":200,"height":80,"background":"#303030"}})),place(16,None,None),{"op":"listen","id":16,"subscription":4},
        create(17,"container",json!({"style":{"width":150,"height":40}})),place(17,Some(16),None),
        row(18,0),place(18,Some(17),None)
    ]));
    h.draw();
    h.take_events();
    h.cx.simulate_mouse_move(
        h.window.into(),
        point(px(20.), px(10.)),
        None,
        Modifiers::default(),
    );
    h.draw();
    h.cx.simulate_click(
        h.window.into(),
        point(px(20.), px(10.)),
        Modifiers::default(),
    );
    h.cx.run_until_parked();
    assert!(
        h.take_events().iter().any(|e| e["type"] == "click"),
        "a layout container must not swallow its parent's click"
    );
    h.apply(json!([{"op":"props","id":17,"props":{"blockMouse":true,"style":{"width":150,"height":40}}}]));
    h.draw();
    h.take_events();
    h.cx.simulate_click(
        h.window.into(),
        point(px(20.), px(10.)),
        Modifiers::default(),
    );
    h.cx.run_until_parked();
    assert!(
        !h.take_events().iter().any(|e| e["type"] == "click"),
        "explicit mouse blocking must isolate the parent"
    );
    println!("PASS click through nested layout containers and explicit mouse blocking");
    h.apply(json!([{"op":"hidden","id":16,"hidden":true}]));
    h.draw();
    h.take_events();
    h.cx.simulate_click(
        h.window.into(),
        point(px(180.), px(60.)),
        Modifiers::default(),
    );
    h.cx.run_until_parked();
    assert!(
        h.take_events().is_empty(),
        "hidden native views must leave no active input region"
    );
    h.apply(json!([{"op":"hidden","id":16,"hidden":false}]));
    h.draw();
    h.cx.simulate_mouse_move(
        h.window.into(),
        point(px(180.), px(60.)),
        None,
        Modifiers::default(),
    );
    h.draw();
    h.cx.simulate_click(
        h.window.into(),
        point(px(180.), px(60.)),
        Modifiers::default(),
    );
    h.cx.run_until_parked();
    assert!(h.take_events().iter().any(|e| e["type"] == "click"));
    println!("PASS hidden subtree input removal and restored native identity");
}
