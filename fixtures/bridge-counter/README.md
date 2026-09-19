# Ordinary GPUI counter with a React wrapper

This composition links only the new bridge host and an ordinary GPUI counter.
Its `Render` implementation remains native. The small `ReactView` implementation
maps props; optional traits expose typed events, commands and queries. No old
GPUiX renderer or worker-side tree is linked.

From the repository root, build a release binary:

```sh
bun install --frozen-lockfile
bun run --cwd packages/bridge build
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
- Three sequential native sessions in one process.
- Worker startup error, missing worker entry, and rejected native props.
- A compiled Bun executable with both entries, moved outside its build directory.

The final state reports a native render count, but occluded windows need not draw
on every update. Native timer progress does not measure display FPS or
input-to-photon latency. Keyboard/IME, list, rich-text, and editor examples are
the next validation stages.
