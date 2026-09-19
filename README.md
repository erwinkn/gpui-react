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

- `crates/gpui-react`: Core bridge crate and the five standard native controls (`Document`, `VirtualList`, `Container`, `Text`, `Input`). Provides the `Host` entity, dense node storage, transaction decoding, frame metadata (`current_frame`), the component traits (`ReactView`, `ReactElement`, `ReactEvents`, `ReactCommands`, `ReactQueries`, `ReactChildren`), style parsing, and `register_builtins`.
- `crates/gpui-react-macros`: `ComponentProps` derive macro for props wire schemas.
- `crates/gpui-react-runtime`: Native application loop for macOS AppKit (`NSApplication::run`), the N-API worker communication channel, and the default composition that builds the macOS arm64 `.node` binary. It builds both an `rlib` and a `cdylib`.

### JavaScript Packages (`packages/`)

- `packages/core` (`@gpui-react/core`): React 19 reconciler for GPUI plus the typed React wrappers and refs for the standard controls (`Document`, `List`, `Container`, `Text`, `Input`). Provides `createRoot`, `nativeComponent`, transaction transport, and application entry helpers (`runApplication`, `attachApplication`).
- `packages/runtime` (`@gpui-react/runtime`): Default runtime package that bundles the compiled native `.node` binary for macOS arm64.

### Application Shapes

A pure React application installs `@gpui-react/core` and `@gpui-react/runtime`
and needs no Rust toolchain: the runtime package ships the compiled native
library, and `host.ts` imports `bindings` from `@gpui-react/runtime`.

An application with its own native components adds a `native/` crate that
depends on `gpui-react` and `gpui-react-runtime`, registers its components in
one `#[napi_derive::module_init]` through `register_components`, and builds a
`cdylib`. Its `host.ts` points at that crate's own `.node` file. Built-in
controls are always registered, so the composition only adds its own kinds.
[`fixtures/counter`](./fixtures/counter/README.md) is the reference for this second shape.

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

Use the standard flags for every Cargo command. The repository is one Cargo
workspace rooted at `Cargo.toml`.

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo test --workspace
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Check the fixture feature graphs that a plain workspace build does not use:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo check -p gpui-react-frame-cost --features allocation-counts
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo check -p gpui-react-texture-example
```

### Build and Test JavaScript Packages

```sh
bun install
bun run build
bun run test
```

## Running Examples and Visual Fixtures

Run the visual examples. Windows open inactive and stay in the background:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo run -p gpui-react --example document_visual
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo run -p gpui-react --example container_visual
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo run -p gpui-react --example list_visual
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo run -p gpui-react --example geometry_visual
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo run -p gpui-react --example selection_toolbar_visual
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo run -p gpui-react --example deferred_document_visual
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo run -p gpui-react-texture-example --example visual
```

## Running Fixtures

### Counter Fixture

Build the counter composition binary and run its automated test:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo build -p gpui-react-counter-example --release
cp /tmp/gpui-react-target/release/libgpui_react_counter_example.dylib fixtures/counter/counter.node
bun fixtures/counter/test.ts
```

Run the interactive counter demo (window stays open):

```sh
bun fixtures/counter/demo-host.ts
```

### Package Packaging Fixture

Create package archives and verify installation in an isolated directory:

```sh
bun scripts/package.ts 0.1.0-bridge.1 /tmp/bridge-archives --allow-dirty
bun scripts/test-packages.ts /tmp/bridge-archives
```

## Benchmarks and Reports

### Native Frame Cost Comparison

Compare native frame times and heap allocations between raw GPUI and the bridge:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  bun scripts/measure-frames.ts /tmp/gpui-react-frame-cost
```

- Report: [`docs/bridge-frame-cost.md`](./docs/bridge-frame-cost.md)
- Raw Data: [`docs/benchmarks/bridge-frame-cost.json`](./docs/benchmarks/bridge-frame-cost.json)

### JavaScript Worker Benchmark

Measure React render and commit, sealing to the wire, wire size, and heap on the
worker. Use React's production build; the development build doubles the render:

```sh
/tmp/gpui-react-target/release/gpui-react-frame-cost schema > /tmp/gpui-react-wire/schema.json
NODE_ENV=production BRIDGE_WIRE=binary bun fixtures/counter/js-bench.tsx 5000 list 5
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
├── Cargo.toml                   # Root Cargo workspace
├── crates/
│   ├── gpui-react/              # Core Host, node tables, protocol, traits, controls
│   ├── gpui-react-macros/       # ComponentProps derive
│   └── gpui-react-runtime/      # macOS AppKit loop, N-API transport, default composition
├── packages/
│   ├── core/                    # React reconciler and control wrappers
│   └── runtime/                 # Bundled macOS arm64 native runtime
├── fixtures/
│   ├── counter/                 # E2E counter fixture and interactive demo
│   ├── gpu-component/           # Metal texture integration fixture
│   ├── package/                 # Tarball verification fixture
│   ├── performance/             # Frame timing and memory comparison
│   └── pierre/                  # External viewport probe
├── docs/                        # Performance reports and benchmark data
├── reviews/                     # Architecture review history
├── scripts/                     # Packaging, benchmark, and build scripts
└── zed/                         # GPUI source submodule
```

## Acknowledgements

This project started as experiments on the GPUiX project by Tommy (@remorses, https://github.com/remorses/gpuix). This project keeps the Apache-2.0 license of GPUiX. No GPUiX renderer code remains.

