# Native controls

This crate contains ordinary GPUI controls. It does not depend on the old GPUiX
renderer. Controls include `Container`, `Text`, `VirtualList`, `Document`, and `Input`. One input entity owns its text, selection,
composition, undo history, scroll position, and caret. GPUI computes its layout
and handles keyboard, mouse, clipboard, and platform input-method operations.

Use `Input::new(props, cx)` in a normal GPUI application. Render the entity as a
child and obtain its `Focusable` handle through GPUI. Native code can use
`update_props`, `apply_command`, `current_snapshot`, and `InputEvent` directly.
The React binding implements the standard optional bridge traits; it does not
replace the control's `Render` implementation.

For React, call `gpui_react_controls::register(registry)` from the composition's
registration function. This registers `input`, `container`, `text`, `list`, and `document` with their supported capabilities.
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
cargo run --manifest-path crates/gpui-react-controls/Cargo.toml --release --example document_visual
cargo run --manifest-path crates/gpui-react-controls/Cargo.toml --release --example deferred_document_visual
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
All scenarios keep windows offscreen. The frame tags are tested through
native paint, including a query inside the same transaction as a layout change.

`Document` owns selection, clipboard handling, search, and a registry of text
from its most recent paint. Native components contribute one complete logical
text at a time with `document_text(key, text)`. Keys must be unique and stable
inside the document, including across virtual row remounts. The helper uses
GPUI `StyledText` and its shaped line layouts. It performs no second text layout.

```rust
use gpui_react_controls::document_text;

// In an ordinary GPUI Render implementation:
div().child(document_text("message-42/body", "Hello reader"))
```

The helper accepts `.with_runs(Vec<gpui::TextRun>)` for styled text. Use
`.selectable(false)` for nonselectable content, `.searchable(false)` to exclude
chrome from search, and `.match_index_offset(n)` for a known global match base.
The defaults are selectable and searchable. Outside `Document`, the helper
renders ordinary text without a document hitbox or registry entry. It also
supplies a native accessibility label value.

Use `Document::new(props, cx)` in native GPUI code. `ReactChildren::set_children`
sets ordinary native view children; `ReactView::set_props` updates the props.
`apply_command`, `snapshot`, and `DocumentEvent` are also available directly.
`Search::new(SearchQuery { .. })` validates a query; its active index, colors,
and match offset are separate fields. These types live in `document`.

The document GPU example tests mixed native and React text, UTF-16 selection,
Unicode words and emoji, clipboard focus, nested documents, query changes,
wrapped geometry, selection after unmount, virtualized match indices, click
routing, and drag autoscroll through lists and nested containers. It saves
`/tmp/bridge-document.png`. Unit tests check shared source ownership, cache
identity, surrogate validation, and geometry beyond 256 lines.

Custom vertical scrollers can call `document::register_scroll_area` during
paint with their entity id, viewport bounds, and a callback. Positive distance
means down. The callback updates the existing native scroll state and returns
whether it moved. `Container` and `VirtualList` already register themselves.
The native drag clock uses 16 ms ticks and distance-based speed. It allows at
most half a viewport per completed document paint, preserving overlap between
virtualized selections. Release and unmount cancel the task.

Ordinary `gpui::deferred` text retains the nearest document scope. Nested
documents keep separate selection and search registries, including inside
floating views. GPUI's native `on_draw_complete` callback finalizes content
revisions, cache pruning, and search counts after all deferred painting, before
`Window::draw` returns. Search events retain the query and match offset used by
that paint, even if a native completion callback has already changed props.
The separate deferred-document GPU example checks text outside its parent's
layout box, nested scope isolation, double-click selection, clipboard, pixels,
frame tags, and removal. It saves `/tmp/bridge-deferred-document.png`.

The registry describes paint callbacks. GPUI cached view replay does not call
these callbacks, so content participating in document services must remain
uncached until metadata replay is implemented. This crate does not add a
cached-view workaround. Broad frame and memory comparisons remain in progress.

List count updates preserve measured rows and update only the changed native
index range. The [release measurements](../../docs/bridge-list-performance.md)
include direct GPUI costs and an allocation regression. Run the ignored
`list_count_update_cost` test separately to repeat the measurement.

The list reads inherited text metrics in its normal layout pass. A changed
font, text size, line height, wrapping mode, or line clamp invalidates measured
row heights before GPUI resolves the scroll anchor. The GPU regression changes
an ancestor font and applies a negative row anchor in the same transaction.
