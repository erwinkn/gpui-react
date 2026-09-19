use crate::{EventSink, MountOptions, MountedView, Prepared, Registry, protocol::*};
use anyhow::{Result, anyhow, ensure};
use gpui::{AnyView, Context, IntoElement, Render, Window, div, prelude::*};
use std::collections::{HashMap, HashSet};

const MAX_SAFE_ID: u64 = (1 << 53) - 1;

#[cfg(test)]
thread_local! {
    static LINK_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[derive(Clone)]
struct Links {
    component: String,
    // None means not attached yet; Some(None) means a root child.
    parent: Option<Option<u64>>,
    children: Vec<u64>,
    subscription: Option<u64>,
    hidden: bool,
}

struct Entry {
    links: Links,
    mounted: MountedView,
}

/// The single native host owner. Component props and interaction state live in
/// their GPUI entities. This index retains only identity, topology and routing.
pub struct Host {
    registry: Registry,
    entries: HashMap<u64, Entry>,
    roots: Vec<u64>,
    events: EventSink,
    sequence: u64,
    frame: u64,
    last_id: u64,
    last_subscription: u64,
    last_request: u64,
}

enum Action {
    Create {
        id: u64,
        component: String,
        props: Prepared,
        subscription: Option<u64>,
    },
    Props {
        id: u64,
        props: Prepared,
    },
    Listen {
        id: u64,
        subscription: Option<u64>,
    },
    Place {
        parent: Option<u64>,
        child: u64,
        before: Option<u64>,
    },
    Remove {
        id: u64,
        ids: Vec<u64>,
    },
    Hidden {
        id: u64,
        hidden: bool,
    },
    Call {
        id: u64,
        request: u64,
        kind: &'static str,
        value: Result<Prepared>,
    },
}

impl Host {
    pub fn new(registry: Registry, events: EventSink) -> Self {
        Self {
            registry,
            entries: HashMap::new(),
            roots: Vec::new(),
            events,
            sequence: 0,
            frame: 0,
            last_id: 0,
            last_subscription: 0,
            last_request: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn roots(&self) -> &[u64] {
        &self.roots
    }
    pub fn children(&self, id: u64) -> Option<&[u64]> {
        self.entries
            .get(&id)
            .map(|entry| entry.links.children.as_slice())
    }
    pub fn view(&self, id: u64) -> Option<&AnyView> {
        self.entries.get(&id).map(|entry| entry.mounted.view())
    }

    /// Validate the whole transaction before invoking a component. Validation
    /// copies only topology records touched by this transaction. It does not
    /// duplicate props, component state, or the complete mounted tree.
    pub fn apply(
        &mut self,
        transaction: Transaction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Reply> {
        ensure!(transaction.version == 1, "unsupported protocol version");
        ensure!(
            transaction.sequence == self.sequence + 1,
            "transaction sequence is out of order"
        );
        let mut draft = Draft::new(self);
        let mut last_id = self.last_id;
        let mut last_subscription = self.last_subscription;
        let mut last_request = self.last_request;
        let mut created = Vec::new();
        let mut actions = Vec::with_capacity(transaction.operations.len());
        for operation in transaction.operations {
            let action = match operation {
                Operation::Create {
                    id,
                    component,
                    props,
                    subscription,
                } => {
                    increasing(id, &mut last_id, "host ID")?;
                    if let Some(subscription) = subscription {
                        increasing(subscription, &mut last_subscription, "subscription")?;
                        ensure!(
                            self.registry.supports(&component, "events")?,
                            "{component} has no events"
                        );
                    }
                    let props = self.registry.prepare_props(&component, props)?;
                    draft.changed.insert(
                        id,
                        Some(Links {
                            component: component.clone(),
                            parent: None,
                            children: vec![],
                            subscription,
                            hidden: false,
                        }),
                    );
                    created.push(id);
                    Action::Create {
                        id,
                        component,
                        props,
                        subscription,
                    }
                }
                Operation::Props { id, props } => Action::Props {
                    id,
                    props: self
                        .registry
                        .prepare_props(&draft.get(id)?.component, props)?,
                },
                Operation::Listen { id, subscription } => {
                    if let Some(subscription) = subscription {
                        increasing(subscription, &mut last_subscription, "subscription")?;
                        ensure!(
                            self.registry
                                .supports(&draft.get(id)?.component, "events")?,
                            "view has no events"
                        );
                    }
                    draft.edit(id)?.subscription = subscription;
                    Action::Listen { id, subscription }
                }
                Operation::Place {
                    parent,
                    child,
                    before,
                } => {
                    if let Some(parent) = parent {
                        ensure!(
                            self.registry
                                .supports(&draft.get(parent)?.component, "children")?,
                            "parent does not accept children"
                        );
                    }
                    draft.place(parent, child, before)?;
                    Action::Place {
                        parent,
                        child,
                        before,
                    }
                }
                Operation::Remove { id } => Action::Remove {
                    id,
                    ids: draft.remove(id)?,
                },
                Operation::Hidden { id, hidden } => {
                    draft.edit(id)?.hidden = hidden;
                    Action::Hidden { id, hidden }
                }
                Operation::Command { id, request, value } => {
                    increasing(request, &mut last_request, "request")?;
                    let value = draft.get(id).and_then(|links| {
                        self.registry.prepare(&links.component, "command", value)
                    });
                    Action::Call {
                        id,
                        request,
                        kind: "command",
                        value,
                    }
                }
                Operation::Query { id, request, value } => {
                    increasing(request, &mut last_request, "request")?;
                    let value = draft
                        .get(id)
                        .and_then(|links| self.registry.prepare(&links.component, "query", value));
                    Action::Call {
                        id,
                        request,
                        kind: "query",
                        value,
                    }
                }
            };
            actions.push(action);
        }
        for id in created {
            if let Ok(links) = draft.get(id) {
                ensure!(links.parent.is_some(), "new view {id} was not attached");
            }
        }
        drop(draft);

        let mut reply = Reply {
            sequence: transaction.sequence,
            retired: vec![],
            results: vec![],
        };
        let mut dirty = HashSet::new();
        let mut changed = HashMap::new();
        for action in actions {
            match action {
                Action::Create {
                    id,
                    component,
                    props,
                    subscription,
                } => {
                    let mounted = self.registry.mount(
                        &component,
                        props,
                        MountOptions {
                            target: id,
                            subscription,
                            events: self.events.clone(),
                        },
                        window,
                        cx,
                    )?;
                    self.entries.insert(
                        id,
                        Entry {
                            links: Links {
                                component,
                                parent: None,
                                children: vec![],
                                subscription,
                                hidden: false,
                            },
                            mounted,
                        },
                    );
                }
                Action::Props { id, props } => {
                    self.entries[&id]
                        .mounted
                        .apply("props", props, window, cx)?;
                    self.changed_ancestors(id, &mut changed);
                }
                Action::Listen { id, subscription } => {
                    let entry = self.entries.get_mut(&id).unwrap();
                    if let Some(old) = entry.links.subscription {
                        reply.retired.push(old);
                    }
                    entry.links.subscription = subscription;
                    entry.mounted.set_subscription(subscription, cx)?;
                }
                Action::Place {
                    parent,
                    child,
                    before,
                } => {
                    if before == Some(child) {
                        continue;
                    }
                    self.changed_parent_ancestors(child, &mut changed);
                    self.detach(child, &mut dirty);
                    let children = self.child_ids_mut(parent);
                    let index = before
                        .map(|id| children.iter().position(|child| *child == id).unwrap())
                        .unwrap_or(children.len());
                    children.insert(index, child);
                    self.entries.get_mut(&child).unwrap().links.parent = Some(parent);
                    dirty.insert(parent);
                    self.changed_parent_ancestors(child, &mut changed);
                }
                Action::Remove { id, ids } => {
                    // Only this subtree's connection to surviving views changes.
                    // Its internal child lists are released with their owners.
                    self.changed_parent_ancestors(id, &mut changed);
                    self.detach(id, &mut dirty);
                    for id in ids {
                        let mut entry = self.entries.remove(&id).unwrap();
                        if let Some(subscription) = entry.links.subscription {
                            reply.retired.push(subscription);
                        }
                        if self.registry.supports(&entry.links.component, "children")? {
                            entry.mounted.set_children(vec![], window, cx)?;
                        }
                        entry.mounted.unmount(window, cx);
                    }
                }
                Action::Hidden { id, hidden } => {
                    self.changed_parent_ancestors(id, &mut changed);
                    let links = &mut self.entries.get_mut(&id).unwrap().links;
                    links.hidden = hidden;
                    if let Some(parent) = links.parent {
                        dirty.insert(parent);
                    }
                }
                Action::Call {
                    id,
                    request,
                    kind,
                    value,
                } => {
                    self.sync_children(&mut dirty, &mut changed, window, cx)?;
                    let mut invoked_command = false;
                    let result = value.and_then(|value| {
                        let mounted = &self
                            .entries
                            .get(&id)
                            .ok_or_else(|| anyhow!("unknown view {id}"))?
                            .mounted;
                        invoked_command = kind == "command";
                        mounted.apply(kind, value, window, cx)
                    });
                    // A native command may change state before returning an
                    // error. Errors are not rollback; dependent caches must
                    // still observe that invocation. Schema failures never run.
                    if invoked_command {
                        self.changed_ancestors(id, &mut changed);
                    }
                    reply.results.push(CallResult::new(request, result));
                }
            }
        }
        self.sync_children(&mut dirty, &mut changed, window, cx)?;
        self.sequence = transaction.sequence;
        self.last_id = last_id;
        self.last_subscription = last_subscription;
        self.last_request = last_request;
        Ok(reply)
    }

    fn child_ids_mut(&mut self, parent: Option<u64>) -> &mut Vec<u64> {
        match parent {
            Some(id) => &mut self.entries.get_mut(&id).unwrap().links.children,
            None => &mut self.roots,
        }
    }

    fn detach(&mut self, id: u64, dirty: &mut HashSet<Option<u64>>) {
        if let Some(parent) = self.entries.get_mut(&id).unwrap().links.parent.take() {
            self.child_ids_mut(parent).retain(|child| {
                #[cfg(test)]
                LINK_VISITS.with(|visits| visits.set(visits.get() + 1));
                *child != id
            });
            dirty.insert(parent);
        }
    }

    fn sync_children(
        &self,
        dirty: &mut HashSet<Option<u64>>,
        changed: &mut HashMap<u64, HashSet<u64>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        for parent in dirty.drain() {
            match parent {
                Some(id) => {
                    if let Some(entry) = self.entries.get(&id) {
                        let children = self.visible(&entry.links.children);
                        entry.mounted.set_children(children, window, cx)?;
                    }
                }
                None => cx.notify(),
            }
        }
        for (parent, children) in changed.drain() {
            if let Some(entry) = self.entries.get(&parent) {
                let affected: Vec<_> = entry
                    .links
                    .children
                    .iter()
                    .filter(|id| children.contains(id))
                    .filter_map(|id| {
                        self.entries
                            .get(id)
                            .map(|entry| entry.mounted.view().entity_id())
                    })
                    .collect();
                if !affected.is_empty() {
                    entry.mounted.children_changed(&affected, window, cx);
                }
            }
        }
        Ok(())
    }

    fn changed_parent_ancestors(&self, child: u64, changed: &mut HashMap<u64, HashSet<u64>>) {
        // set_children handles this immediate parent's topology. Ancestors can
        // still cache the size of the changed parent as one of their rows.
        if let Some(Some(Some(parent))) = self.entries.get(&child).map(|entry| entry.links.parent) {
            self.changed_ancestors(parent, changed);
        }
    }

    fn changed_ancestors(&self, mut child: u64, changed: &mut HashMap<u64, HashSet<u64>>) {
        while let Some(Some(Some(parent))) =
            self.entries.get(&child).map(|entry| entry.links.parent)
        {
            #[cfg(test)]
            LINK_VISITS.with(|visits| visits.set(visits.get() + 1));
            changed.entry(parent).or_default().insert(child);
            child = parent;
        }
    }

    fn visible(&self, ids: &[u64]) -> Vec<AnyView> {
        ids.iter()
            .filter_map(|id| {
                let entry = &self.entries[id];
                (!entry.links.hidden).then(|| entry.mounted.view().clone())
            })
            .collect()
    }

    /// Explicit lifecycle cleanup while the GPUI window and app still exist.
    pub fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<_> = self.entries.keys().copied().collect();
        for id in ids {
            let entry = self.entries.get_mut(&id).unwrap();
            if self
                .registry
                .supports(&entry.links.component, "children")
                .unwrap_or(false)
            {
                entry
                    .mounted
                    .set_children(vec![], window, cx)
                    .expect("registered child capability");
            }
            entry.mounted.unmount(window, cx);
        }
        self.roots.clear();
        self.entries.clear();
        cx.notify();
    }
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.frame += 1;
        crate::frame::FrameScope {
            child: div()
                .size_full()
                .children(self.visible(&self.roots))
                .into_any_element(),
            info: std::rc::Rc::new(crate::FrameInfo {
                root: cx.entity_id().as_u64(),
                frame: self.frame,
                commit: self.sequence,
                viewport_width: window.viewport_size().width.into(),
                viewport_height: window.viewport_size().height.into(),
                scale_factor: window.scale_factor(),
            }),
        }
    }
}

fn increasing(value: u64, previous: &mut u64, name: &str) -> Result<()> {
    ensure!(
        value > *previous && value <= MAX_SAFE_ID,
        "{name} must increase and be a safe positive integer"
    );
    *previous = value;
    Ok(())
}

/// A transaction-local overlay of changed links only. No native construction or
/// mutation is allowed while this validates topology and component schemas.
struct Draft<'a> {
    host: &'a Host,
    changed: HashMap<u64, Option<Links>>,
    roots: Option<Vec<u64>>,
}

impl<'a> Draft<'a> {
    fn new(host: &'a Host) -> Self {
        Self {
            host,
            changed: HashMap::new(),
            roots: None,
        }
    }
    fn get(&self, id: u64) -> Result<&Links> {
        match self.changed.get(&id) {
            Some(links) => links.as_ref(),
            None => self.host.entries.get(&id).map(|entry| &entry.links),
        }
        .ok_or_else(|| anyhow!("unknown view {id}"))
    }
    fn edit(&mut self, id: u64) -> Result<&mut Links> {
        if !self.changed.contains_key(&id) {
            self.changed.insert(id, Some(self.get(id)?.clone()));
        }
        self.changed
            .get_mut(&id)
            .unwrap()
            .as_mut()
            .ok_or_else(|| anyhow!("unknown view {id}"))
    }
    fn children(&mut self, parent: Option<u64>) -> Result<&mut Vec<u64>> {
        match parent {
            Some(id) => Ok(&mut self.edit(id)?.children),
            None => Ok(self.roots.get_or_insert_with(|| self.host.roots.clone())),
        }
    }
    fn detach(&mut self, child: u64) -> Result<()> {
        if let Some(parent) = self.edit(child)?.parent.take() {
            self.children(parent)?.retain(|id| *id != child);
        }
        Ok(())
    }
    fn place(&mut self, parent: Option<u64>, child: u64, before: Option<u64>) -> Result<()> {
        self.get(child)?;
        if let Some(before) = before {
            ensure!(
                self.children(parent)?.contains(&before),
                "insertion anchor is not a child of this parent"
            );
        }
        let mut ancestor = parent;
        while let Some(id) = ancestor {
            ensure!(id != child, "child insertion would create a cycle");
            ancestor = self.get(id)?.parent.flatten();
        }
        if before == Some(child) {
            return Ok(());
        }
        self.detach(child)?;
        let children = self.children(parent)?;
        let index = before
            .map(|id| children.iter().position(|child| *child == id).unwrap())
            .unwrap_or(children.len());
        children.insert(index, child);
        self.edit(child)?.parent = Some(parent);
        Ok(())
    }
    fn remove(&mut self, id: u64) -> Result<Vec<u64>> {
        self.get(id)?;
        self.detach(id)?;
        let mut todo = vec![id];
        let mut ids = vec![];
        while let Some(id) = todo.pop() {
            todo.extend(self.get(id)?.children.iter().copied());
            self.changed.insert(id, None);
            ids.push(id);
        }
        ids.reverse();
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Component, ReactChildren, ReactView};
    use gpui::{Empty, TestAppContext};
    use std::{cell::RefCell, sync::Arc};

    thread_local! {
        static UNMOUNTED: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    }

    struct View {
        id: u64,
        children: Vec<AnyView>,
    }
    impl Render for View {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            Empty
        }
    }
    impl ReactView for View {
        type Props = u64;
        fn create(id: u64, _: &mut Window, _: &mut Context<Self>) -> Self {
            Self {
                id,
                children: vec![],
            }
        }
        fn set_props(&mut self, _: u64, _: &mut Window, _: &mut Context<Self>) {}
        fn unmounting(&mut self, _: &mut Window, _: &mut Context<Self>) {
            assert!(
                self.children.is_empty(),
                "release child ownership before unmount"
            );
            UNMOUNTED.with(|ids| ids.borrow_mut().push(self.id));
        }
    }
    impl ReactChildren for View {
        fn set_children(&mut self, children: Vec<AnyView>, _: &mut Window, _: &mut Context<Self>) {
            self.children = children;
        }
    }

    #[gpui::test]
    fn subtree_removal_has_linear_link_work_and_preserves_survivors(cx: &mut TestAppContext) {
        for deep in [false, true] {
            let mut registry = Registry::default();
            registry
                .register(Component::<View>::new("view").children())
                .unwrap();
            let window = cx.add_window(|_, _| Host::new(registry, Arc::new(|_| {})));
            window
                .update(cx, |host, window, cx| {
                    let count = 128;
                    let mut operations = Vec::new();
                    for id in 1..=count {
                        operations.push(Operation::Create {
                            id,
                            component: "view".into(),
                            props: id.into(),
                            subscription: None,
                        });
                        operations.push(Operation::Place {
                            child: id,
                            parent: match id {
                                1 => None,
                                2 | 3 => Some(1),
                                _ if deep => Some(id - 2),
                                _ => Some(2),
                            },
                            before: None,
                        });
                    }
                    host.apply(
                        Transaction {
                            version: 1,
                            sequence: 1,
                            operations,
                        },
                        window,
                        cx,
                    )
                    .unwrap();
                    let survivor = host.view(3).unwrap().entity_id();
                    LINK_VISITS.with(|visits| visits.set(0));
                    UNMOUNTED.with(|ids| ids.borrow_mut().clear());
                    host.apply(
                        Transaction {
                            version: 1,
                            sequence: 2,
                            operations: vec![Operation::Remove { id: 2 }],
                        },
                        window,
                        cx,
                    )
                    .unwrap();
                    let visits = LINK_VISITS.with(|visits| visits.get());
                    assert!(
                        visits <= count as usize * 4,
                        "removing a subtree revisited {visits} links, deep={deep}"
                    );
                    assert_eq!(host.children(1), Some([3].as_slice()));
                    assert_eq!(host.view(3).unwrap().entity_id(), survivor);
                    let root = host.view(1).unwrap().clone().downcast::<View>().unwrap();
                    assert_eq!(root.read(cx).children[0].entity_id(), survivor);
                    UNMOUNTED.with(|ids| {
                        let ids = ids.borrow();
                        assert_eq!(ids.last(), Some(&2));
                        assert!(!ids.contains(&3));
                        if deep {
                            for pair in ids.windows(2) {
                                assert!(pair[0] > pair[1], "unmount descendants before ancestors");
                            }
                        }
                    });
                    host.clear(window, cx);
                })
                .unwrap();
        }
    }
}
