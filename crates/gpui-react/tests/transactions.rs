use gpui_react::{
    gpui::{self, AnyView, Context, Empty, IntoElement, Render, TestAppContext, Window},
    protocol::*,
    *,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Props {
    value: u32,
}
struct View {
    value: u32,
    children: Vec<AnyView>,
    renders: u32,
    changed: Vec<Vec<gpui::EntityId>>,
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
            children: vec![],
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
    fn children_changed(
        &mut self,
        children: &[gpui::EntityId],
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.changed.push(children.to_vec());
    }
    fn set_children(&mut self, children: Vec<AnyView>, _: &mut Window, cx: &mut Context<Self>) {
        self.children = children;
        cx.notify();
    }
}
impl ReactQueries for View {
    type Query = ();
    type Reply = Value;
    fn query(&mut self, _: (), _: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<Value> {
        let children: Vec<_> = self
            .children
            .iter()
            .map(|view| view.clone().downcast::<View>().unwrap().read(cx).value)
            .collect();
        Ok(json!({"value": self.value, "children": children}))
    }
}

fn registry() -> Registry {
    let mut registry = Registry::default();
    registry
        .register(Component::<View>::new("view").children().queries())
        .unwrap();
    registry
        .register(Component::<View>::new("leaf").queries())
        .unwrap();
    registry
}
fn tx(sequence: u64, ops: Value) -> Transaction {
    serde_json::from_value(json!({"version":1,"sequence":sequence,"operations":ops})).unwrap()
}
fn create(id: u64) -> Value {
    json!({"op":"create","id":id,"component":"view","props":{"value":id}})
}
fn place(parent: Option<u64>, child: u64, before: Option<u64>) -> Value {
    json!({"op":"place","parent":parent,"child":child,"before":before})
}

#[gpui::test]
fn topology_is_validated_before_any_component_is_created_or_updated(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, _| Host::new(registry(), Arc::new(|_| {})));
    window.update(cx, |host, window, cx| {
        let invalid = tx(1,json!([create(1), place(None,1,None), {"op":"props","id":1,"props":{"value":-1}}]));
        assert!(host.apply(invalid,window,cx).is_err());
        assert!(host.is_empty());
        host.apply(tx(1,json!([create(1),place(None,1,None)])),window,cx).unwrap();
        let invalid = tx(2,json!([{"op":"props","id":1,"props":{"value":99}},create(2),place(Some(1),2,None),place(Some(2),1,None)]));
        assert!(host.apply(invalid,window,cx).is_err());
        assert_eq!(host.len(),1);
        assert_eq!(host.view(1).unwrap().clone().downcast::<View>().unwrap().read(cx).value,1);
        host.clear(window,cx);
    }).unwrap();
}

#[gpui::test]
fn moves_hidden_children_and_removal_preserve_the_live_gpui_views(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, _| Host::new(registry(), Arc::new(|_| {})));
    window
        .update(cx, |host, window, cx| {
            host.apply(
                tx(
                    1,
                    json!([
                        create(1),
                        create(2),
                        create(3),
                        place(Some(1), 2, None),
                        place(Some(1), 3, None),
                        place(None, 1, None)
                    ]),
                ),
                window,
                cx,
            )
            .unwrap();
            let identity = host.view(2).unwrap().entity_id();
            let reply = host
                .apply(
                    tx(
                        2,
                        json!([
                            place(Some(1),3,Some(2)),
                            {"op":"query","id":1,"request":1,"value":null},
                            {"op":"hidden","id":3,"hidden":true},
                            {"op":"query","id":1,"request":2,"value":null},
                            {"op":"hidden","id":3,"hidden":false},
                            {"op":"remove","id":2},
                            {"op":"query","id":1,"request":3,"value":null}
                        ]),
                    ),
                    window,
                    cx,
                )
                .unwrap();
            assert_eq!(
                reply.results[0].value,
                Some(json!({"value":1,"children":[3,2]}))
            );
            assert_eq!(
                reply.results[1].value,
                Some(json!({"value":1,"children":[2]}))
            );
            assert_eq!(
                reply.results[2].value,
                Some(json!({"value":1,"children":[3]}))
            );
            assert_ne!(host.view(3).unwrap().entity_id(), identity);
            assert_eq!(host.children(1), Some([3].as_slice()));
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
                    tx(
                        1,
                        json!([
                            create(1),place(None,1,None),
                            {"op":"command","id":1,"request":1,"value":"unsupported"},
                            {"op":"query","id":999,"request":2,"value":null},
                            {"op":"query","id":1,"request":3,"value":null}
                        ]),
                    ),
                    window,
                    cx,
                )
                .unwrap();
            assert!(
                reply.results[0]
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("no commands")
            );
            assert!(
                reply.results[1]
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("unknown view")
            );
            assert_eq!(
                reply.results[2].value,
                Some(json!({"value":1,"children":[]}))
            );
            assert!(host.apply(tx(1, json!([])), window, cx).is_err());
            assert!(host.apply(tx(2, json!([create(1)])), window, cx).is_err());
            assert!(host.apply(tx(2, json!([create(2)])), window, cx).is_err());
            host.clear(window, cx);
        })
        .unwrap();
}

#[gpui::test]
fn invalid_parent_and_insertion_anchor_leave_the_original_tree_unchanged(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, _| Host::new(registry(), Arc::new(|_| {})));
    window.update(cx, |host, window, cx| {
        host.apply(tx(1,json!([{"op":"create","id":1,"component":"leaf","props":{"value":1}},place(None,1,None)])),window,cx).unwrap();
        assert!(host.apply(tx(2,json!([create(2),place(Some(1),2,None)])),window,cx).is_err());
        assert!(host.apply(tx(2,json!([create(2),place(None,2,Some(99))])),window,cx).is_err());
        assert_eq!(host.roots(), &[1]);
        assert_eq!(host.len(),1);
        host.clear(window,cx);
    }).unwrap();
}

#[gpui::test]
fn a_state_query_does_not_rebuild_an_unchanged_native_view(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, _| Host::new(registry(), Arc::new(|_| {})));
    let view = window
        .update(cx, |host, window, cx| {
            host.apply(tx(1, json!([create(1), place(None, 1, None)])), window, cx)
                .unwrap();
            host.view(1).unwrap().clone().downcast::<View>().unwrap()
        })
        .unwrap();
    let before = view.read_with(cx, |view, _| view.renders);
    assert!(before > 0);
    window
        .update(cx, |host, window, cx| {
            host.apply(
                tx(2, json!([{"op":"query","id":1,"request":1,"value":null}])),
                window,
                cx,
            )
            .unwrap();
        })
        .unwrap();
    assert_eq!(view.read_with(cx, |view, _| view.renders), before);
    window
        .update(cx, |host, window, cx| host.clear(window, cx))
        .unwrap();
}

#[gpui::test]
fn descendant_updates_invalidate_each_direct_branch_once_before_queries(cx: &mut TestAppContext) {
    let window = cx.add_window(|_, _| Host::new(registry(), Arc::new(|_| {})));
    window
        .update(cx, |host, window, cx| {
            host.apply(
                tx(
                    1,
                    json!([
                        create(1),
                        place(None, 1, None),
                        create(2),
                        place(Some(1), 2, None),
                        create(3),
                        place(Some(2), 3, None)
                    ]),
                ),
                window,
                cx,
            )
            .unwrap();
            let root = host.view(1).unwrap().clone().downcast::<View>().unwrap();
            let child = host.view(2).unwrap().clone().downcast::<View>().unwrap();
            let leaf = host.view(3).unwrap().clone().downcast::<View>().unwrap();
            host.apply(
                tx(
                    2,
                    json!([
                        {"op":"props","id":3,"props":{"value":30}},
                        {"op":"props","id":3,"props":{"value":31}},
                        {"op":"query","id":1,"request":1,"value":null}
                    ]),
                ),
                window,
                cx,
            )
            .unwrap();
            assert_eq!(root.read(cx).changed, vec![vec![child.entity_id()]]);
            assert_eq!(child.read(cx).changed, vec![vec![leaf.entity_id()]]);
            host.apply(
                tx(
                    3,
                    json!([
                        place(Some(1),3,Some(2)),{"op":"props","id":3,"props":{"value":32}},
                        {"op":"remove","id":2},{"op":"query","id":1,"request":2,"value":null}
                    ]),
                ),
                window,
                cx,
            )
            .unwrap();
            // Structural synchronization already invalidates this parent.
            assert_eq!(root.read(cx).changed.len(), 1);
            host.clear(window, cx);
        })
        .unwrap();
}
