# Public extension composition fixture

This crate links the core runtime with one test component. It contains no
application component. It uses only the public extension API. The component
produces a solid-colour float texture on the renderer's GPU queue, then calls
`Window::paint_gpu_texture` during paint.

Build and test on macOS:

```sh
CARGO_BUILD_JOBS=3 cargo build --manifest-path fixtures/native-composition/Cargo.toml --release
cp fixtures/native-composition/target/release/libgpuix_extension_example.dylib fixtures/native-composition/example.node
GPUIX_BACKGROUND=1 bun fixtures/native-composition/test.ts
```

If `CARGO_TARGET_DIR` is set, copy the library from that target directory.
The test selects the composition through the real binding loader. It checks
bounds, text, clicks, inherited opacity, HDR blending, clipping, corners,
multiple textures, resize, and removal. Screenshots are written under
`packages/react/screenshots/extension-composition`.

`color` is premultiplied RGBA. Values above alpha are intentional. The producer
uses GPU clear commands. It does not upload CPU pixels or wait for each frame.
`radius` is in logical pixels. `label` tests the shared text API.

The native host test also checks source workers and a compiled executable after
relocation. Both host and worker configure the same composition before React:

```sh
GPUIX_BACKGROUND=1 bun fixtures/native-composition/test-host.ts
```

Build the browser composition with the installed nightly Rust toolchain and
wasm-bindgen CLI:

```sh
bun fixtures/native-composition/build.ts --wasm
bun fixtures/native-composition/server.ts
```

The test server rejects requests for the default WASM runtime. The browser must
use the configured composition. `?backend=webgl` hides WebGPU from this test page
to exercise GPUI's WebGL fallback. It does not change browser or OS settings.

With the server running, run the repeatable browser checks through an installed
`agent-browser` CLI. The test creates and closes its own headless session:

```sh
bun fixtures/native-composition/test-browser.ts
```

It checks both backend selections, exact pixels, clicks, resize, removal, runtime
metadata, and browser errors. Results and screenshots use the same evidence
directory as the native test. Run `bunx tsc -p fixtures/native-composition/tsconfig.json`
after generating the WASM declarations to check the TypeScript scripts.
