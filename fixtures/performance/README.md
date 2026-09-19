# Native frame and heap comparison

This fixture compares three native paths in one binary and one GPUI build:

- `raw`: one handwritten GPUI view, strings, a scroll handle, and `ListState`.
  It has no document selection, inspection records, or per-row view entities.
  Treat it as a lower bound, not a feature-equivalent application.
- `bridge`: the standard `Document`, `Text`, `Container`, and `VirtualList`
  controls, created and updated through the production `Host` transaction
  decoder and applier, from the JSON wire.
- `binary`: the same controls from the binary wire, decoded positionally
  against the component schema.

With `FRAME_BENCH_WIRE_DIR` set, both bridge modes mount the transaction the
JavaScript bridge sealed for the scene (`fixtures/bridge-counter/js-wire-dump.tsx`
writes `mount-<scene>-<rows>.json` and `.bin`); the measuring script does this.
Without it, `bridge` builds an equivalent JSON transaction in Rust and `binary`
is unavailable. `gpui-react-frame-cost schema` prints the controls' kind table
for worker-side tools.

Both scenes have an 800 by 600 logical-pixel viewport, a 32-pixel status line,
and identical 20-pixel rows. `flow` retains and lays out all row views inside
a native scroll container. `list` retains the supplied row descriptions but
lets GPUI build visible rows. This is not the separate 100,000-logical-row,
60-supplied-row test. Document selection is enabled in the bridge mode.
The raw lower bound only draws text.

Each process mounts once, changes only the status line beside unchanged rows,
then alternates a one-row wheel movement. It records ten warmup operations and
100 measured operations for each repeated case. Every wheel must be consumed
by a native scroller. Removal clears the scene and draws an empty frame.
The bridge mode applies its real root-removal operations; the raw mode drops its
native view. This measures ordinary unmount rather than the host's separate
failure-shutdown cleanup method. The driver uses a hidden production-mode GPUI
application. It asserts that mount and update do not draw, and that each
explicit draw renders exactly once. `VisualTestAppContext` is unsuitable for
these phase measurements because it automatically draws after mutations. The
initial scene pixels match across both modes in both scenes.

`mount` includes decoding and native construction, but excludes construction
of the source bytes. `BRIDGE_MOUNT_PHASES=1` prints the decode and apply
phases of the bridge mount separately. `nativeUpdate` includes decoding, validation, and applying
the small update. In the deployed bridge, decoding runs on the worker; this
fixture reports combined native CPU cost on one thread. `updatedDraw` includes GPUI view construction, layout,
prepaint, paint, draw-completion callbacks, arena cleanup, and queued effects.
`wheelAndDraw` also includes native wheel dispatch. It does not include physical
presentation, React reconciliation, JS encoding, worker latency, or GPU
completion. Native drawing can still prepare glyphs and atlas resources.

The timing and allocation builds disable GPUI test support and entity leak
tracking. The runner checks the resolved feature graph before building. A
third build enables image capture solely for scene verification. This avoids
charging production view creation/cloning for test-only tracking.

The allocation build counts all requested Rust heap bytes in the process.
It excludes Objective-C allocations, allocator metadata, GPU memory, and RSS.
Live bytes are relative to an empty offscreen GPUI window. Input bytes and the
sample buffers exist before that baseline. Frame caches and native text caches
remain part of the measured live memory. The first draw has cold text caches;
the repeated samples are warm. Background Rust allocations can contribute to
the allocation counts. Timing results use a separate build without the counter.

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  bun scripts/measure-bridge-frames.ts /tmp/gpui-react-frame-cost
```

The script builds all three variants, then runs each mode in a separate process.
It first checks that all modes produce the same nonempty scene image. These
image-check runs are separate from the timing and allocation samples.
It uses 100, 1,000, and 5,000 supplied rows, three repeats, and rotates mode order.
It writes raw JSON and machine/source metadata. Run without other builds or
benchmarks in progress. All windows stay off screen. Source dependency features
and release optimization are shared by both modes; this is not a timing
comparison between independently built published binaries.

`rustLiveBytesAboveEmpty` reports live bytes after mount, after the first draw,
after the status-line updates, after the wheel movements, and after removal, so
growth can be attributed to a phase. `BRIDGE_ROOT=container` replaces the bridge
scene's document root with a plain container to exclude document selection from
a comparison. `BRIDGE_SKIP_UPDATE=1` sends empty transactions instead of the
status-line update, so the following draws re-render nothing changed.

The allocation build also reports `liveGrowthBySize`: the allocation sizes whose
live count changed most between the first draw and the end of the update phase,
which identifies what a retained-memory change is made of. `PROBE_TRACK_SIZE=N`
records a backtrace for every live allocation of exactly `N` bytes after the
first draw, prints the most common allocation stack, and prints the stack of the
first frees of blocks that outlived their draw. That is how the previous-frame
measure closures retained by the layout engine were found.

The `sizes` example prints the sizes of GPUI types, to match histogram buckets to
structures.

For scene inspection, run the `scenes` binary with `FRAME_BENCH_IMAGES` set to a
directory. Do not use that run for the memory table: GPU image capture can
allocate and warm extra caches.
