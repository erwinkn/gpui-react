# External Pierre runtime probe

This fixture loads Pierre's optional `pierre-react-runtime` composition. The
viewport, document payloads, rendering code, and Rust adapters stay in Pierre.
This folder contains integration probes only. It is not an editor package.

Build Pierre's native compositions separately. Then run:

```sh
bun fixtures/bridge-pierre/legacy.ts /path/to/libpierre_native_runtime.dylib
bun fixtures/bridge-pierre/test.ts /path/to/libpierre_react_runtime.dylib
bunx tsc -p fixtures/bridge-pierre/tsconfig.json
```

The new-host probe runs both the source worker and a compiled Bun executable
that has been moved away from its build directory. All windows stay hidden.
It checks layout-effect focus commands, initial layout events and frame tags,
document patches, annotation children, native identity across React updates,
construction-only initial source, stale view acknowledgements, blur, and unmount.
A native query reads saved geometry. The probe waits for the named painted
revision when its assertion needs paint; `flush()` alone does not force a draw.

GPUI stops display callbacks when a macOS window is hidden or fully covered.
Build Pierre's optional `frame-probe` feature and pass `--frames` to test later
paint without activating a window:

```sh
# In the Pierre checkout:
cargo build --release --locked -p pierre-react-runtime --features frame-probe
# In this checkout:
bun fixtures/bridge-pierre/test.ts /path/to/libpierre_react_runtime.dylib --frames
```

That test-only component draws from the native executor every 16 ms until
unmount. The frame test requires painted document revision 2 and the new
annotation height of 60 pixels. The default composition contains no frame driver.
Neither run is a display performance benchmark.

`host.ts` and `worker.tsx` show the bootstrap and the thin `nativeComponent`
wrapper. Both load the same composition. Props carry initial source and style.
Commands apply source/view/patch payloads. Events carry the existing Pierre
objects without the legacy `change.value` JSON string envelope.

The legacy probe checks registration, text inspection, bounds, the old event
envelope, keyboard input, patch application, and removal through
`TestGpuixRenderer`. It enables the legacy key listener before programmatic focus,
as required by that renderer's focus-handle registration.

The existing external document model still applies committed editor edits and
owns undo. This extraction does not promise committed typing while that model
is blocked. Native IME preview, selection, scrolling, geometry, and painting
remain in the ordinary GPUI viewport. Pierre's Rust tests and offscreen GPU
example check those behaviors directly.

The new bootstrap currently supports macOS/Bun. Pierre's default legacy native
and WebGPU/WebGL entry points remain unchanged. A WASM composition build proves
that the legacy browser adapter still compiles. The complete browser interaction
suites belong to the Pierre project and are separate from this probe.

`input-method.ts` checks the legacy renderer's generic `simulateInputMethod`
helper against its built-in input. It verifies missing focus, UTF-16 preedit
selection, composition commit, handler restoration, events, and undo. The
legacy Pierre probe also checks the helper against the external viewport.

```sh
bun fixtures/bridge-pierre/input-method.ts /path/to/libgpuix_native.dylib
```
