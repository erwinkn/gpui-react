mod support;
use serde_json::{Value, json};
use support::*;

fn props(count: Option<usize>, start: usize, follow: bool) -> Value {
    json!({"itemCount":count,"windowStart":start,"estimatedItemHeight":20,"followTail":follow,"style":{"width":320,"height":100}})
}
fn main() {
    let mut h = Harness::new(400., 200.);
    let mut ops = vec![
        create(1, "list", props(Some(100_000), 0, false)),
        place(1, None, None),
        json!({"op":"listen","id":1,"subscription":1}),
    ];
    for i in 0..10 {
        ops.push(row(2 + i, i as usize));
        ops.push(place(2 + i, Some(1), None));
    }
    h.apply(json!(ops));
    h.draw();
    assert_eq!(h.query(1)["anchor"]["index"], 0);
    assert!(h.query(1)["paintedRows"]["end"].as_u64().unwrap() <= 7);
    h.wheel(20., 50., 0., -1_000_000.);
    h.draw();
    assert_eq!(
        h.query(1)["anchor"]["index"],
        50_000,
        "focus ownership must not erase unpainted row estimates"
    );
    h.command(1, json!({"type":"scrollTo","index":0}));
    h.draw();
    h.take_events();
    h.command(1, json!({"type":"scrollTo","index":50_000}));
    h.draw();
    let events = h.take_events();
    assert!(
        events.iter().any(|e| e["type"] == "needRows"
            && e["range"]["start"].as_u64().unwrap() <= 50_000
            && e["range"]["end"].as_u64().unwrap() > 50_000),
        "distant native jump did not request rows: {events:?}"
    );
    let mut ops = vec![json!({"op":"props","id":1,"props":props(Some(100_000),49_998,false)})];
    for id in 2..12 {
        ops.push(json!({"op":"remove","id":id}));
    }
    for i in 0..10 {
        ops.push(row(12 + i, 49_998 + i as usize));
        ops.push(place(12 + i, Some(1), None));
    }
    ops.push(h.command_op(1, json!({"type":"scrollTo","index":50_000,"offset":3})));
    h.apply(json!(ops));
    assert_eq!(h.query(1)["anchor"], json!({"index":50_000,"offset":3.}));
    h.draw();
    let first = h.query(14);
    assert_eq!(first["text"], "row 50000");
    assert_eq!(first["painted"]["bounds"]["y"], -3.);
    let list = h.query(1);
    assert_eq!(list["itemCount"], 100_000);
    assert_eq!(list["supplied"], json!({"start":49_998,"end":50_008}));
    println!("PASS 100,000 logical rows, distant request, and atomic window/anchor");
    h.apply(json!([{"op":"remove","id":1}]));

    // The short-to-overflow transition exposed the original prepend regression.
    let mut ops = vec![
        create(22, "list", props(None, 0, false)),
        place(22, None, None),
    ];
    for i in 0..2 {
        ops.push(row(23 + i, i as usize));
        ops.push(place(23 + i, Some(22), None));
    }
    h.apply(json!(ops));
    h.draw();
    let mut ops = vec![];
    for i in 0..10 {
        ops.push(row(25 + i, 10 + i as usize));
        ops.push(place(25 + i, Some(22), Some(23)));
    }
    h.apply(json!(ops));
    h.draw();
    assert_eq!(h.query(22)["anchor"]["index"], 0);
    assert_eq!(h.query(25)["painted"]["bounds"]["y"], 0.);
    h.command(22, json!({"type":"scrollTo","index":4}));
    h.draw();
    let before = h.query(29)["painted"]["bounds"]["y"].clone();
    h.apply(json!([
        row(35, 30),
        place(35, Some(22), Some(25)),
        row(36, 31),
        place(36, Some(22), Some(25))
    ]));
    h.draw();
    assert_eq!(h.query(22)["anchor"]["index"], 6);
    assert_eq!(h.query(29)["painted"]["bounds"]["y"], before);
    println!("PASS prepend at top, overflow transition, and reader anchor");

    h.apply(json!([{"op":"props","id":22,"props":props(None,0,true)}]));
    h.draw();
    assert!(h.query(22)["followingTail"].as_bool().unwrap());
    h.apply(json!([row(37, 32), place(37, Some(22), None)]));
    h.draw();
    let last = h.query(37);
    assert_eq!(
        last["painted"]["bounds"]["y"].as_f64().unwrap()
            + last["painted"]["bounds"]["height"].as_f64().unwrap(),
        100.
    );
    h.wheel(20., 50., 0., 40.);
    h.draw();
    assert!(!h.query(22)["followingTail"].as_bool().unwrap());
    let before = h.query(22)["anchor"].clone();
    h.apply(json!([row(38, 33), place(38, Some(22), None)]));
    h.draw();
    assert_eq!(h.query(22)["anchor"], before);
    h.command(22, json!({"type":"end"}));
    h.draw();
    assert!(h.query(22)["followingTail"].as_bool().unwrap());
    println!("PASS follow tail, pause on native wheel, append while reading, and resume");

    // Cache the old height, leave the row, then change it in the same commit
    // as a pixel anchor. Offscreen estimates may remain approximate; resolving
    // an explicit negative anchor must measure changed preceding rows.
    h.command(22, json!({"type":"scrollTo","index":0}));
    h.draw();
    h.command(22, json!({"type":"end"}));
    h.draw();
    let anchor = h.command_op(22, json!({"type":"scrollTo","index":1,"offset":-40}));
    h.apply(
        json!([{"op":"props","id":35,"props":{"text":"taller row","style":{"height":100}}},anchor]),
    );
    h.draw();
    assert_eq!(
        h.query(36)["painted"]["bounds"]["y"],
        40.,
        "negative anchor must use the new height of its preceding row"
    );
    println!("PASS changed offscreen row and same-commit negative anchor");
    assert_eq!(h.query(22)["anchor"], json!({"index":0,"offset":60.}));
    h.apply(json!([place(35, Some(22), Some(25))]));
    h.draw();
    assert_eq!(
        h.query(22)["anchor"],
        json!({"index":1,"offset":60.}),
        "a keyed reorder must retain the reader's native row anchor"
    );
    assert_eq!(h.query(35)["painted"]["bounds"]["y"], -60.);
    println!("PASS keyed row reorder preserves the reader anchor");
}
