# Native component composition with React bindings

This composition links the new bridge host, an ordinary GPUI counter, and the
new native controls, plus an [external GPU view](../gpu-component/README.md).
Its `Render` implementation remains native. The small `ReactView` implementation
maps props; optional traits expose typed events, commands and queries.

## Interactive example

After the release build below, run `bun fixtures/counter/demo-host.ts`.
The window opens inactive. It contains a native input, a 500-row virtual list,
document selection/search, an ordinary Rust counter, and a React heartbeat.
Click **Pause JavaScript for 5 seconds**, then type, scroll, or click the native
counter. The heartbeat and JS event label pause. Native editing, scrolling,
hover, and counter updates continue. When JS resumes, the label receives the
queued events. Close the window to stop the example.

Compile both `demo-host.ts` and `demo-worker.tsx` for a standalone executable.
`BRIDGE_DEMO_AUTOCLOSE=1` runs a short startup/cleanup check.
With the optional `interaction-tests` build, set `BRIDGE_DEMO_IMAGE` to a PNG
path to capture the first complete native draw. This uses GPU readback, so a
covered background window does not need to present a frame to the display.

From the repository root, build a release binary:

```sh
bun install --frozen-lockfile
bun run build
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 cargo build -p gpui-react-counter-example --release
cp /tmp/gpui-react-target/release/libgpui_react_counter_example.dylib fixtures/counter/counter.node
bun fixtures/counter/test.ts
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
- Interpolated document text as one native value, search, versioned UTF-16
  selection, and native selection events.
- A separate GPU component crate, float texture creation, shared document text,
  native transition/cancellation, and state-preserving prop updates.
- A compiled Bun executable with all worker entries, moved outside its build directory.

The final state reports a native render count, but occluded windows need not draw
on every update. Native timer progress does not measure display FPS or
input-to-photon latency. The kit crate has a separate GPU-backed keyboard,
IME, accessibility, and nested-scroll test. The native controls also have list, shared-scroll, and document scenarios.
External editor examples remain a later validation stage.

## Native interaction during a worker stall

Build the optional test driver and enable its source and relocated-worker cases:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 cargo build -p gpui-react-counter-example --release --features interaction-tests
cp /tmp/gpui-react-target/release/libgpui_react_counter_example.dylib fixtures/counter/counter.node
BRIDGE_INTERACTION_TESTS=1 bun fixtures/counter/test.ts
```

React mounts the ordinary `Input`, `List`, and `Text` wrappers plus the external
`Texture` component. A fixture-only
native driver then waits for a shared test flag. The worker sets that flag and
runs a synchronous loop until the native script finishes. No JavaScript timer,
React work, or event callback can run in that interval.

Set `BRIDGE_EXAMPLE_OUTPUT` to a directory to save native GPU images before
input, after typing, after scrolling, and after animation. All captures occur
while the test flag proves the worker is still blocked. This option exists only
in the fixture's optional test driver.

The native script sends keystrokes and wheel events through GPUI and uses the
platform input handler for IME. It checks selection deletion, undo, committed
composition, subsequent typing, native list scroll, hover pixels, caret pixels,
the caret timer's notification, and animated pixels. The external texture producer
must create new resources and change its GPU pixels during the same stall. Each
operation checks that
the worker is still blocked. Native draw numbers advance without another React
commit. After the worker resumes, ordered edit events reach their original
callback even when React has replaced it before draining the queue. A command
with the pre-stall input revision cannot erase the newer text.

The window stays hidden. The native driver requests draws and reads GPU images
because an occluded window has no guaranteed display cadence. This validates
native input and draw progress under a worker stall. It does not measure physical
presentation, frame rate, or OS input-to-photon latency. Keystrokes use GPUI's
native dispatch and simulated IME path, not a system-wide event injector.
The test flags and driver are compiled only into this fixture with
`interaction-tests`; the production bridge has no test input API.

## Shutdown and failure

The same optional test build runs `lifecycle-test.ts` against source entries
and a separate compiled executable after relocation. It covers normal unmount,
SIGINT, SIGTERM, a blocked worker, native window removal, event-queue overflow,
worker failure after mount, early worker exit, and default SIGTERM behavior
after the host ends. Each case records `mounted`, `unmount`, and `drop` to a
temporary file and requires each native cleanup step exactly once.

Responsive workers must run both layout-effect and passive-effect cleanup,
followed by their process exit handler. A blocked worker cannot run React
cleanup. The test verifies that the launcher terminates it and still releases
native resources. Readiness uses a temporary file because worker console output
can wait for the launcher's blocked JS loop. The test only signals its own child
processes and removes its temporary files.

Run the source cases alone after the optional native build:

```sh
bun fixtures/counter/lifecycle-test.ts
bun run --cwd fixtures/counter typecheck
```

## Worker-side benchmark

`bun fixtures/counter/js-bench.tsx <rows> <list|flow> <repeats>` measures
React render and commit, sealing, wire size, and JavaScript heap for the frame
comparison scene with a recording transport and no native code. `BENCH_MEMO=1`
memoizes the rows, which is how an application would write them.
