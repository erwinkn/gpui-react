# Native controls

This crate contains the standard controls. `Container` and `Text` are host-owned
data rendered into ordinary
GPUI elements each frame; equal styles share one value per app through
`shared_style`. `VirtualList`, `Document`, and `Input` are ordinary GPUI
entities. One input entity owns its text, selection,
composition, undo history, scroll position, and caret. GPUI computes its layout
and handles keyboard, mouse, clipboard, and platform input-method operations.

Use `Input::new(props, cx)` in a normal GPUI application. Render the entity as a
child and obtain its `Focusable` handle through GPUI. Native code can use
`update_props`, `apply_command`, `current_snapshot`, and `InputEvent` directly.
The React binding implements the standard optional bridge traits; it does not
replace the control's `Render` implementation.

For React, call `gpui_react_controls::register(registry)` from the composition's
registration function. This registers `input`, `container`, `text`, `list`, and `document` with their supported capabilities.
Import its wrapper from `@gpui-react/controls`. See the
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
cargo run --manifest-path crates/gpui-react-controls/Cargo.toml --release --example selection_toolbar_visual
cargo run --manifest-path crates/gpui-react-controls/Cargo.toml --release --example geometry_visual
cargo run --manifest-path crates/gpui-react-controls/Cargo.toml --release --example modal_visual
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

Use `Document::new(props, cx)` in native GPUI code. `set_native_children`
composes native views ahead of any React children; `ReactView::set_props`
updates the props.
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


Native components can read `Document::selected_range(key, text)` before paint.
It returns UTF-8 byte offsets from the native selection, or `None` if the key
is not selected or those selected bytes no longer match the supplied text.
Unselected bytes can change without invalidating that range. It does not compute
layout or change the retained selection snapshot used by copy. JavaScript
selection commands and reported ranges continue to use UTF-16 offsets.

`DocumentText::layout()` exposes the underlying GPUI `TextLayout`. Clone the
handle before moving the text into its parent, then read its positions after
that text has completed prepaint. The clone shares GPUI's layout; it does not
shape the text again. Reading positions before prepaint violates GPUI's API.

The [selection-toolbar example](./examples/selection_toolbar_visual.rs) is a
normal native view with a thin `ReactView` binding. A canvas after the text reads
its current layout and defers an ordinary anchored button to the selected range's
end. GPUI performs the toolbar layout and hit testing. The example checks every
draw, including the first one after a width or font change. It also checks native
double-click selection, button clicks, clipboard, changed source, and clearing.
It saves `/tmp/bridge-selection-toolbar.png`. This is a worked native composition,
not a new built-in React control or a general menu implementation.


Two further native compositions exercise geometry and interaction boundaries.
[The menu and connector example](./examples/geometry_visual.rs) uses GPUI's
`anchored().match_parent_width()` and current painted card bounds. It checks
every draw during resize, native click/Enter selection events, Escape, focus
return, outside dismissal, and connector pixels at their expected coordinates.
The window-pixel conversion comes from actual screenshot and viewport sizes.

[The nested-modal example](./examples/modal_visual.rs) uses normal deferred GPUI
layers and focus handles. It checks that the top backdrop blocks mouse delivery
to a covered button, Tab remains in the active dialog, Escape restores the prior
focus, and overflowing content stays clipped. It also reads GPUI's accessibility
tree to check dialog roles, active modality, disabled underlying content, focus,
and removal. Each fixture dialog has one focus owner; this is not a general
multi-control focus-trap implementation. The examples save
`/tmp/bridge-native-{geometry,menu,modals}.png`. All windows remain off screen.
