# Ordinary GPUI counter with a React wrapper

This composition links the new bridge host, an ordinary GPUI counter, and the
new native controls.
Its `Render` implementation remains native. The small `ReactView` implementation
maps props; optional traits expose typed events, commands and queries. No old
GPUiX renderer or worker-side tree is linked.

From the repository root, build a release binary:

```sh
bun install --frozen-lockfile
bun run --cwd packages/bridge build
bun run --cwd packages/bridge-controls build
CARGO_TARGET_DIR=/tmp/gpuix-framework-target CARGO_BUILD_JOBS=3 cargo build --manifest-path fixtures/bridge-counter/Cargo.toml --release
cp /tmp/gpuix-framework-target/release/libgpui_react_counter_example.dylib fixtures/bridge-counter/counter.node
GPUIX_BACKGROUND=1 bun fixtures/bridge-counter/test.ts
```

The test keeps windows in the background and closes its own processes. It checks:

- Initial React layout-effect command and native event delivery.
- Prop changes that preserve the existing native counter state.
- Asynchronous state queries.
- Native executor progress during a 300 ms synchronous application-worker block.
- Event delivery for a command immediately followed by component removal.
- Input identity, typed events, stale-command rejection, selection commands, and
  prop changes that preserve native text.
- Three sequential native sessions in one process.
- Worker startup error, missing worker entry, and rejected native props.
- A 100,000-row logical list supplied with sixty React rows, a first-frame
  layout-effect anchor, an atomic row-window change, and keyed child identity.
- A compiled Bun executable with all worker entries, moved outside its build directory.

The final state reports a native render count, but occluded windows need not draw
on every update. Native timer progress does not measure display FPS or
input-to-photon latency. The controls crate has a separate GPU-backed keyboard,
IME, accessibility, and nested-scroll test. The native controls also have list and shared-scroll scenarios. Document text
services and external editor examples remain later validation stages.
