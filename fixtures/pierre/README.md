# External Pierre runtime probe

This fixture loads Pierre's optional `pierre-react-runtime` composition. The
viewport, document payloads, rendering code, and Rust adapters stay in Pierre.
This folder contains integration probes only. It is not an editor package.

Build Pierre's native composition separately. Then run:

```sh
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
Commands apply source/view/patch payloads. Events carry Pierre event objects.

The external document model applies committed editor edits and owns undo.
Native IME preview, selection, scrolling, geometry, and painting remain in
the GPUI viewport. Pierre's Rust tests and offscreen GPU example verify those
behaviors directly.

The bootstrap currently supports macOS and Bun. Pierre's native and web entry
points remain separate.
