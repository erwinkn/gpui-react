# GPUI views exposed to React

This crate is the new integration's native component boundary. It depends on
GPUI, Serde, and anyhow. It does not depend on `gpuix-native`, its renderer, or
its retained tree. [`gpui-react-host`](../gpui-react-host/README.md) supplies a
native loop and worker transport separately.

A view keeps its ordinary `impl gpui::Render`. Implement `ReactView` with a
deserializable `Props` type, `create`, and `set_props`. `mounted` and `unmounting`
default to no-ops. Mount happens before initial layout, so it is not a geometry
notification. Preserve native state when applying props unless the component's
contract explicitly replaces it.

Optional traits and registration methods are:

| Trait | Registration | Contract |
| --- | --- | --- |
| `ReactEvents` | `.events()` | One serializable GPUI event type, delivered through the host queue. |
| `ReactCommands` | `.commands()` | Typed native operations with explicit errors. |
| `ReactQueries` | `.queries()` | Typed state observations with serializable replies. Does not force layout. |
| `ReactChildren` | `.children()` | Ordinary `AnyView` child handles for native composition. |

```rust,ignore
registry.register(
    Component::<Counter>::new("counter")
        .events()
        .commands()
        .queries(),
)?;
```

`Registry::prepare_props` decodes props before construction. `Registry::mount`
creates the GPUI entity using `MountOptions` for host identity and event routing.
The returned `MountedView` exposes its ordinary `AnyView`; `prepare` and `apply`
decode and execute prop changes, commands, and queries. Missing capabilities,
invalid data, duplicate names, and unknown components return errors.

`MountedView::unmount` retires events, releases subscriptions, and invokes the
window-aware cleanup hook once. The owner must call it while the app and window
are alive, then remove and release the view. A rendered frame can temporarily
retain a view handle. Dropping the binding still disables its event route.
Subscription replacement and unmount retirement follow GPUI's effect queue.
An event emitted before retirement keeps its old subscription even when removal
occurs in the same native transaction. The view stays alive until that event
has been processed. The event sink must enqueue without waiting for JavaScript and fail explicitly
on overflow. It receives serialization errors as well as successful events.

Tests use GPUI's test application and real entity/subscription machinery:

```sh
cargo test --manifest-path crates/gpui-react/Cargo.toml --release
cargo clippy --manifest-path crates/gpui-react/Cargo.toml --release --all-targets -- -D warnings
```

`Host` owns one native index of view identity, topology, visibility and event
routes. Component props and interaction state remain in the GPUI views. It
validates complete transactions before component construction or mutation,
using a temporary overlay of only affected topology records. Unknown parents,
cycles, invalid insertion anchors, unsupported child slots, invalid props,
reused IDs, and unplaced new views reject the transaction. Invalid commands or
queries return request errors while valid mutations remain committed.

Host IDs are allocated at React commit time and must increase within a session.
This rejects reuse without retaining deleted-node tombstones. Child views are
synchronized once per affected parent before a command/query and at transaction
end. Hidden children are omitted from layout while their native state remains
mounted. `Host::clear` runs native cleanup while the window still exists.

The local tests validate the real GPUI entity/subscription machinery and native
transaction ownership. The separate counter composition tests the worker-host
application. Neither certifies physical display performance.

Containers with native measurement caches can override
`ReactChildren::children_changed`. The default does nothing. The host reports
affected direct child entities after descendant prop, structure, or successful
command changes. It groups changes before the next command/query or transaction
end. A structural `set_children` already provides that parent's invalidation.
This lets a virtual list invalidate changed row heights before applying a scroll
anchor. It does not copy child props or create a second tree. Native changes
outside bridge transactions still use the component's ordinary GPUI cache APIs.

A native component can call `current_frame(window, cx)` during paint. Below a
`Host`, it returns `FrameInfo` with the host identity, draw number, incorporated
transaction, viewport size, and scale factor. It returns `None` outside that
paint scope. Nested hosts restore the enclosing scope. The scope delegates the
normal element lifecycle without adding a layout box. These records identify
native paint work, not physical OS presentation. A component should retain the
record with its measured bounds, rather than read it later during a query.
