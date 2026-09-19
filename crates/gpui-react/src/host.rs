//! The single native owner of a React tree: a flat vector of node records with
//! sibling links, one typed row table per component kind, rendered into GPUI
//! elements every frame. Entities exist only for kinds registered as views.
use crate::{
    Decoder, FrameInfo, ReactElement, Registry,
    protocol::{CallResult, Reply, Transaction},
    registry::{Emitter, EventSink, Op, Prepared, Table},
};
use anyhow::{Result, anyhow, ensure};
use gpui::{
    AnyElement, AnyView, App, Context, ElementId, IntoElement, Render, WeakEntity, Window, div,
    prelude::*,
};
use rustc_hash::FxHashSet;
use std::rc::Rc;

/// Link sentinels. Real ids stay below these.
const ROOT: u32 = u32::MAX;
const DETACHED: u32 = u32::MAX - 1;
const NONE: u32 = u32::MAX;
const EMPTY_KIND: u16 = u16::MAX;
const HIDDEN: u16 = 1 << 15;

/// One React host node. Thirty-six bytes of integers; the node's data is a row
/// in its kind's table, and its children are reached through sibling links.
#[repr(C)]
#[derive(Clone, Copy)]
struct Node {
    parent: u32,
    first_child: u32,
    last_child: u32,
    next_sibling: u32,
    prev_sibling: u32,
    child_count: u32,
    /// Current `onEvent` subscription, zero when nobody listens.
    subscription: u32,
    /// Row in `tables[kind]`.
    slot: u32,
    /// Registry index, with the top bit marking a hidden node. `EMPTY_KIND`
    /// marks a free record.
    kind: u16,
    /// Incremented each time this record is reused, so a listener from an
    /// earlier life of the id cannot reach the new node.
    generation: u16,
}

impl Node {
    const EMPTY: Node = Node {
        parent: DETACHED,
        first_child: NONE,
        last_child: NONE,
        next_sibling: NONE,
        prev_sibling: NONE,
        child_count: 0,
        subscription: 0,
        slot: 0,
        kind: EMPTY_KIND,
        generation: 0,
    };
    fn kind(&self) -> u16 {
        self.kind & !HIDDEN
    }
    fn hidden(&self) -> bool {
        self.kind & HIDDEN != 0
    }
    fn is_empty(&self) -> bool {
        self.kind == EMPTY_KIND
    }
}

/// Owns the committed React tree for one window root.
pub struct Host {
    decoder: Decoder,
    /// Indexed by host id. JavaScript reuses ids after their removal is
    /// acknowledged, so this stays dense.
    nodes: Vec<Node>,
    live: usize,
    first_root: u32,
    last_root: u32,
    root_count: u32,
    /// One row table per registered kind, indexed by `Node::kind`.
    tables: Vec<Box<dyn Table>>,
    events: EventSink,
    /// Subscriptions of removed nodes, kept until the effects queued before
    /// removal have run. Cleared at the start of the next transaction.
    retiring: Vec<(u32, u16, u32)>,
    this: Option<WeakEntity<Host>>,
    sequence: u64,
    frame: u64,
    last_subscription: u64,
    last_request: u64,
}

/// Passed to host-owned elements outside rendering.
pub struct ElementContext<'a> {
    pub id: u32,
    /// The row's slot in its kind's table; the key for `Extras` maps.
    pub slot: u32,
    pub child_count: usize,
    pub window: &'a mut Window,
    pub cx: &'a mut App,
}

/// Passed to host-owned elements while the host builds its element tree.
pub struct RenderContext<'a> {
    pub id: u32,
    /// The row's slot in its kind's table; the key for `Extras` maps.
    pub slot: u32,
    pub window: &'a mut Window,
    pub cx: &'a mut App,
    host: &'a Host,
    node: Node,
}

impl RenderContext<'_> {
    /// A stable id for this node. GPUI keeps hover, active, and scroll state
    /// across frames for elements that use it.
    pub fn element_id(&self) -> ElementId {
        ElementId::Integer(self.id as u64)
    }
    /// Present only while JavaScript listens to this node.
    pub fn emitter(&self) -> Option<Emitter> {
        (self.node.subscription != 0).then(|| self.host.emitter(self.id, self.node.generation))
    }
    pub fn child_count(&self) -> usize {
        self.node.child_count as usize
    }
    /// Build the visible children now. Elements that lay out all children
    /// call this; lazy consumers register as views and use `Children`.
    pub fn children(&mut self) -> Vec<AnyElement> {
        let mut children = Vec::with_capacity(self.node.child_count as usize);
        let mut id = self.node.first_child;
        while id != NONE {
            if let Some(element) = self.host.render_child(id, self.window, self.cx) {
                children.push(element);
            }
            id = self.host.nodes[id as usize].next_sibling;
        }
        children
    }
    /// For paint callbacks that write back into this node through
    /// `Host::update_element`.
    pub fn host(&self) -> WeakEntity<Host> {
        self.host.this.clone().expect("host renders only as an entity")
    }
}

/// Lazily renders the React children of a view from the host tree.
#[derive(Clone)]
pub struct Children {
    host: WeakEntity<Host>,
    ids: Rc<[u32]>,
}

impl Children {
    /// Visible child node ids, in order, as of the last change notification.
    pub fn ids(&self) -> &[u32] {
        &self.ids
    }
    pub fn len(&self) -> usize {
        self.ids.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
    /// Build one child. Call this from a view's layout or render, never while
    /// the host itself is being updated.
    pub fn render(&self, index: usize, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let id = *self.ids.get(index)?;
        self.host
            .update(cx, |host, cx| host.render_child(id, window, cx))
            .ok()
            .flatten()
    }
    pub fn render_all(&self, window: &mut Window, cx: &mut App) -> Vec<AnyElement> {
        self.host
            .update(cx, |host, cx| {
                self.ids
                    .iter()
                    .filter_map(|id| host.render_child(*id, window, cx))
                    .collect()
            })
            .unwrap_or_default()
    }
    /// The entity behind a child registered as a view.
    pub fn view(&self, index: usize, cx: &App) -> Option<AnyView> {
        let id = *self.ids.get(index)?;
        self.host.read_with(cx, |host, _| host.view(id)).ok().flatten()
    }
}

impl Host {
    pub fn new(registry: Registry, events: EventSink) -> Self {
        let tables = registry.tables();
        Self {
            decoder: Decoder::new(registry),
            nodes: Vec::new(),
            live: 0,
            first_root: NONE,
            last_root: NONE,
            root_count: 0,
            tables,
            events,
            retiring: Vec::new(),
            this: None,
            sequence: 0,
            frame: 0,
            last_subscription: 0,
            last_request: 0,
        }
    }

    pub fn registry(&self) -> &Registry {
        self.decoder.registry()
    }
    pub fn len(&self) -> usize {
        self.live
    }
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }
    pub fn roots(&self) -> Vec<u32> {
        self.siblings(self.first_root)
    }
    fn siblings(&self, mut id: u32) -> Vec<u32> {
        let mut ids = Vec::new();
        while id != NONE {
            ids.push(id);
            id = self.nodes[id as usize].next_sibling;
        }
        ids
    }
    fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.get(id as usize).filter(|node| !node.is_empty())
    }
    fn existing(&self, id: u32) -> Result<&Node> {
        self.node(id).ok_or_else(|| anyhow!("unknown view {id}"))
    }
    pub fn children(&self, id: u32) -> Option<Vec<u32>> {
        self.node(id).map(|node| self.siblings(node.first_child))
    }
    /// The entity of a node registered as a view.
    pub fn view(&self, id: u32) -> Option<AnyView> {
        let node = self.node(id)?;
        self.tables[node.kind() as usize].view(node.slot)
    }
    /// Mutate a host-owned element and its kind's extras, for example from a
    /// paint callback. The closure receives the row, the extras, and the slot.
    pub fn update_element<T: ReactElement, R>(
        &mut self,
        id: u32,
        f: impl FnOnce(&mut T, &mut T::Extras, u32) -> R,
    ) -> Option<R> {
        let node = *self.node(id)?;
        let (row, extras) = self.tables[node.kind() as usize].element_mut(node.slot)?;
        let row = row.downcast_mut::<T>()?;
        let extras = extras.downcast_mut::<T::Extras>()?;
        Some(f(row, extras, node.slot))
    }
    fn emitter(&self, id: u32, generation: u16) -> Emitter {
        Emitter::new(
            self.this.clone().expect("host renders only as an entity"),
            id,
            generation,
        )
    }
    /// The sink and subscription for a listener created for `(id, generation)`,
    /// or none if that node is gone and its events were already retired.
    pub(crate) fn route(&self, id: u32, generation: u16) -> Option<(EventSink, u64)> {
        let subscription = match self.node(id) {
            Some(node) if node.generation == generation => node.subscription,
            _ => self
                .retiring
                .iter()
                .find(|(retired, life, _)| *retired == id && *life == generation)
                .map(|(_, _, subscription)| *subscription)
                .unwrap_or(0),
        };
        (subscription != 0).then(|| (self.events.clone(), subscription as u64))
    }

    /// Decode transaction text with this host's session state. Production
    /// hosts decode on the worker with their own `Decoder`.
    pub fn decode(&mut self, json: &str) -> Result<Prepared> {
        self.decoder.parse(json)
    }
    /// The binary wire, decoded with this host's session state.
    pub fn decode_binary(&mut self, bytes: &[u8]) -> Result<Prepared> {
        self.decoder.parse_binary(bytes)
    }

    /// Decode and apply in one step. Production hosts decode on the worker and
    /// call `apply_prepared`.
    pub fn apply(
        &mut self,
        transaction: Transaction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Reply> {
        let prepared = self.decoder.parse(&transaction.0)?;
        self.apply_prepared(prepared, window, cx)
    }

    /// Apply committed React changes. A failure leaves the tree partially
    /// updated and must end the session; the reconciler never produces one.
    pub fn apply_prepared(
        &mut self,
        prepared: Prepared,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Reply> {
        ensure!(
            prepared.sequence == self.sequence + 1,
            "transaction sequence is out of order"
        );
        self.this = Some(cx.weak_entity());
        // Effects queued before the previous transaction returned have run.
        self.retiring.clear();
        // Size the node vector and each kind's rows once, from the
        // transaction's own counts, rather than growing by doubling.
        let mut highest = None;
        let mut creates = vec![0usize; self.tables.len()];
        for operation in &prepared.operations {
            if let Op::Create { id, kind, .. } = operation {
                highest = Some(highest.map_or(*id, |h: u32| h.max(*id)));
                creates[*kind as usize] += 1;
            }
        }
        if let Some(highest) = highest
            && (highest as usize) < self.nodes.len() + 65_536
        {
            let needed = highest as usize + 1;
            if needed > self.nodes.len() {
                self.nodes.reserve_exact(needed - self.nodes.len());
            }
        }
        for (kind, count) in creates.into_iter().enumerate() {
            if count > 0 {
                self.tables[kind].reserve(count);
            }
        }
        let mut reply = Reply {
            sequence: prepared.sequence,
            retired: vec![],
            results: vec![],
        };
        let mut dirty = FxHashSet::default();
        let mut created = Vec::new();
        let mut changed = false;
        for operation in prepared.operations {
            match operation {
                Op::Create {
                    id,
                    kind,
                    props,
                    subscription,
                    place,
                } => {
                    ensure!(id < DETACHED, "host ID out of range");
                    let index = id as usize;
                    ensure!(
                        index <= self.nodes.len() + 65_536,
                        "host ID {id} skips too far ahead"
                    );
                    if index >= self.nodes.len() {
                        self.nodes.resize(index + 1, Node::EMPTY);
                    }
                    ensure!(self.nodes[index].is_empty(), "host ID {id} is in use");
                    let capabilities = self.decoder.registry().binding(kind).capabilities();
                    if let Some(subscription) = subscription {
                        increasing(subscription, &mut self.last_subscription, "subscription")?;
                        ensure!(
                            capabilities.events,
                            "{} has no events",
                            self.decoder.registry().binding(kind).name()
                        );
                    }
                    let generation = self.nodes[index].generation;
                    let emitter = capabilities.events.then(|| self.emitter(id, generation));
                    let mut context = ElementContext {
                        id,
                        slot: 0,
                        child_count: 0,
                        window,
                        cx,
                    };
                    let slot = self.tables[kind as usize].create(props, emitter, &mut context)?;
                    self.nodes[index] = Node {
                        subscription: subscription.unwrap_or(0) as u32,
                        slot,
                        kind,
                        generation,
                        ..Node::EMPTY
                    };
                    self.live += 1;
                    created.push(id);
                    if let Some((parent, before)) = place {
                        self.place(parent, id, before, &mut dirty)?;
                        changed = true;
                    }
                }
                Op::Props { id, kind, props } => {
                    let node = self.checked(id, kind)?;
                    let mut context = ElementContext {
                        id,
                        slot: node.slot,
                        child_count: node.child_count as usize,
                        window,
                        cx,
                    };
                    self.tables[kind as usize].set_props(node.slot, props, &mut context)?;
                    self.invalidate(id, window, cx);
                    changed = true;
                }
                Op::Listen { id, subscription } => {
                    if let Some(subscription) = subscription {
                        increasing(subscription, &mut self.last_subscription, "subscription")?;
                    }
                    let node = *self.existing(id)?;
                    let capabilities = self.decoder.registry().binding(node.kind()).capabilities();
                    ensure!(
                        capabilities.events || subscription.is_none(),
                        "view has no events"
                    );
                    if node.subscription != 0 {
                        reply.retired.push(node.subscription as u64);
                    }
                    let subscription = subscription.unwrap_or(0) as u32;
                    if capabilities.view {
                        // Entity events travel through GPUI's ordered effect
                        // queue. Update the route in that same queue so earlier
                        // native commands retain their callback.
                        let host = self.this.clone().unwrap();
                        let generation = node.generation;
                        cx.defer(move |cx| {
                            host.update(cx, |host, _| {
                                if let Some(node) = host.nodes.get_mut(id as usize)
                                    && node.generation == generation
                                    && !node.is_empty()
                                {
                                    node.subscription = subscription;
                                } else if let Some(entry) = host
                                    .retiring
                                    .iter_mut()
                                    .find(|(retired, life, _)| *retired == id && *life == generation)
                                {
                                    // Removed before this write ran: later
                                    // events still queued use the new route.
                                    entry.2 = subscription;
                                }
                            })
                            .ok();
                        });
                    } else {
                        self.nodes[id as usize].subscription = subscription;
                    }
                    changed = true;
                }
                Op::Place {
                    parent,
                    child,
                    before,
                } => {
                    self.place(parent, child, before, &mut dirty)?;
                    changed = true;
                }
                Op::Remove { id } => {
                    self.remove(id, &mut dirty, &mut reply.retired, window, cx)?;
                    changed = true;
                }
                Op::Hidden { id, hidden } => {
                    let node = self.existing(id)?;
                    if node.hidden() != hidden {
                        let parent = node.parent;
                        self.nodes[id as usize].kind ^= HIDDEN;
                        dirty.insert(parent);
                        changed = true;
                    }
                }
                Op::Call {
                    id,
                    kind,
                    request,
                    command,
                    value,
                } => {
                    increasing(request, &mut self.last_request, "request")?;
                    // A command may depend on children placed earlier in this
                    // transaction, such as a scroll anchor after row supply.
                    self.flush(&mut dirty, window, cx);
                    let result = value.and_then(|value| {
                        let node = self.checked(id, kind)?;
                        let mut context = ElementContext {
                            id,
                            slot: node.slot,
                            child_count: node.child_count as usize,
                            window,
                            cx,
                        };
                        let table = &mut self.tables[kind as usize];
                        if command {
                            table.command(node.slot, value, &mut context)?;
                            Ok(serde_json::Value::Null)
                        } else {
                            table.query(node.slot, value, &mut context)
                        }
                    });
                    if command {
                        // A command may change state before returning an error.
                        self.invalidate(id, window, cx);
                        changed = true;
                    }
                    reply.results.push(CallResult::new(request, result));
                }
            }
        }
        for id in created {
            if let Some(node) = self.node(id) {
                ensure!(node.parent != DETACHED, "new view {id} was not attached");
            }
        }
        self.flush(&mut dirty, window, cx);
        if changed {
            cx.notify();
        }
        self.sequence = prepared.sequence;
        Ok(reply)
    }

    /// Explicit lifecycle cleanup while the GPUI window and app still exist.
    pub fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for id in 0..self.nodes.len() as u32 {
            let node = self.nodes[id as usize];
            if node.is_empty() {
                continue;
            }
            self.nodes[id as usize] = Node {
                generation: node.generation.wrapping_add(1),
                ..Node::EMPTY
            };
            self.live -= 1;
            let mut context = ElementContext {
                id,
                slot: node.slot,
                child_count: node.child_count as usize,
                window,
                cx,
            };
            self.tables[node.kind() as usize].remove(node.slot, &mut context);
        }
        self.first_root = NONE;
        self.last_root = NONE;
        self.root_count = 0;
        cx.notify();
    }

    fn checked(&self, id: u32, kind: u16) -> Result<Node> {
        let node = *self.existing(id)?;
        ensure!(node.kind() == kind, "component mismatch for view {id}");
        Ok(node)
    }

    /// The head, tail, and count of a sibling list: a parent's children or the roots.
    fn list_mut(&mut self, parent: u32) -> (&mut u32, &mut u32, &mut u32) {
        if parent == ROOT {
            (
                &mut self.first_root,
                &mut self.last_root,
                &mut self.root_count,
            )
        } else {
            let node = &mut self.nodes[parent as usize];
            (
                &mut node.first_child,
                &mut node.last_child,
                &mut node.child_count,
            )
        }
    }

    fn detach(&mut self, id: u32, dirty: &mut FxHashSet<u32>) {
        let node = self.nodes[id as usize];
        if node.parent == DETACHED {
            return;
        }
        let (prev, next) = (node.prev_sibling, node.next_sibling);
        if prev != NONE {
            self.nodes[prev as usize].next_sibling = next;
        }
        if next != NONE {
            self.nodes[next as usize].prev_sibling = prev;
        }
        let (first, last, count) = self.list_mut(node.parent);
        if *first == id {
            *first = next;
        }
        if *last == id {
            *last = prev;
        }
        *count -= 1;
        dirty.insert(node.parent);
        let node = &mut self.nodes[id as usize];
        node.parent = DETACHED;
        node.prev_sibling = NONE;
        node.next_sibling = NONE;
    }

    fn place(
        &mut self,
        parent: Option<u32>,
        child: u32,
        before: Option<u32>,
        dirty: &mut FxHashSet<u32>,
    ) -> Result<()> {
        let target = match parent {
            Some(id) => {
                let node = self.existing(id)?;
                ensure!(
                    self.decoder
                        .registry()
                        .binding(node.kind())
                        .capabilities()
                        .children,
                    "parent does not accept children"
                );
                id
            }
            None => ROOT,
        };
        self.existing(child)?;
        let mut ancestor = target;
        while ancestor != ROOT && ancestor != DETACHED {
            ensure!(ancestor != child, "child insertion would create a cycle");
            ancestor = self.existing(ancestor)?.parent;
        }
        if before == Some(child) {
            return Ok(());
        }
        if let Some(anchor) = before {
            ensure!(
                self.existing(anchor)?.parent == target,
                "insertion anchor is not a child of this parent"
            );
        }
        self.detach(child, dirty);
        let (prev, next) = match before {
            Some(anchor) => (self.nodes[anchor as usize].prev_sibling, anchor),
            None => (*self.list_mut(target).1, NONE),
        };
        if prev != NONE {
            self.nodes[prev as usize].next_sibling = child;
        }
        if next != NONE {
            self.nodes[next as usize].prev_sibling = child;
        }
        {
            let (first, last, count) = self.list_mut(target);
            if prev == NONE {
                *first = child;
            }
            if next == NONE {
                *last = child;
            }
            *count += 1;
        }
        let node = &mut self.nodes[child as usize];
        node.parent = target;
        node.prev_sibling = prev;
        node.next_sibling = next;
        dirty.insert(target);
        Ok(())
    }

    fn remove(
        &mut self,
        id: u32,
        dirty: &mut FxHashSet<u32>,
        retired: &mut Vec<u64>,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<()> {
        self.existing(id)?;
        self.detach(id, dirty);
        // Release descendants before ancestors so a parent never sees a
        // dangling child.
        let mut order = vec![id];
        let mut index = 0;
        while index < order.len() {
            let mut child = self.nodes[order[index] as usize].first_child;
            while child != NONE {
                order.push(child);
                child = self.nodes[child as usize].next_sibling;
            }
            index += 1;
        }
        for id in order.into_iter().rev() {
            let node = self.nodes[id as usize];
            self.nodes[id as usize] = Node {
                generation: node.generation.wrapping_add(1),
                ..Node::EMPTY
            };
            self.live -= 1;
            if node.subscription != 0 {
                retired.push(node.subscription as u64);
                // Entity events already queued keep their subscription until
                // the effects run; element listeners of the last frame too.
                self.retiring
                    .push((id, node.generation, node.subscription));
            }
            let mut context = ElementContext {
                id,
                slot: node.slot,
                child_count: node.child_count as usize,
                window,
                cx,
            };
            self.tables[node.kind() as usize].remove(node.slot, &mut context);
        }
        Ok(())
    }

    /// Tell the nearest enclosing view with children that one of its direct
    /// children changed inside. Only views cache child geometry.
    fn invalidate(&self, id: u32, window: &mut Window, cx: &mut App) {
        let mut child = id;
        let mut parent = match self.node(id) {
            Some(node) => node.parent,
            None => return,
        };
        while parent != ROOT && parent != DETACHED {
            let id = parent;
            let node = self.nodes[id as usize];
            let capabilities = self.decoder.registry().binding(node.kind()).capabilities();
            if capabilities.view && capabilities.children {
                self.tables[node.kind() as usize].child_changed(node.slot, child, window, cx);
                return;
            }
            child = id;
            parent = node.parent;
        }
    }

    /// Deliver changed child lists to views. Elements read the tree directly.
    fn flush(&self, dirty: &mut FxHashSet<u32>, window: &mut Window, cx: &mut App) {
        for id in dirty.drain() {
            if id == ROOT || id == DETACHED {
                continue;
            }
            let Some(node) = self.node(id) else {
                continue;
            };
            let capabilities = self.decoder.registry().binding(node.kind()).capabilities();
            if capabilities.view && capabilities.children {
                let children = Children {
                    host: self.this.clone().expect("set before apply"),
                    ids: self.visible_children(node.first_child).into(),
                };
                self.tables[node.kind() as usize].set_children(node.slot, children, window, cx);
            }
        }
    }

    fn visible_children(&self, mut id: u32) -> Vec<u32> {
        let mut ids = Vec::new();
        while id != NONE {
            let node = &self.nodes[id as usize];
            if !node.hidden() {
                ids.push(id);
            }
            id = node.next_sibling;
        }
        ids
    }

    fn render_child(&self, id: u32, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let node = *self.node(id)?;
        if node.hidden() {
            return None;
        }
        Some(self.tables[node.kind() as usize].render(
            node.slot,
            &mut RenderContext {
                id,
                slot: node.slot,
                window,
                cx,
                host: self,
                node,
            },
        ))
    }
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.frame += 1;
        if self.this.is_none() {
            self.this = Some(cx.weak_entity());
        }
        let info = Rc::new(FrameInfo {
            root: cx.entity_id().as_u64(),
            frame: self.frame,
            commit: self.sequence,
            viewport_width: window.viewport_size().width.into(),
            viewport_height: window.viewport_size().height.into(),
            scale_factor: window.scale_factor(),
        });
        let mut roots = Vec::with_capacity(self.root_count as usize);
        let mut id = self.first_root;
        while id != NONE {
            if let Some(element) = self.render_child(id, window, cx) {
                roots.push(element);
            }
            id = self.nodes[id as usize].next_sibling;
        }
        crate::frame::FrameScope {
            child: div().size_full().children(roots).into_any_element(),
            info,
        }
    }
}

fn increasing(value: u64, previous: &mut u64, name: &str) -> Result<()> {
    ensure!(
        value > *previous && value < (1 << 32),
        "{name} must increase and fit in 32 bits"
    );
    *previous = value;
    Ok(())
}

#[cfg(test)]
mod layout {
    #[test]
    fn node_is_thirty_six_bytes_of_integers() {
        assert_eq!(std::mem::size_of::<super::Node>(), 36);
        assert_eq!(std::mem::align_of::<super::Node>(), 4);
    }
}
