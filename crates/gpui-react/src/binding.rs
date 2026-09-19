use crate::{ReactChildren, ReactCommands, ReactEvents, ReactQueries, ReactView};
use anyhow::{Context as _, Result, anyhow, bail};
use gpui::{AnyView, App, AppContext, Entity, Subscription, Window};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{any::Any, cell::Cell, collections::HashMap, marker::PhantomData, rc::Rc, sync::Arc};

/// Already decoded data. It exists only while a transaction is being prepared
/// or applied; mounted views retain their typed props, not this representation.
pub struct Prepared(Box<dyn Any>);

fn decode<T: DeserializeOwned + 'static>(value: Value) -> Result<Prepared> {
    Ok(Prepared(Box::new(serde_json::from_value::<T>(value)?)))
}

fn take<T: 'static>(value: Prepared) -> Result<T> {
    value
        .0
        .downcast::<T>()
        .map(|v| *v)
        .map_err(|_| anyhow!("binding type mismatch"))
}

#[derive(Debug, Serialize)]
pub struct Emission {
    pub target: u64,
    pub subscription: u64,
    pub payload: Result<Value, String>,
}

/// The host must enqueue this record without waiting for JavaScript. Queue
/// overflow must fail the session explicitly. Event serialization failures are
/// delivered as errors rather than dropping the native event.
pub type EventSink = Arc<dyn Fn(Emission) + Send + Sync>;

pub struct MountOptions {
    pub target: u64,
    pub subscription: Option<u64>,
    pub events: EventSink,
}

type Decode = fn(Value) -> Result<Prepared>;
type Apply<T> = fn(&Entity<T>, Prepared, &mut Window, &mut App) -> Result<Value>;
type Children<T> = fn(&Entity<T>, Vec<AnyView>, &mut Window, &mut App);
type Subscribe<T> = fn(&Entity<T>, u64, Rc<Cell<Option<u64>>>, EventSink, &mut App) -> Subscription;

pub struct Component<T: ReactView> {
    name: String,
    events: Option<Subscribe<T>>,
    command: Option<(Decode, Apply<T>)>,
    query: Option<(Decode, Apply<T>)>,
    children: Option<Children<T>>,
    _view: PhantomData<fn() -> T>,
}

impl<T: ReactView> Component<T> {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            events: None,
            command: None,
            query: None,
            children: None,
            _view: PhantomData,
        }
    }

    pub fn events(mut self) -> Self
    where
        T: ReactEvents,
    {
        self.events = Some(|entity, target, route, sink, cx| {
            cx.subscribe(entity, move |_, event: &T::Event, _| {
                if let Some(subscription) = route.get() {
                    sink(Emission {
                        target,
                        subscription,
                        payload: serde_json::to_value(event).map_err(|e| e.to_string()),
                    });
                }
            })
        });
        self
    }

    pub fn commands(mut self) -> Self
    where
        T: ReactCommands,
    {
        self.command = Some((decode::<T::Command>, |entity, value, window, cx| {
            let command = take::<T::Command>(value)?;
            entity.update(cx, |view, cx| view.command(command, window, cx))?;
            Ok(Value::Null)
        }));
        self
    }

    pub fn queries(mut self) -> Self
    where
        T: ReactQueries,
    {
        self.query = Some((decode::<T::Query>, |entity, value, window, cx| {
            let query = take::<T::Query>(value)?;
            let reply = entity.update(cx, |view, cx| view.query(query, window, cx))?;
            Ok(serde_json::to_value(reply)?)
        }));
        self
    }

    pub fn children(mut self) -> Self
    where
        T: ReactChildren,
    {
        self.children = Some(|entity, children, window, cx| {
            entity.update(cx, |view, cx| view.set_children(children, window, cx));
        });
        self
    }
}

trait Binding {
    fn prepare(&self, kind: &str, value: Value) -> Result<Prepared>;
    fn supports(&self, capability: &str) -> bool;
    fn mount(
        &self,
        props: Prepared,
        target: u64,
        route: Rc<Cell<Option<u64>>>,
        sink: EventSink,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<(AnyView, Vec<Subscription>)>;
    fn apply(
        &self,
        view: &AnyView,
        kind: &str,
        value: Prepared,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Value>;
    fn children(
        &self,
        view: &AnyView,
        children: Vec<AnyView>,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<()>;
    fn unmount(&self, view: &AnyView, window: &mut Window, cx: &mut App);
}

fn entity<T: ReactView>(view: &AnyView) -> Entity<T> {
    view.clone()
        .downcast::<T>()
        .expect("registered binding owns its view type")
}

impl<T: ReactView> Binding for Component<T> {
    fn prepare(&self, kind: &str, value: Value) -> Result<Prepared> {
        match kind {
            "props" => decode::<T::Props>(value),
            "command" => self
                .command
                .ok_or_else(|| anyhow!("{} has no commands", self.name))?
                .0(value),
            "query" => self
                .query
                .ok_or_else(|| anyhow!("{} has no queries", self.name))?
                .0(value),
            _ => bail!("unknown binding operation {kind}"),
        }
        .with_context(|| format!("{} {kind}", self.name))
    }

    fn supports(&self, capability: &str) -> bool {
        match capability {
            "events" => self.events.is_some(),
            "commands" => self.command.is_some(),
            "queries" => self.query.is_some(),
            "children" => self.children.is_some(),
            _ => false,
        }
    }

    fn mount(
        &self,
        props: Prepared,
        target: u64,
        route: Rc<Cell<Option<u64>>>,
        sink: EventSink,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<(AnyView, Vec<Subscription>)> {
        let props = take::<T::Props>(props)?;
        let entity = cx.new(|cx| T::create(props, window, cx));
        let subscriptions = self
            .events
            .map(|subscribe| subscribe(&entity, target, route, sink, cx))
            .into_iter()
            .collect();
        entity.update(cx, |view, cx| view.mounted(window, cx));
        Ok((entity.into(), subscriptions))
    }

    fn apply(
        &self,
        view: &AnyView,
        kind: &str,
        value: Prepared,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Value> {
        let entity = entity::<T>(view);
        match kind {
            "props" => {
                let props = take::<T::Props>(value)?;
                entity.update(cx, |view, cx| view.set_props(props, window, cx));
                Ok(Value::Null)
            }
            "command" => self
                .command
                .ok_or_else(|| anyhow!("{} has no commands", self.name))?
                .1(&entity, value, window, cx),
            "query" => self
                .query
                .ok_or_else(|| anyhow!("{} has no queries", self.name))?
                .1(&entity, value, window, cx),
            _ => bail!("unknown binding operation {kind}"),
        }
    }

    fn children(
        &self,
        view: &AnyView,
        children: Vec<AnyView>,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<()> {
        let apply = self
            .children
            .ok_or_else(|| anyhow!("{} does not accept children", self.name))?;
        apply(&entity::<T>(view), children, window, cx);
        Ok(())
    }

    fn unmount(&self, view: &AnyView, window: &mut Window, cx: &mut App) {
        entity::<T>(view).update(cx, |view, cx| view.unmounting(window, cx));
    }
}

#[derive(Default)]
pub struct Registry {
    bindings: HashMap<String, Rc<dyn Binding>>,
}

impl Registry {
    pub fn register<T: ReactView>(&mut self, component: Component<T>) -> Result<()> {
        if component.name.is_empty()
            || !component
                .name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            bail!("component names must contain lowercase ASCII letters, digits, or hyphens");
        }
        if self.bindings.contains_key(&component.name) {
            bail!("duplicate component {}", component.name);
        }
        self.bindings
            .insert(component.name.clone(), Rc::new(component));
        Ok(())
    }

    pub fn prepare_props(&self, name: &str, value: Value) -> Result<Prepared> {
        self.binding(name)?.prepare("props", value)
    }

    pub(crate) fn prepare(&self, name: &str, kind: &str, value: Value) -> Result<Prepared> {
        self.binding(name)?.prepare(kind, value)
    }

    pub fn supports(&self, name: &str, capability: &str) -> Result<bool> {
        Ok(self.binding(name)?.supports(capability))
    }

    fn binding(&self, name: &str) -> Result<Rc<dyn Binding>> {
        self.bindings
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow!("unknown component {name}"))
    }

    pub fn mount(
        &self,
        name: &str,
        props: Prepared,
        options: MountOptions,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<MountedView> {
        let binding = self.binding(name)?;
        if options.subscription.is_some() && !binding.supports("events") {
            bail!("{name} has no events");
        }
        let route = Rc::new(Cell::new(options.subscription));
        let (view, subscriptions) = binding.mount(
            props,
            options.target,
            route.clone(),
            options.events,
            window,
            cx,
        )?;
        Ok(MountedView {
            view,
            binding,
            route,
            subscriptions,
            unmounted: false,
        })
    }
}

pub struct MountedView {
    view: AnyView,
    binding: Rc<dyn Binding>,
    route: Rc<Cell<Option<u64>>>,
    subscriptions: Vec<Subscription>,
    unmounted: bool,
}

impl MountedView {
    pub fn view(&self) -> &AnyView {
        &self.view
    }

    pub fn prepare(&self, kind: &str, value: Value) -> Result<Prepared> {
        if self.unmounted {
            bail!("view is unmounted");
        }
        self.binding.prepare(kind, value)
    }

    pub fn apply(
        &self,
        kind: &str,
        value: Prepared,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Value> {
        if self.unmounted {
            bail!("view is unmounted");
        }
        self.binding.apply(&self.view, kind, value, window, cx)
    }

    pub fn set_children(
        &self,
        children: Vec<AnyView>,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<()> {
        if self.unmounted {
            bail!("view is unmounted");
        }
        self.binding.children(&self.view, children, window, cx)
    }

    pub fn set_subscription(&self, subscription: Option<u64>, cx: &mut App) -> Result<()> {
        if self.unmounted {
            bail!("view is unmounted");
        }
        if subscription.is_some() && !self.binding.supports("events") {
            bail!("view has no events");
        }
        // GPUI emits events through its ordered effect queue. Update the route
        // in that same queue so earlier native commands retain their callback.
        let route = self.route.clone();
        cx.defer(move |_| route.set(subscription));
        Ok(())
    }

    pub fn unmount(&mut self, window: &mut Window, cx: &mut App) {
        if self.unmounted {
            return;
        }
        self.unmounted = true;
        let route = self.route.clone();
        let subscriptions = std::mem::take(&mut self.subscriptions);
        let view = self.view.clone();
        // Earlier Emit effects must run before retirement. Keep the entity
        // alive until then even if the owner removes its last normal handle.
        cx.defer(move |_| {
            route.set(None);
            drop(subscriptions);
            drop(view);
        });
        self.binding.unmount(&self.view, window, cx);
    }
}

impl Drop for MountedView {
    fn drop(&mut self) {
        // A scene can still hold the GPUI view until its frame is retired.
        // Dropping the bridge must nevertheless stop its event delivery now.
        if !self.unmounted {
            self.route.set(None);
        }
    }
}
