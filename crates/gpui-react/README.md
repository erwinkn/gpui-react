# React bindings for GPUI

This crate is the engine: the host data structures, element rebuilding, the
transaction decoder, the component registry, the wire, and the component
traits. It registers no component kinds and depends on GPUI, Serde, and
anyhow. [`gpui-react-kit`](../gpui-react-kit/README.md) supplies the five
standard controls and their `Style`;
[`gpui-react-runtime`](../gpui-react-runtime/README.md) supplies the native
loop, the N-API worker transport, and the default composition of engine and
kit. An application that wants none of the standard controls builds its own
`Registry` against this crate alone.

## One host, one tree

GPUI retains entities and per-element state, not an element tree. Elements are
rebuilt every frame from entity state. The bridge therefore keeps the committed
React tree as plain data inside one `Host` entity per window root, and rebuilds
GPUI elements from it on every draw. There is no second native tree and no
entity per React node.

A node is one of two kinds:

| Kind | Trait | Registration | What it is |
| --- | --- | --- | --- |
| Host-owned element | `ReactElement` | `HostElement::<T>::new(name)` | Plain data rendered by the host into ordinary GPUI elements. No entity, no persistent element. Used for divs and text. |
| Entity-backed view | `ReactView` | `Component::<T>::new(name)` | An ordinary `impl gpui::Render` entity for anything with native state: inputs, lists, animations, GPU resources, or code that needs layout. |

Both kinds implement `create` and `set_props` with a deserializable `Props`
type. `set_props` receives complete validated props; preserve native state
unless the contract replaces it. Optional capabilities are opt-in at
registration:

| Capability | View trait | Element access | Registration |
| --- | --- | --- | --- |
| Events | `ReactEvents` (a GPUI `EventEmitter`) | `RenderContext::emitter()` | `.events()` |
| Commands | `ReactCommands` | `ElementCommands` | `.commands()` |
| Queries | `ReactQueries` | `ElementQueries` | `.queries()` |
| Children | `ReactChildren` | `RenderContext::children()` | `.children()` |

```rust,ignore
registry.register(Component::<Counter>::new("counter").events().commands().queries())?;
registry.register(HostElement::<Container>::new("container").children().events().commands().queries())?;
```

A host-owned element renders with `RenderContext`. `element_id()` is the
node's stable GPUI `ElementId`, so hover, active, and scroll state that GPUI
keeps by element id persist across frames without bridge storage. `children()`
builds the visible children now. `emitter()` is present only while JavaScript
listens, so an element installs listeners only when someone would receive them.
`host()` returns the host entity for paint callbacks that write back through
`Host::update_element`.

A view receives its React children as a `Children` handle through
`ReactChildren::set_children`, at mount and whenever the ordered list of visible
children changes. The handle renders children on demand from the host tree:
`render(index, window, cx)` builds one child, `render_all` builds every child,
`ids()` lists their node ids, and `view(index, cx)` returns the entity behind a
child registered as a view. A virtual list builds only its visible rows. Call
these from the view's own render or layout, never while the host is being
updated; GPUI renders views during layout, after the host's render has
returned, so this holds in ordinary use. `child_changed(id)` tells the nearest
enclosing view that a prop change or command touched a node inside that direct
child. Only views cache child geometry, so elements are walked through.

## Storage

A node is a 36-byte record of `u32` and `u16` fields in one vector indexed by
host id: parent, sibling links, child count, subscription, the row slot, and
the kind with a hidden bit. Each registered kind owns a vector of its own row
type behind one table object, so a text node is its record plus an 80-byte row
plus its string, and a plain container row is 32 bytes. Event listeners capture
the host handle, the node id, and the record's generation, and read the
subscription from the record when they fire; a retiring list keeps the
subscription of a removed node until the effects queued before its removal
have run. A layout test pins the node size.

## Transactions

A `Decoder` turns transaction text into typed operations. The structural pass
borrows each props value as a slice of the input, resolves component names to
registry indices without allocating, and decodes each slice into the
component's typed props with static serde as soon as it is read; no value tree
exists. A single pass through type erasure was measured slower, because it
boxes intermediate values. Shared values are defined once on the wire and
referenced by id; the decoder owns those definitions for its session, so a
`Shared<T>` prop already holds its `Arc<T>` when it reaches the UI thread, and
definition operations never cross to the UI. The engine does not know `T`: the
registry declares the session's one shared definition type with
`registry.shared::<T>()`, where `T: Deserialize + SharedDefinition` supplies
the default value a `Shared<T>` field falls back to. The wire spells the
operations `style` and `dropStyle` and the schema type `WireType::Style`,
after their one use today. In the native host the decoder runs on the
application worker; the UI thread parses no JSON.

Host ids are `u32` and dense: the reconciler reuses an id once the transaction
that removed its node has been acknowledged, so the host stores nodes in a
vector indexed by id rather than a hash map. Subscription ids are never
reused, which is what keeps late events from reaching a replacement node. `Host::apply_prepared` applies them in order:
create mounts an instance, props and commands reach the instance, place and
remove edit the child lists, hidden removes a branch from rendering while its
state stays mounted. Commands and queries run after pending child lists have
been delivered to views, so a scroll anchor sees rows placed earlier in the
same transaction. `Host::apply` decodes and applies in one step for tests.

Operations name their target's component so the worker can decode them. Host
IDs, subscription IDs, and request IDs must increase within a session, which
rejects reuse without tombstones. Unknown ids, cycles, invalid anchors,
unsupported child slots, unplaced new nodes, and schema failures in props fail
the transaction, and the session must end; the reconciler never produces them.
An invalid command or query value returns a request error and never invokes
native code. A native command error does not roll back state it already
changed.

Events travel through an `Emitter` that carries the node id and its current
subscription. Entity events use GPUI's ordered effect queue, so subscription
replacement and retirement are deferred into that queue: an event emitted
before a `listen` keeps its old subscription, and a removed view stays alive
until its pending events have run. Element listeners fire synchronously from
the current frame and stop immediately on removal. Dropping the host stops
delivery for views a frame still retains. The event sink must enqueue without
waiting for JavaScript and fail explicitly on overflow.

`Host::clear` runs native cleanup while the window still exists. Subtree
removal releases descendants before ancestors and retires their subscriptions
in the reply.

## Frame metadata

A native component can call `current_frame(window, cx)` during paint. Below a
`Host`, it returns `FrameInfo` with the host identity, draw number, incorporated
transaction, viewport size, and scale factor, including through ordinary GPUI
deferred elements and nested hosts. It returns `None` outside that draw scope.
Record measurements during paint; earlier phases do not prove that an element
will be painted, and replayed paint does not run callbacks again. These records
identify native draw work, not physical presentation. Recording geometry costs
a paint callback per node per frame, so the kit's controls make it opt-in.

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo test -p gpui-react --release
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy -p gpui-react --release --all-targets -- -D warnings
```

The tests use GPUI's test application and real entity and subscription
machinery with kinds defined locally. They cover registration, typed events,
effect-ordered callback replacement, removal, hidden branches, invalid
operations, element children, change notification to the nearest view, shared
value definition and resolution, and frame scopes through nested and deferred
hosts.
