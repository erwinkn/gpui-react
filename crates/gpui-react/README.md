# GPUI views exposed to React

This crate is the new integration's native component boundary. It depends on
GPUI, Serde, and anyhow. It does not depend on `gpuix-native`, its renderer, or
its retained tree. Native host transport integration is still in progress.

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
The event sink must enqueue without waiting for JavaScript and fail explicitly
on overflow. It receives serialization errors as well as successful events.

Tests use GPUI's test application and real entity/subscription machinery:

```sh
cargo test --manifest-path crates/gpui-react/Cargo.toml --release
cargo clippy --manifest-path crates/gpui-react/Cargo.toml --release --all-targets -- -D warnings
```

These tests currently validate binding behavior, not physical GPU presentation
or a complete worker-host application.
