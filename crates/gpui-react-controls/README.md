# Native controls

This crate contains ordinary GPUI controls. It does not depend on the old GPUiX
renderer. Controls include `Container`, `Text`, `VirtualList`, and `Input`. One input entity owns its text, selection,
composition, undo history, scroll position, and caret. GPUI computes its layout
and handles keyboard, mouse, clipboard, and platform input-method operations.

Use `Input::new(props, cx)` in a normal GPUI application. Render the entity as a
child and obtain its `Focusable` handle through GPUI. Native code can use
`update_props`, `apply_command`, `current_snapshot`, and `InputEvent` directly.
The React binding implements the standard optional bridge traits; it does not
replace the control's `Render` implementation.

For React, call `gpui_react_controls::register(registry)` from the composition's
registration function. This registers `input`, `container`, `text`, and `list` with their supported capabilities.
Import its wrapper from `@gpuix/bridge-controls`. See the
[JavaScript contract](../../packages/bridge-controls/README.md) for every prop,
event, command, and style field.

Input editing behavior was extracted from the existing native editor after its
renderer callbacks, duplicate prop buffer, and value-echo queue were removed.
Its GPUI and Comet source references remain in `src/input.rs`; the applicable
notice is in `THIRD_PARTY_NOTICES.md`.

The unit tests use real GPUI entities and its `EntityInputHandler` interface.
The separate example runs on the main OS thread with native Metal rendering,
offscreen windows, platform keyboard dispatch, and the installed platform IME
handler. It checks typing, selection, undo, composition, text pixels,
accessibility values, scroll consumption, ancestor callbacks, and boundary
chaining. It saves `/tmp/bridge-input.png`.

```sh
cargo test --manifest-path crates/gpui-react-controls/Cargo.toml --release
cargo run --manifest-path crates/gpui-react-controls/Cargo.toml --release --example input_visual
cargo run --manifest-path crates/gpui-react-controls/Cargo.toml --release --example container_visual
cargo run --manifest-path crates/gpui-react-controls/Cargo.toml --release --example list_visual
cargo clippy --manifest-path crates/gpui-react-controls/Cargo.toml --release --all-targets -- -D warnings
```

The GPU test is currently macOS-only. It does not measure physical display
latency. The [worker fixture](../../fixtures/bridge-counter/README.md) also checks
this input through React in source and relocated compiled applications.

The container scenario checks outer geometry, inherited layout changes, ordinary
click routing, explicit mouse blocking, independent axes, and 72 linked-scroll
frames. The list scenario checks 100,000 logical rows, a 50,000-row wheel delta,
distant data requests, atomic row-window/anchor changes, the short-to-overflow
prepend, native tail following, changed offscreen height, and keyed row moves.
All three scenarios keep windows offscreen. The frame tags are tested through
native paint, including a query inside the same transaction as a layout change.

Text selection/search services and broad performance comparisons are still in
progress. The current `Text` control is a native leaf with explicit text and
paint inspection. It does not yet replace the old document text services.
