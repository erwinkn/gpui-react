use gpui_react::{
    gpui::{
        self, AnyElement, AppContext, Context, Empty, IntoElement, Render, TestAppContext, Window,
        div, prelude::*,
    },
    protocol::*,
    *,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Deserialize, gpui_react::ComponentProps)]
#[serde(deny_unknown_fields)]
struct Props {
    value: u32,
}
struct View {
    value: u32,
    children: Option<Children>,
    renders: u32,
    changed: Vec<u32>,
}
impl Render for View {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders += 1;
        Empty
    }
}
impl ReactView for View {
    type Props = Props;
    fn create(props: Props, _: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            value: props.value,
            children: None,
            renders: 0,
            changed: vec![],
        }
    }
    fn set_props(&mut self, props: Props, _: &mut Window, cx: &mut Context<Self>) {
        self.value = props.value;
        cx.notify();
    }
}
impl ReactChildren for View {
    fn child_changed(&mut self, child: u32, _: &mut Window, _: &mut Context<Self>) {
        self.changed.push(child);
    }
    fn set_children(&mut self, children: Children, _: &mut Window, cx: &mut Context<Self>) {
        self.children = Some(children);
        cx.notify();
    }
}
impl ReactQueries for View {
    type Query = ();
    type Reply = Value;
    fn query(&mut self, _: (), _: &mut Window, _: &mut Context<Self>) -> anyhow::Result<Value> {
        let children: Vec<_> = self
            .children
            .as_ref()
            .map(|children| children.ids().to_vec())
            .unwrap_or_default();
        Ok(json!({"value": self.value, "children": children}))
    }
}
impl ReactCommands for View {
    type Command = u32;
    fn command(
        &mut self,
        value: u32,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.value = value;
        cx.notify();
        anyhow::bail!("native operation failed after changing state")
    }
}

/// A host-owned element with children, like a div.
struct Boxed(u32);
impl ReactElement for Boxed {
    type Props = Props;
    type Extras = ();
    fn create(props: Props, _: &mut (), _: &mut ElementContext) -> Self {
        Self(props.value)
    }
    fn set_props(&mut self, props: Props, _: &mut (), _: &mut ElementContext) {
        self.0 = props.value;
    }
    fn render(&self, _: &(), cx: &mut RenderContext) -> AnyElement {
        let children = cx.children();
        div().id(cx.element_id()).children(children).into_any_element()
    }
}
impl ElementQueries for Boxed {
    type Query = ();
    type Reply = Value;
    fn query(&mut self, _: (), _: &mut (), cx: &mut ElementContext) -> anyhow::Result<Value> {
        Ok(json!({"value": self.0, "childCount": cx.child_count}))
    }
}

fn registry() -> Registry {
    let mut registry = Registry::default();
    registry
        .register(
            Component::<View>::new("view")
                .children()
                .queries()
                .commands(),
        )
        .unwrap();
    registry
        .register(Component::<View>::new("leaf").queries())
        .unwrap();
    registry
        .register(HostElement::<Boxed>::new("box").children().queries())
        .unwrap();
    registry
}
fn tx(sequence: u64, ops: Value) -> Transaction {
    gpui_react::protocol::Transaction::from_json(&json!({"version":1,"sequence":sequence,"operations":ops})).unwrap()
}
fn create(id: u64) -> Value {
    json!({"op":"create","id":id,"component":"view","props":{"value":id}})
}
fn boxed(id: u64) -> Value {
    json!({"op":"create","id":id,"component":"box","props":{"value":id}})
}
fn place(parent: Option<u64>, child: u64, before: Option<u64>) -> Value {
    json!({"op":"place","parent":parent,"child":child,"before":before})
}
fn query(id: u64, component: &str, request: u64) -> Value {
    json!({"op":"query","id":id,"component":component,"request":request,"value":null})
}
fn view(host: &Host, id: u32) -> gpui::Entity<View> {
    host.view(id).unwrap().downcast::<View>().unwrap()
}

#[gpui::test]
fn invalid_operations_fail_the_transaction(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, _| Host::new(registry(), Arc::new(|_| {})));
    window
        .update(cx, |host, window, cx| {
            // Schema failures are found while decoding, before any native work.
            let invalid = tx(1, json!([create(1), place(None,1,None), {"op":"props","id":1,"component":"view","props":{"value":-1}}]));
            assert!(host.apply(invalid, window, cx).is_err());
            assert!(host.is_empty());
            host.apply(tx(1, json!([create(1), place(None, 1, None)])), window, cx)
                .unwrap();
            assert!(host.apply(tx(1, json!([])), window, cx).is_err(), "sequence must advance");
            assert!(host.apply(tx(2, json!([create(1)])), window, cx).is_err(), "ids must increase");
            assert!(host.apply(tx(3, json!([{"op":"create","id":2,"component":"leaf","props":{"value":2}}, place(Some(2), 1, None)])), window, cx).is_err(), "leaves take no children");
            assert!(host.apply(tx(4, json!([create(3)])), window, cx).is_err(), "created views must be placed");
            assert!(host.apply(tx(5, json!([create(4), place(None, 4, Some(99))])), window, cx).is_err(), "anchors must be siblings");
            assert!(host.apply(tx(6, json!([create(5), place(Some(1), 5, None), place(Some(5), 1, None)])), window, cx).is_err(), "cycles are rejected");
            host.clear(window, cx);
        })
        .unwrap();
}

#[gpui::test]
fn moves_hidden_children_and_removal_preserve_the_live_gpui_views(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, _| Host::new(registry(), Arc::new(|_| {})));
    window
        .update(cx, |host, window, cx| {
            host.apply(
                tx(1, json!([create(1), create(2), create(3), place(Some(1), 2, None), place(Some(1), 3, None), place(None, 1, None)])),
                window,
                cx,
            )
            .unwrap();
            let identity = host.view(2).unwrap().entity_id();
            let reply = host
                .apply(
                    tx(2, json!([
                        place(Some(1),3,Some(2)),
                        query(1, "view", 1),
                        {"op":"hidden","id":3,"hidden":true},
                        query(1, "view", 2),
                        {"op":"hidden","id":3,"hidden":false},
                        {"op":"remove","id":2},
                        query(1, "view", 3)
                    ])),
                    window,
                    cx,
                )
                .unwrap();
            assert_eq!(reply.results[0].value, Some(json!({"value":1,"children":[3,2]})));
            assert_eq!(reply.results[1].value, Some(json!({"value":1,"children":[2]})));
            assert_eq!(reply.results[2].value, Some(json!({"value":1,"children":[3]})));
            assert_ne!(host.view(3).unwrap().entity_id(), identity);
            assert_eq!(host.children(1), Some(vec![3]));
            host.apply(tx(3, json!([{"op":"remove","id":1}])), window, cx)
                .unwrap();
            assert!(host.is_empty());
            assert!(host.roots().is_empty());
        })
        .unwrap();
}

#[gpui::test]
fn invalid_calls_return_errors_without_losing_other_committed_changes(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, _| Host::new(registry(), Arc::new(|_| {})));
    window
        .update(cx, |host, window, cx| {
            let reply = host
                .apply(
                    tx(1, json!([
                        {"op":"create","id":1,"component":"leaf","props":{"value":1}}, place(None,1,None),
                        {"op":"command","id":1,"component":"leaf","request":1,"value":7},
                        query(999, "leaf", 2),
                        query(1, "leaf", 3)
                    ])),
                    window,
                    cx,
                )
                .unwrap();
            assert!(reply.results[0].error.as_ref().unwrap().contains("no commands"));
            assert!(reply.results[1].error.as_ref().unwrap().contains("unknown view"));
            assert_eq!(reply.results[2].value, Some(json!({"value":1,"children":[]})));
            host.clear(window, cx);
        })
        .unwrap();
}

#[gpui::test]
fn moving_a_child_out_before_removal_preserves_it(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, _| Host::new(registry(), Arc::new(|_| {})));
    window
        .update(cx, |host, window, cx| {
            host.apply(
                tx(1, json!([
                    create(1), create(2), create(3), create(4), create(5),
                    place(None, 1, None), place(Some(1), 2, None), place(Some(2), 3, None),
                    place(Some(3), 4, None), place(Some(2), 5, None)
                ])),
                window,
                cx,
            )
            .unwrap();
            let survivor = host.view(4).unwrap().entity_id();
            let reply = host
                .apply(
                    tx(2, json!([place(Some(2), 4, Some(5)), {"op":"remove", "id":3}, query(2, "view", 1)])),
                    window,
                    cx,
                )
                .unwrap();
            assert_eq!(reply.results[0].value, Some(json!({"value":2,"children":[4,5]})));
            assert_eq!(host.children(2), Some(vec![4, 5]));
            assert_eq!(host.view(4).unwrap().entity_id(), survivor);
            assert!(host.view(3).is_none());
            assert_eq!(host.len(), 4);
            host.clear(window, cx);
        })
        .unwrap();
}

#[gpui::test]
fn a_state_query_does_not_rebuild_an_unchanged_native_view(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, _| Host::new(registry(), Arc::new(|_| {})));
    let view = window
        .update(cx, |host, window, cx| {
            host.apply(tx(1, json!([create(1), place(None, 1, None)])), window, cx)
                .unwrap();
            view(host, 1)
        })
        .unwrap();
    let before = view.read_with(cx, |view, _| view.renders);
    assert!(before > 0);
    window
        .update(cx, |host, window, cx| {
            host.apply(tx(2, json!([query(1, "view", 1)])), window, cx)
                .unwrap();
        })
        .unwrap();
    assert_eq!(view.read_with(cx, |view, _| view.renders), before);
    window
        .update(cx, |host, window, cx| host.clear(window, cx))
        .unwrap();
}

#[gpui::test]
fn changes_inside_a_branch_reach_the_nearest_view_that_owns_children(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, _| Host::new(registry(), Arc::new(|_| {})));
    window
        .update(cx, |host, window, cx| {
            // view 1 > box 2 > box 3 > leaf 4, and view 1 > view 5 > leaf 6
            host.apply(
                tx(1, json!([
                    create(1), place(None, 1, None),
                    boxed(2), place(Some(1), 2, None),
                    boxed(3), place(Some(2), 3, None),
                    {"op":"create","id":4,"component":"leaf","props":{"value":4}}, place(Some(3), 4, None),
                    create(5), place(Some(1), 5, None),
                    {"op":"create","id":6,"component":"leaf","props":{"value":6}}, place(Some(5), 6, None)
                ])),
                window,
                cx,
            )
            .unwrap();
            let root = view(host, 1);
            let inner = view(host, 5);
            root.update(cx, |root, _| root.changed.clear());
            inner.update(cx, |inner, _| inner.changed.clear());
            let reply = host
                .apply(
                    tx(2, json!([
                        {"op":"props","id":4,"component":"leaf","props":{"value":40}},
                        {"op":"props","id":6,"component":"leaf","props":{"value":60}},
                        {"op":"command","id":5,"component":"view","request":1,"value":50},
                        query(2, "box", 2)
                    ])),
                    window,
                    cx,
                )
                .unwrap();
            assert!(reply.results[0].error.as_ref().unwrap().contains("after changing state"));
            assert_eq!(reply.results[1].value, Some(json!({"value":2,"childCount":1})));
            // Elements do not cache; the walk passes through them to the view.
            assert_eq!(root.read(cx).changed, vec![2, 5]);
            // The nearest view stops the walk.
            assert_eq!(inner.read(cx).changed, vec![6]);
            host.clear(window, cx);
        })
        .unwrap();
}

#[gpui::test]
fn elements_render_their_children_from_the_host_tree(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, _| Host::new(registry(), Arc::new(|_| {})));
    let leaf = window
        .update(cx, |host, window, cx| {
            host.apply(
                tx(1, json!([
                    boxed(1), place(None, 1, None),
                    boxed(2), place(Some(1), 2, None),
                    create(3), place(Some(2), 3, None)
                ])),
                window,
                cx,
            )
            .unwrap();
            view(host, 3)
        })
        .unwrap();
    let before = leaf.read_with(cx, |leaf, _| leaf.renders);
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    assert_eq!(leaf.read_with(cx, |leaf, _| leaf.renders), before + 1, "a view nested in elements renders once per frame");
    window
        .update(cx, |host, window, cx| {
            host.apply(tx(2, json!([{"op":"hidden","id":2,"hidden":true}])), window, cx)
                .unwrap();
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    assert_eq!(leaf.read_with(cx, |leaf, _| leaf.renders), before + 1, "hidden branches are not rendered");
    window
        .update(cx, |host, window, cx| host.clear(window, cx))
        .unwrap();
}
