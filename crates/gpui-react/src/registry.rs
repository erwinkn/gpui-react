//! Component bindings. A binding decodes typed props on the worker thread and
//! owns a table of its kind's rows on the UI thread: either host-owned element
//! values or GPUI entities.
use crate::{
    wire, Children, ElementCommands, ElementContext, ElementQueries, Host, ReactChildren,
    ReactCommands, ReactElement, ReactEvents, ReactQueries, ReactView, RenderContext,
};
use anyhow::{Context as _, Result, anyhow, bail, ensure};
use gpui::{
    AnyElement, AnyView, App, AppContext, Entity, IntoElement, Subscription, WeakEntity, Window,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, value::RawValue};
use std::{any::Any, collections::HashMap, marker::PhantomData, sync::Arc};

#[derive(Debug, Serialize)]
pub struct Emission {
    pub target: u32,
    pub subscription: u64,
    pub payload: Result<Value, String>,
}

/// The host must enqueue this record without waiting for JavaScript. Queue
/// overflow must fail the session explicitly. Event serialization failures are
/// delivered as errors rather than dropping the native event.
pub type EventSink = Arc<dyn Fn(Emission) + Send + Sync>;

/// Delivers a node's native events to its current JavaScript listener. It
/// carries only the host and the node's identity; the subscription is read
/// from the node when the event fires, so a replaced or removed listener is
/// honored without rebuilding native listeners.
#[derive(Clone)]
pub struct Emitter {
    host: WeakEntity<Host>,
    id: u32,
    generation: u16,
}

impl Emitter {
    pub(crate) fn new(host: WeakEntity<Host>, id: u32, generation: u16) -> Self {
        Self {
            host,
            id,
            generation,
        }
    }
    pub fn emit<E: Serialize + ?Sized>(&self, event: &E, cx: &App) {
        let Some(host) = self.host.upgrade() else {
            return;
        };
        let Some((sink, subscription)) = host.read(cx).route(self.id, self.generation) else {
            return;
        };
        sink(Emission {
            target: self.id,
            subscription,
            payload: serde_json::to_value(event).map_err(|e| e.to_string()),
        });
    }
}

/// Decoded, typed data. It crosses from the worker to the UI thread.
pub(crate) type Payload = Box<dyn Any + Send>;

fn decode<T: DeserializeOwned + Send + 'static>(value: &RawValue) -> Result<Payload> {
    Ok(Box::new(serde_json::from_str::<T>(value.get())?))
}

/// A free-form value from the tagged tree: commands and queries.
fn decode_wire<T: DeserializeOwned + Send + 'static>(reader: &mut wire::Reader<'_>) -> Result<Payload> {
    Ok(Box::new(T::deserialize(reader)?))
}
/// Props: positional against the type's schema, or a tagged map without one.
fn decode_wire_props<T: DeserializeOwned + wire::ComponentProps + Send + 'static>(reader: &mut wire::Reader<'_>) -> Result<Payload> {
    Ok(Box::new(match T::SCHEMA {
        wire::Schema::Fields(fields) => T::deserialize(wire::PropsReader { reader, fields })?,
        wire::Schema::Map => T::deserialize(reader)?,
    }))
}

fn take<T: 'static>(value: Payload) -> Result<T> {
    value
        .downcast::<T>()
        .map(|v| *v)
        .map_err(|_| anyhow!("binding type mismatch"))
}

/// One component as the worker sees it.
#[derive(Debug, Serialize)]
pub struct KindSchema {
    pub name: String,
    pub capabilities: Capabilities,
    /// `None` when the props type has no positional schema and travels as a map.
    pub fields: wire::Schema,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub events: bool,
    pub commands: bool,
    pub queries: bool,
    pub children: bool,
    /// The node is a GPUI entity rather than host-owned data.
    pub view: bool,
}

#[doc(hidden)]
#[derive(Clone, Copy)]
pub enum Kind {
    Props,
    Command,
    Query,
}

/// The rows of one component kind on the UI thread. One per kind, indexed by
/// slot; the host's nodes point into it.
#[doc(hidden)]
pub trait Table {
    fn create(
        &mut self,
        props: Payload,
        emitter: Option<Emitter>,
        cx: &mut ElementContext,
    ) -> Result<u32>;
    fn set_props(&mut self, slot: u32, props: Payload, cx: &mut ElementContext) -> Result<()>;
    fn command(&mut self, slot: u32, value: Payload, cx: &mut ElementContext) -> Result<()>;
    fn query(&mut self, slot: u32, value: Payload, cx: &mut ElementContext) -> Result<Value>;
    fn render(&self, slot: u32, cx: &mut RenderContext) -> AnyElement;
    fn set_children(&self, slot: u32, children: Children, window: &mut Window, cx: &mut App);
    fn child_changed(&self, slot: u32, child: u32, window: &mut Window, cx: &mut App);
    fn remove(&mut self, slot: u32, cx: &mut ElementContext);
    fn view(&self, slot: u32) -> Option<AnyView>;
    /// The row and its kind's extras, for host-owned elements.
    fn element_mut(&mut self, slot: u32) -> Option<(&mut dyn Any, &mut dyn Any)>;
    fn reserve(&mut self, additional: usize);
}

#[doc(hidden)]
pub trait Binding: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn capabilities(&self) -> Capabilities;
    fn decode(&self, kind: Kind, value: &RawValue) -> Result<Payload>;
    /// The same typed decode from the binary wire.
    fn decode_wire(&self, kind: Kind, reader: &mut wire::Reader<'_>) -> Result<Payload>;
    /// The props schema the worker encodes against.
    fn schema(&self) -> wire::Schema;
    /// Checks the derived schema against serde's view of the props struct.
    fn verify_schema(&self) -> Result<()>;
    fn table(&self) -> Box<dyn Table>;
}

type Decode = fn(&RawValue) -> Result<Payload>;
type DecodeWire = fn(&mut wire::Reader<'_>) -> Result<Payload>;

/// Dense rows with a free list. Removed rows are reused before the vector grows.
struct Rows<T> {
    rows: Vec<Option<T>>,
    free: Vec<u32>,
}
impl<T> Rows<T> {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            free: Vec::new(),
        }
    }
    fn reserve(&mut self, additional: usize) {
        let additional = additional.saturating_sub(self.free.len());
        self.rows.reserve_exact(additional);
    }
    fn insert(&mut self, row: T) -> u32 {
        self.insert_with(|_| row)
    }
    /// Insert a row built with knowledge of its slot.
    fn insert_with(&mut self, build: impl FnOnce(u32) -> T) -> u32 {
        match self.free.pop() {
            Some(slot) => {
                self.rows[slot as usize] = Some(build(slot));
                slot
            }
            None => {
                let slot = self.rows.len() as u32;
                self.rows.push(Some(build(slot)));
                slot
            }
        }
    }
    fn get(&self, slot: u32) -> &T {
        self.rows[slot as usize]
            .as_ref()
            .expect("host slots reference live rows")
    }
    fn get_mut(&mut self, slot: u32) -> &mut T {
        self.rows[slot as usize]
            .as_mut()
            .expect("host slots reference live rows")
    }
    fn remove(&mut self, slot: u32) -> T {
        self.free.push(slot);
        self.rows[slot as usize]
            .take()
            .expect("host slots reference live rows")
    }
}

// ---- entity-backed components ---------------------------------------------

type Apply<T> = fn(&Entity<T>, Payload, &mut Window, &mut App) -> Result<Value>;
type Subscribe<T> = fn(&Entity<T>, Emitter, &mut App) -> Subscription;
type SetChildren<T> = fn(&Entity<T>, Children, &mut Window, &mut App);
type ChildChanged<T> = fn(&Entity<T>, u32, &mut Window, &mut App);

/// Registers a `ReactView` under a component name with optional capabilities.
pub struct Component<T: ReactView> {
    name: String,
    subscribe: Option<Subscribe<T>>,
    command: Option<(Decode, DecodeWire, Apply<T>)>,
    query: Option<(Decode, DecodeWire, Apply<T>)>,
    children: Option<(SetChildren<T>, ChildChanged<T>)>,
    _view: PhantomData<fn() -> T>,
}

impl<T: ReactView> Component<T> {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            subscribe: None,
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
        self.subscribe = Some(|entity, emitter, cx| {
            cx.subscribe(entity, move |_, event: &T::Event, cx| emitter.emit(event, cx))
        });
        self
    }
    pub fn commands(mut self) -> Self
    where
        T: ReactCommands,
    {
        self.command = Some((decode::<T::Command>, decode_wire::<T::Command>, |entity, value, window, cx| {
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
        self.query = Some((decode::<T::Query>, decode_wire::<T::Query>, |entity, value, window, cx| {
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
        self.children = Some((
            |entity, children, window, cx| {
                entity.update(cx, |view, cx| view.set_children(children, window, cx))
            },
            |entity, child, window, cx| {
                entity.update(cx, |view, cx| view.child_changed(child, window, cx))
            },
        ));
        self
    }
}

struct ViewRow<T> {
    entity: Entity<T>,
    /// Kept alive with the entity; dropped together after pending effects.
    _subscription: Option<Subscription>,
}

struct ViewTable<T: ReactView> {
    rows: Rows<ViewRow<T>>,
    subscribe: Option<Subscribe<T>>,
    command: Option<Apply<T>>,
    query: Option<Apply<T>>,
    children: Option<(SetChildren<T>, ChildChanged<T>)>,
}

impl<T: ReactView> Table for ViewTable<T> {
    fn create(
        &mut self,
        props: Payload,
        emitter: Option<Emitter>,
        cx: &mut ElementContext,
    ) -> Result<u32> {
        let props = take::<T::Props>(props)?;
        let window = &mut *cx.window;
        let entity = cx.cx.new(|cx| T::create(props, window, cx));
        let subscription = match (self.subscribe, emitter) {
            (Some(subscribe), Some(emitter)) => Some(subscribe(&entity, emitter, cx.cx)),
            _ => None,
        };
        entity.update(cx.cx, |view, cx| view.mounted(window, cx));
        Ok(self.rows.insert(ViewRow {
            entity,
            _subscription: subscription,
        }))
    }
    fn set_props(&mut self, slot: u32, props: Payload, cx: &mut ElementContext) -> Result<()> {
        let props = take::<T::Props>(props)?;
        let window = &mut *cx.window;
        self.rows
            .get(slot)
            .entity
            .update(cx.cx, |view, cx| view.set_props(props, window, cx));
        Ok(())
    }
    fn command(&mut self, slot: u32, value: Payload, cx: &mut ElementContext) -> Result<()> {
        let apply = self.command.ok_or_else(|| anyhow!("view has no commands"))?;
        apply(&self.rows.get(slot).entity, value, cx.window, cx.cx).map(|_| ())
    }
    fn query(&mut self, slot: u32, value: Payload, cx: &mut ElementContext) -> Result<Value> {
        let apply = self.query.ok_or_else(|| anyhow!("view has no queries"))?;
        apply(&self.rows.get(slot).entity, value, cx.window, cx.cx)
    }
    fn render(&self, slot: u32, _: &mut RenderContext) -> AnyElement {
        self.rows.get(slot).entity.clone().into_any_element()
    }
    fn set_children(&self, slot: u32, children: Children, window: &mut Window, cx: &mut App) {
        if let Some((set, _)) = self.children {
            set(&self.rows.get(slot).entity, children, window, cx);
        }
    }
    fn child_changed(&self, slot: u32, child: u32, window: &mut Window, cx: &mut App) {
        if let Some((_, changed)) = self.children {
            changed(&self.rows.get(slot).entity, child, window, cx);
        }
    }
    fn remove(&mut self, slot: u32, cx: &mut ElementContext) {
        let row = self.rows.remove(slot);
        let window = &mut *cx.window;
        row.entity
            .update(cx.cx, |view, cx| view.unmounting(window, cx));
        // GPUI delivers `cx.emit` through its ordered effect queue. Keep the
        // entity and its subscription alive until earlier effects have run;
        // the host keeps the retired subscription routable until then.
        cx.cx.defer(move |_| drop(row));
    }
    fn view(&self, slot: u32) -> Option<AnyView> {
        Some(self.rows.get(slot).entity.clone().into())
    }
    fn element_mut(&mut self, _: u32) -> Option<(&mut dyn Any, &mut dyn Any)> {
        None
    }
    fn reserve(&mut self, additional: usize) {
        self.rows.reserve(additional);
    }
}

impl<T: ReactView> Binding for Component<T> {
    fn name(&self) -> &str {
        &self.name
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            events: self.subscribe.is_some(),
            commands: self.command.is_some(),
            queries: self.query.is_some(),
            children: self.children.is_some(),
            view: true,
        }
    }
    fn decode(&self, kind: Kind, value: &RawValue) -> Result<Payload> {
        match kind {
            Kind::Props => decode::<T::Props>(value),
            Kind::Command => self
                .command
                .ok_or_else(|| anyhow!("{} has no commands", self.name))?
                .0(value),
            Kind::Query => self
                .query
                .ok_or_else(|| anyhow!("{} has no queries", self.name))?
                .0(value),
        }
        .with_context(|| format!("{} {}", self.name, kind_name(kind)))
    }
    fn decode_wire(&self, kind: Kind, reader: &mut wire::Reader<'_>) -> Result<Payload> {
        match kind {
            Kind::Props => decode_wire_props::<T::Props>(reader),
            Kind::Command => self
                .command
                .ok_or_else(|| anyhow!("{} has no commands", self.name))?
                .1(reader),
            Kind::Query => self
                .query
                .ok_or_else(|| anyhow!("{} has no queries", self.name))?
                .1(reader),
        }
        .with_context(|| format!("{} {}", self.name, kind_name(kind)))
    }
    fn schema(&self) -> wire::Schema {
        <T::Props as wire::ComponentProps>::SCHEMA
    }
    fn verify_schema(&self) -> Result<()> {
        wire::verify::<T::Props>().with_context(|| format!("{} props", self.name))
    }
    fn table(&self) -> Box<dyn Table> {
        Box::new(ViewTable::<T> {
            rows: Rows::new(),
            subscribe: self.subscribe,
            command: self.command.map(|c| c.2),
            query: self.query.map(|q| q.2),
            children: self.children,
        })
    }
}

// ---- host-owned elements ---------------------------------------------------

type ElementApply<T> =
    fn(&mut T, &mut <T as ReactElement>::Extras, Payload, &mut ElementContext) -> Result<Value>;

/// Registers a `ReactElement` under a component name with optional capabilities.
pub struct HostElement<T: ReactElement> {
    name: String,
    events: bool,
    children: bool,
    command: Option<(Decode, DecodeWire, ElementApply<T>)>,
    query: Option<(Decode, DecodeWire, ElementApply<T>)>,
    _element: PhantomData<fn() -> T>,
}

impl<T: ReactElement> HostElement<T> {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            events: false,
            children: false,
            command: None,
            query: None,
            _element: PhantomData,
        }
    }
    /// The element emits events through `RenderContext::emitter`.
    pub fn events(mut self) -> Self {
        self.events = true;
        self
    }
    /// The element renders `RenderContext::children`.
    pub fn children(mut self) -> Self {
        self.children = true;
        self
    }
    pub fn commands(mut self) -> Self
    where
        T: ElementCommands,
    {
        self.command = Some((decode::<T::Command>, decode_wire::<T::Command>, |element, extras, value, cx| {
            element.command(take::<T::Command>(value)?, extras, cx)?;
            Ok(Value::Null)
        }));
        self
    }
    pub fn queries(mut self) -> Self
    where
        T: ElementQueries,
    {
        self.query = Some((decode::<T::Query>, decode_wire::<T::Query>, |element, extras, value, cx| {
            Ok(serde_json::to_value(
                element.query(take::<T::Query>(value)?, extras, cx)?,
            )?)
        }));
        self
    }
}

struct ElementTable<T: ReactElement> {
    rows: Rows<T>,
    extras: T::Extras,
    command: Option<ElementApply<T>>,
    query: Option<ElementApply<T>>,
}

impl<T: ReactElement> Table for ElementTable<T> {
    fn create(&mut self, props: Payload, _: Option<Emitter>, cx: &mut ElementContext) -> Result<u32> {
        let props = take::<T::Props>(props)?;
        let extras = &mut self.extras;
        Ok(self.rows.insert_with(|slot| {
            cx.slot = slot;
            T::create(props, extras, cx)
        }))
    }
    fn set_props(&mut self, slot: u32, props: Payload, cx: &mut ElementContext) -> Result<()> {
        self.rows
            .get_mut(slot)
            .set_props(take::<T::Props>(props)?, &mut self.extras, cx);
        Ok(())
    }
    fn command(&mut self, slot: u32, value: Payload, cx: &mut ElementContext) -> Result<()> {
        let apply = self.command.ok_or_else(|| anyhow!("element has no commands"))?;
        apply(self.rows.get_mut(slot), &mut self.extras, value, cx).map(|_| ())
    }
    fn query(&mut self, slot: u32, value: Payload, cx: &mut ElementContext) -> Result<Value> {
        let apply = self.query.ok_or_else(|| anyhow!("element has no queries"))?;
        apply(self.rows.get_mut(slot), &mut self.extras, value, cx)
    }
    fn render(&self, slot: u32, cx: &mut RenderContext) -> AnyElement {
        self.rows.get(slot).render(&self.extras, cx)
    }
    fn set_children(&self, _: u32, _: Children, _: &mut Window, _: &mut App) {}
    fn child_changed(&self, _: u32, _: u32, _: &mut Window, _: &mut App) {}
    fn remove(&mut self, slot: u32, cx: &mut ElementContext) {
        self.rows.remove(slot).unmount(&mut self.extras, cx);
    }
    fn view(&self, _: u32) -> Option<AnyView> {
        None
    }
    fn element_mut(&mut self, slot: u32) -> Option<(&mut dyn Any, &mut dyn Any)> {
        Some((self.rows.get_mut(slot), &mut self.extras))
    }
    fn reserve(&mut self, additional: usize) {
        self.rows.reserve(additional);
    }
}

impl<T: ReactElement> Binding for HostElement<T> {
    fn name(&self) -> &str {
        &self.name
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            events: self.events,
            commands: self.command.is_some(),
            queries: self.query.is_some(),
            children: self.children,
            view: false,
        }
    }
    fn decode(&self, kind: Kind, value: &RawValue) -> Result<Payload> {
        match kind {
            Kind::Props => decode::<T::Props>(value),
            Kind::Command => self
                .command
                .ok_or_else(|| anyhow!("{} has no commands", self.name))?
                .0(value),
            Kind::Query => self
                .query
                .ok_or_else(|| anyhow!("{} has no queries", self.name))?
                .0(value),
        }
        .with_context(|| format!("{} {}", self.name, kind_name(kind)))
    }
    fn decode_wire(&self, kind: Kind, reader: &mut wire::Reader<'_>) -> Result<Payload> {
        match kind {
            Kind::Props => decode_wire_props::<T::Props>(reader),
            Kind::Command => self
                .command
                .ok_or_else(|| anyhow!("{} has no commands", self.name))?
                .1(reader),
            Kind::Query => self
                .query
                .ok_or_else(|| anyhow!("{} has no queries", self.name))?
                .1(reader),
        }
        .with_context(|| format!("{} {}", self.name, kind_name(kind)))
    }
    fn schema(&self) -> wire::Schema {
        <T::Props as wire::ComponentProps>::SCHEMA
    }
    fn verify_schema(&self) -> Result<()> {
        wire::verify::<T::Props>().with_context(|| format!("{} props", self.name))
    }
    fn table(&self) -> Box<dyn Table> {
        Box::new(ElementTable::<T> {
            rows: Rows::new(),
            extras: T::Extras::default(),
            command: self.command.map(|c| c.2),
            query: self.query.map(|q| q.2),
        })
    }
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Props => "props",
        Kind::Command => "command",
        Kind::Query => "query",
    }
}

// ---- registry --------------------------------------------------------------

#[derive(Default)]
pub struct Registry {
    names: HashMap<String, u16>,
    bindings: Vec<Arc<dyn Binding>>,
}

impl Registry {
    pub fn register(&mut self, binding: impl Binding) -> Result<()> {
        let name = binding.name();
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            bail!("component names must contain lowercase ASCII letters, digits, or hyphens");
        }
        if self.names.contains_key(name) {
            bail!("duplicate component {name}");
        }
        ensure!(self.bindings.len() < u16::MAX as usize, "too many components");
        self.names
            .insert(name.to_owned(), self.bindings.len() as u16);
        self.bindings.push(Arc::new(binding));
        Ok(())
    }

    pub fn kind(&self, name: &str) -> Result<u16> {
        self.names
            .get(name)
            .copied()
            .ok_or_else(|| anyhow!("unknown component {name}"))
    }

    pub(crate) fn len(&self) -> usize {
        self.bindings.len()
    }

    pub(crate) fn binding(&self, kind: u16) -> &dyn Binding {
        &*self.bindings[kind as usize]
    }

    pub fn capabilities(&self, name: &str) -> Result<Capabilities> {
        Ok(self.binding(self.kind(name)?).capabilities())
    }

    /// The kind table the worker encodes against: index order is kind order.
    pub fn schema(&self) -> Vec<KindSchema> {
        self.bindings
            .iter()
            .map(|binding| KindSchema {
                name: binding.name().to_owned(),
                capabilities: binding.capabilities(),
                fields: binding.schema(),
            })
            .collect()
    }

    /// Fails when any component's derived schema disagrees with its serde derive.
    pub fn verify_schemas(&self) -> Result<()> {
        self.bindings.iter().try_for_each(|binding| binding.verify_schema())
    }

    /// One empty row table per kind, in kind order.
    pub(crate) fn tables(&self) -> Vec<Box<dyn Table>> {
        self.bindings.iter().map(|binding| binding.table()).collect()
    }
}

/// A decoded transaction ready for the UI thread.
pub struct Prepared {
    pub(crate) sequence: u64,
    pub(crate) operations: Vec<Op>,
}

impl Prepared {
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
}

pub(crate) enum Op {
    Create {
        id: u32,
        kind: u16,
        props: Payload,
        subscription: Option<u64>,
        /// Placement folded into creation: `Some(parent)` places the new node,
        /// with `parent == None` meaning the root list.
        place: Option<(Option<u32>, Option<u32>)>,
    },
    Props {
        id: u32,
        kind: u16,
        props: Payload,
    },
    Listen {
        id: u32,
        subscription: Option<u64>,
    },
    Place {
        parent: Option<u32>,
        child: u32,
        before: Option<u32>,
    },
    Remove {
        id: u32,
    },
    Hidden {
        id: u32,
        hidden: bool,
    },
    /// A schema failure is reported as the call's result and never invokes
    /// native code.
    Call {
        id: u32,
        kind: u16,
        request: u64,
        command: bool,
        value: Result<Payload>,
    },
}
