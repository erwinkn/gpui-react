# GPUI React Bridge

GPUI React Bridge connects React 19 to [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui), the GPU UI framework from Zed.

## Architecture

One `Host` entity in GPUI manages one window root. The host keeps the committed React tree as plain data. The host rebuilds ephemeral GPUI elements from that tree on each frame.

Components with native state (text input, virtualized lists, GPU views) are ordinary GPUI entities. Ephemeral layout nodes (containers and text leaves) are plain host data with no entity overhead.

Application JavaScript runs in an explicit worker thread. A worker-side decoder turns transaction text into typed operations before they cross to the UI thread. The UI thread parses no JSON.

```
React (Worker Thread)  -->  Typed Transactions  -->  Host Entity (Main UI Thread)  -->  GPUI (Metal / Vulkan)
```

## Crates and Packages

### Rust Crates (`crates/`)

- `crates/gpui-react`: Core bridge crate. Provides the `Host` entity, dense node storage, transaction decoding, frame metadata (`current_frame`), and component traits (`ReactView`, `ReactElement`, `ReactEvents`, `ReactCommands`, `ReactQueries`, `ReactChildren`).
- `crates/gpui-react-controls`: Five standard native controls (`Document`, `VirtualList`, `Container`, `Text`, `Input`), style parsing, and registry installation.
- `crates/gpui-react-host`: Native application loop for macOS AppKit (`NSApplication::run`) and N-API worker communication channel.
- `crates/gpui-react-runtime`: Default native composition library that compiles the macOS arm64 `.node` binary for the standard controls.

### JavaScript Packages (`packages/`)

- `packages/bridge` (`@gpui-react/core`): React 19 reconciler for GPUI. Provides `createRoot`, `nativeComponent`, transaction transport, and application entry helpers (`runApplication`, `attachApplication`).
- `packages/bridge-controls` (`@gpui-react/controls`): Typed React wrappers and refs for the standard controls (`Document`, `List`, `Container`, `Text`, `Input`).
- `packages/bridge-runtime` (`@gpui-react/runtime`): Default runtime package that bundles the compiled native `.node` binary for macOS arm64.

## Standard Controls

The standard control library provides five native components:

1. **`Input`**: Native single-line and multiline text editor. Owns caret blinking, text selection, IME composition, drag autoscroll, and undo history. Supports replacement by expected revision.
2. **`Container`**: Host-owned layout element (GPUI `div`). Supports flex direction, alignment, gap, padding, borders, background, scrolling (`x`, `y`, `both`), focus handles, and mouse isolation (`blockMouse`).
3. **`Text`**: Host-owned text leaf. Connects text to the nearest parent `Document` for selection and search.
4. **`List` (`VirtualList`)**: Variable-height virtualized list powered by GPUI `ListState`. Supports bounded React child windows over large logical item counts (such as 100,000 items), missing row requests (`needRows`), and scroll anchors.
5. **`Document`**: Document text coordinator. Provides native selection across nodes, clipboard copy, search highlights, and painted geometry queries.

## Custom Component Traits

Developers can register custom native components with the bridge:

### Native GPUI Entities (`ReactView`)

Use `ReactView` for components that hold native state or resources:

- `ReactView`: Defines `create(props, window, cx)` and `set_props(props, window, cx)`.
- `ReactEvents`: Emits strongly typed events to React through `EventEmitter`.
- `ReactCommands`: Receives asynchronous commands from React refs.
- `ReactQueries`: Returns asynchronous query replies about native state to React refs.
- `ReactChildren`: Receives on-demand child view handles from the host tree.

### Host-Owned Elements (`ReactElement`)

Use `ReactElement` for stateless or element-id based elements that do not need an entity:

- `ReactElement`: Defines `create`, `set_props`, and `render(cx) -> AnyElement`.
- `ElementCommands`: Receives asynchronous commands from React refs.
- `ElementQueries`: Returns asynchronous query replies from React refs.

### Props and the Wire

Every `Props` type derives `Deserialize` and `ComponentProps`:

```rust
#[derive(Default, Deserialize, gpui_react::ComponentProps)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CounterProps {
    pub step: u32,
    pub label: Option<String>,
    pub style: SharedStyle,
}
```

`ComponentProps` reads the same serde attributes (`default`, `rename`,
`rename_all = "camelCase"`, `skip`) and declares the props schema to the
worker: field names, wire types, and which fields are required. Native is the
source of truth; the worker receives the kind table from `NativeClient.schema()`
at attach and encodes props by name from it. `Registry::verify_schemas()`
checks every derived schema against serde's view of the struct, and the
controls test suite runs it.

Wire types follow the Rust field type: `bool`; `i32`, `u32` (`usize` and
`u64` travel as `u32`); `f32`, `f64`; `String` and `SharedString`;
`SharedStyle` as an interned style id; anything else, including unit enums,
`Vec`, and nested structs, as a self-describing value. A field is required
unless the struct or the field has `#[serde(default)]` or the type is
`Option`. `flatten`, tagged enums, and other `rename_all` casings are
rejected at compile time.

Transactions cross as a binary payload written during React's commit: kind
indices, ids, a presence mask, then the declared fields in schema order, with no
keys. Props the schema does not declare are not sent; in development the
worker warns once per component and prop. Integers must be integers in range
and floats finite, or the root fails with the component and field name. A
missing required prop fails the root the same way. Native decodes positionally
into the props struct's own `Deserialize` derive, so defaults, renames, and
custom field deserializers apply unchanged.

`createRoot(transport, options)` accepts `schema` (the kind table),
`wire: "json" | "binary"` (default `"json"`; `attachApplication` uses
`"binary"`), and `wireChecks` (number checks, on by default). `Transport.send`
receives a string on the JSON wire and a `Uint8Array` on the binary wire that
is valid until the send settles. `decodeWire(bytes, schema)` rebuilds operation
objects for tests and tools. The native runtime version is 2.

## Building and Testing

### Prerequisites

- macOS arm64 (Apple Silicon)
- Rust toolchain (matches `zed/rust-toolchain.toml`)
- Bun 1.4+
- Xcode Metal toolchain (`xcodebuild -downloadComponent MetalToolchain`)

### Build Rust Crates

Use the standard flags for every Cargo command:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo test --manifest-path crates/gpui-react/Cargo.toml
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo test --manifest-path crates/gpui-react-controls/Cargo.toml
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo test --manifest-path crates/gpui-react-host/Cargo.toml
```

Check the runtime and fixtures:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo check --manifest-path crates/gpui-react-runtime/Cargo.toml
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo check --manifest-path fixtures/bridge-performance/Cargo.toml
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo check --manifest-path fixtures/bridge-performance/Cargo.toml --features allocation-counts
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo check --manifest-path fixtures/bridge-gpu-component/Cargo.toml
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo check --manifest-path fixtures/bridge-counter/Cargo.toml
```

Run Clippy:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --all-targets --all-features --manifest-path crates/gpui-react/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --all-targets --all-features --manifest-path crates/gpui-react-controls/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --all-targets --all-features --manifest-path crates/gpui-react-host/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --all-targets --all-features --manifest-path crates/gpui-react-runtime/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --all-targets --all-features --manifest-path fixtures/bridge-performance/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --all-targets --all-features --manifest-path fixtures/bridge-gpu-component/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --all-targets --all-features --manifest-path fixtures/bridge-counter/Cargo.toml -- -D warnings
```

### Build and Test JavaScript Packages

```sh
bun install
bun run --cwd packages/bridge build
bun run --cwd packages/bridge test
bun run --cwd packages/bridge-controls build
```

## Running Examples and Visual Fixtures

Run the visual examples. Windows open inactive and stay in the background:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo run --manifest-path crates/gpui-react-controls/Cargo.toml --example document_visual
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo run --manifest-path crates/gpui-react-controls/Cargo.toml --example container_visual
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo run --manifest-path crates/gpui-react-controls/Cargo.toml --example list_visual
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo run --manifest-path crates/gpui-react-controls/Cargo.toml --example geometry_visual
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo run --manifest-path crates/gpui-react-controls/Cargo.toml --example selection_toolbar_visual
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo run --manifest-path crates/gpui-react-controls/Cargo.toml --example deferred_document_visual
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo run --manifest-path fixtures/bridge-gpu-component/Cargo.toml --example visual
```

## Running Fixtures

### Counter Fixture

Build the counter composition binary and run its automated test:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo build --manifest-path fixtures/bridge-counter/Cargo.toml --release
cp /tmp/gpui-react-target/release/libgpui_react_counter_example.dylib fixtures/bridge-counter/counter.node
bun fixtures/bridge-counter/test.ts
```

Run the interactive counter demo (window stays open):

```sh
bun fixtures/bridge-counter/demo-host.ts
```

### Package Packaging Fixture

Create package archives and verify installation in an isolated directory:

```sh
bun scripts/package-bridge.ts 0.1.0-bridge.1 /tmp/bridge-archives --allow-dirty
bun scripts/test-bridge-packages.ts /tmp/bridge-archives
```

## Benchmarks and Reports

### Native Frame Cost Comparison

Compare native frame times and heap allocations between raw GPUI and the bridge:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  bun scripts/measure-bridge-frames.ts /tmp/gpui-react-frame-cost
```

- Report: [`docs/bridge-frame-cost.md`](./docs/bridge-frame-cost.md)
- Raw Data: [`docs/benchmarks/bridge-frame-cost.json`](./docs/benchmarks/bridge-frame-cost.json)

### JavaScript Worker Benchmark

Measure React render and commit, sealing to the wire, wire size, and heap on the
worker. Use React's production build; the development build doubles the render:

```sh
/tmp/gpui-react-target/release/gpui-react-frame-cost schema > /tmp/gpui-react-wire/schema.json
NODE_ENV=production BRIDGE_WIRE=binary bun fixtures/bridge-counter/js-bench.tsx 5000 list 5
```

`BRIDGE_WIRE` selects `json` or `binary`. For sub-millisecond comparisons,
`BENCH_BALLAST=32` and `BENCH_NO_PARSE=1` keep the collector's pacing out of
the numbers; `BENCH_HOIST=1`, `BENCH_MEMO=1`, `BENCH_UPDATES=<n>`, and
`BENCH_TRACE=1` are described in the file header.

- Raw Data: [`docs/benchmarks/bridge-js-cost.json`](./docs/benchmarks/bridge-js-cost.json)

### List Mutation Performance

Count-update measurements and height-index update analysis:

- Report: [`docs/bridge-list-performance.md`](./docs/bridge-list-performance.md)
- Raw Data: [`docs/benchmarks/bridge-list-count.json`](./docs/benchmarks/bridge-list-count.json)

## Repository Layout

```
.
├── crates/
│   ├── gpui-react/              # Core Host, node tables, protocol, traits
│   ├── gpui-react-controls/     # Five standard native controls
│   ├── gpui-react-host/         # macOS AppKit loop and N-API transport
│   └── gpui-react-runtime/      # Default native composition dylib
├── packages/
│   ├── bridge/                  # React reconciler and application launcher
│   ├── bridge-controls/         # React wrappers for standard controls
│   └── bridge-runtime/          # Bundled macOS arm64 native runtime
├── fixtures/
│   ├── bridge-counter/          # E2E counter fixture and interactive demo
│   ├── bridge-gpu-component/    # Metal texture integration fixture
│   ├── bridge-package/          # Tarball verification fixture
│   ├── bridge-performance/      # Frame timing and memory comparison
│   └── bridge-pierre/           # External viewport probe
├── docs/                        # Performance reports and benchmark data
├── reviews/                     # Architecture review history
├── scripts/                     # Packaging, benchmark, and build scripts
└── zed/                         # GPUI source submodule
```

## Acknowledgements

This project started as experiments on the GPUiX project by Tommy (@remorses, https://github.com/remorses/gpuix). This project keeps the Apache-2.0 license of GPUiX. No GPUiX renderer code remains.

