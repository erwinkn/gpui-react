use gpui_react::{
    gpui::{self, Context, Entity, Global, Render, TestAppContext, Window, div, prelude::*, px},
    *,
};
use serde_json::json;
use std::sync::Arc;

#[derive(Default)]
struct Observed(Vec<FrameInfo>);
impl Global for Observed {}
struct Probe;
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("probe")
            .w(px(10.))
            .h(px(10.))
            .on_painted(|_, window, cx| {
                let info = current_frame(window, cx).unwrap();
                cx.global_mut::<Observed>().0.push(info);
            })
    }
}
impl ReactView for Probe {
    type Props = ();
    fn create(_: (), _: &mut Window, _: &mut Context<Self>) -> Self {
        Self
    }
    fn set_props(&mut self, _: (), _: &mut Window, _: &mut Context<Self>) {}
}
struct Nested(Entity<Host>);
impl Render for Nested {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.clone()
    }
}
impl ReactView for Nested {
    type Props = ();
    fn create(_: (), window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut registry = Registry::default();
        registry.register(Component::<Probe>::new("probe")).unwrap();
        let host = cx.new(|_| Host::new(registry, Arc::new(|_| {})));
        host.update(cx, |host, cx| {
            host.apply(
                gpui_react::protocol::Transaction::from_json(&json!({"version":1,"sequence":1,"operations":[
                    {"op":"create","id":1,"component":"probe","props":null},
                    {"op":"place","parent":null,"child":1,"before":null}
                ]}))
                .unwrap(),
                window,
                cx,
            )
            .unwrap();
            host.apply(
                gpui_react::protocol::Transaction::from_json(&json!({"version":1,"sequence":2,"operations":[]})).unwrap(),
                window,
                cx,
            )
            .unwrap();
        });
        Self(host)
    }
    fn set_props(&mut self, _: (), _: &mut Window, _: &mut Context<Self>) {}
    fn unmounting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.0.update(cx, |host, cx| host.clear(window, cx));
    }
}
#[gpui::test]
fn nested_hosts_restore_the_paint_scope_without_a_layout_box(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Observed::default()));
    let mut registry = Registry::default();
    registry.register(Component::<Probe>::new("probe")).unwrap();
    registry
        .register(Component::<Nested>::new("nested"))
        .unwrap();
    let window = cx.add_window(|_, _| Host::new(registry, Arc::new(|_| {})));
    window.update(cx,|host,window,cx| {
        host.apply(gpui_react::protocol::Transaction::from_json(&json!({"version":1,"sequence":1,"operations":[
            {"op":"create","id":1,"component":"probe","props":null},{"op":"place","parent":null,"child":1,"before":null},
            {"op":"create","id":2,"component":"nested","props":null},{"op":"place","parent":null,"child":2,"before":null},
            {"op":"create","id":3,"component":"probe","props":null},{"op":"place","parent":null,"child":3,"before":null}
        ]})).unwrap(),window,cx).unwrap();
    }).unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        cx.global_mut::<Observed>().0.clear();
        window.draw(cx).clear(cx);
        let seen = &cx.global::<Observed>().0;
        assert_eq!(seen.len(), 3);
        assert_eq!(
            seen.iter().map(|info| info.commit).collect::<Vec<_>>(),
            vec![1, 2, 1]
        );
        assert_eq!(seen[0].root, seen[2].root);
        assert_ne!(seen[0].root, seen[1].root);
        assert_eq!(seen[0].frame, seen[2].frame);
        assert!(current_frame(window, cx).is_none());
    })
    .unwrap();
    window
        .update(cx, |host, window, cx| host.clear(window, cx))
        .unwrap();
}

// Deferred GPUI elements are painted after the ordinary root paint returns.
// They still belong to the same native host and must keep its frame metadata.
struct FloatingLayer(Entity<Probe>);
impl Render for FloatingLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .child(gpui::deferred(self.0.clone()).with_priority(2))
            .on_painted(|_, window, cx| {
                let info = current_frame(window, cx).unwrap();
                cx.global_mut::<Observed>().0.push(info);
            })
    }
}
struct DeferredProbe(Entity<FloatingLayer>);
impl Render for DeferredProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::deferred(self.0.clone())
    }
}
impl ReactView for DeferredProbe {
    type Props = ();
    fn create(_: (), _: &mut Window, cx: &mut Context<Self>) -> Self {
        Self(cx.new(|cx| FloatingLayer(cx.new(|_| Probe))))
    }
    fn set_props(&mut self, _: (), _: &mut Window, _: &mut Context<Self>) {}
}
#[gpui::test]
fn deferred_native_views_keep_their_host_frame_context(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Observed::default()));
    let mut registry = Registry::default();
    registry
        .register(Component::<DeferredProbe>::new("floating"))
        .unwrap();
    let handle = cx.add_window(|_, _| Host::new(registry, Arc::new(|_| {})));
    handle
        .update(cx, |host, window, cx| {
            host.apply(
                gpui_react::protocol::Transaction::from_json(&json!({"version":1,"sequence":1,"operations":[
                    {"op":"create","id":1,"component":"floating","props":null},
                    {"op":"place","parent":null,"child":1,"before":null}
                ]}))
                .unwrap(),
                window,
                cx,
            )
            .unwrap();
        })
        .unwrap();
    cx.update_window(handle.into(), |_, window, cx| {
        cx.global_mut::<Observed>().0.clear();
        window.draw(cx).clear(cx);
        let observed = &cx.global::<Observed>().0;
        assert_eq!(observed.len(), 2);
        assert!(observed.iter().all(|info| info.commit == 1));
        assert_eq!(observed[0].root, observed[1].root);
        assert_eq!(observed[0].frame, observed[1].frame);
        assert!(current_frame(window, cx).is_none());
    })
    .unwrap();
    handle
        .update(cx, |host, window, cx| host.clear(window, cx))
        .unwrap();
}

struct DeferredNested(Entity<Host>);
impl Render for DeferredNested {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::deferred(self.0.clone())
    }
}
impl ReactView for DeferredNested {
    type Props = ();
    fn create(_: (), window: &mut Window, cx: &mut Context<Self>) -> Self {
        let nested = cx.new(|cx| Nested::create((), window, cx));
        Self(nested.read(cx).0.clone())
    }
    fn set_props(&mut self, _: (), _: &mut Window, _: &mut Context<Self>) {}
    fn unmounting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.0.update(cx, |host, cx| host.clear(window, cx));
    }
}
#[gpui::test]
fn deferred_nested_hosts_keep_distinct_scopes_across_draws(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Observed::default()));
    let mut registry = Registry::default();
    registry
        .register(Component::<DeferredNested>::new("nested"))
        .unwrap();
    registry
        .register(Component::<DeferredProbe>::new("floating"))
        .unwrap();
    let handle = cx.add_window(|_, _| Host::new(registry, Arc::new(|_| {})));
    handle
        .update(cx, |host, window, cx| {
            host.apply(
                gpui_react::protocol::Transaction::from_json(&json!({"version":1,"sequence":1,"operations":[
                    {"op":"create","id":1,"component":"nested","props":null},
                    {"op":"place","parent":null,"child":1,"before":null},
                    {"op":"create","id":2,"component":"floating","props":null},
                    {"op":"place","parent":null,"child":2,"before":null}
                ]}))
                .unwrap(),
                window,
                cx,
            )
            .unwrap();
        })
        .unwrap();
    let mut previous_frame = None;
    for sequence in 2..=3 {
        handle
            .update(cx, |host, window, cx| {
                host.apply(
                    gpui_react::protocol::Transaction::from_json(&
                        json!({"version":1,"sequence":sequence,"operations":[]}),
                    )
                    .unwrap(),
                    window,
                    cx,
                )
                .unwrap();
            })
            .unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            cx.global_mut::<Observed>().0.clear();
            window.draw(cx).clear(cx);
            let observed = &cx.global::<Observed>().0;
            assert_eq!(observed.len(), 3);
            assert_eq!(observed[0].commit, 2);
            assert_ne!(observed[0].root, observed[1].root);
            assert_eq!(observed[1].root, observed[2].root);
            assert_eq!(observed[1].commit, sequence);
            assert_eq!(observed[2].commit, sequence);
            assert!(previous_frame.is_none_or(|frame| observed[1].frame > frame));
            previous_frame = Some(observed[1].frame);
            assert!(current_frame(window, cx).is_none());
        })
        .unwrap();
    }
    handle
        .update(cx, |host, window, cx| host.clear(window, cx))
        .unwrap();
}
