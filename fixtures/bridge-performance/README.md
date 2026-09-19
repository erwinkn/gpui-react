# Native frame and heap comparison

This fixture compares four native paths in one binary and one GPUI build:

- `raw`: one handwritten GPUI view, strings, a scroll handle, and `ListState`.
  It has no document selection, inspection records, or per-row view entities.
  Treat it as a lower bound, not a feature-equivalent application.
- `controls`: the standard ordinary GPUI `Document`, `Text`, `Container`, and
  `VirtualList`, called directly with typed Rust values.
- `bridge`: the same controls, created and updated through the production
  `Host` transaction parser and validator.
- `legacy`: the existing `GpuixView`, retained tree, and batch parser. The
  optional Rust-only `bench-internals` constructor supplies the production
  view without an N-API wrapper. No benchmark method enters the JS API.

Both scenes have an 800 by 600 logical-pixel viewport, a 32-pixel status line,
and identical 20-pixel rows. `flow` retains and lays out all row views inside
a native scroll container. `list` retains the supplied row descriptions but
lets GPUI build visible rows. This is not the separate 100,000-logical-row,
60-supplied-row test. Document selection is enabled in both control modes and
the existing renderer. The raw lower bound only draws text.

Each process mounts once, changes only the status line beside unchanged rows,
then alternates a one-row wheel movement. It records ten warmup operations and
100 measured operations for each repeated case. Every wheel must be consumed
by a native scroller. Removal clears the scene and draws an empty frame.
The driver uses a hidden production-mode GPUI application. It asserts that
mount and update do not draw, and that each explicit draw renders exactly once.
`VisualTestAppContext` is unsuitable for these phase measurements because it
automatically draws after mutations. The initial scene pixels match exactly
across all four modes in both scenes.

`mount` includes decoding and native construction, but excludes construction
of the source bytes. `nativeUpdate` includes decoding, validation, and applying
the small update. In the deployed bridge, decoding runs on the worker; this
fixture reports combined native CPU cost on one thread. The direct modes use
typed Rust calls. `updatedDraw` includes GPUI view construction, layout,
prepaint, paint, draw-completion callbacks, arena cleanup, and queued effects.
`wheelAndDraw` also includes native wheel dispatch. It does not include physical
presentation, React reconciliation, JS encoding, worker latency, or GPU
completion. Native drawing can still prepare glyphs and atlas resources.

The allocation build counts all requested Rust heap bytes in the process.
It excludes Objective-C allocations, allocator metadata, GPU memory, and RSS.
Live bytes are relative to an empty offscreen GPUI window. Input bytes and the
sample buffers exist before that baseline. Frame caches and native text caches
remain part of the measured live memory. The first draw has cold text caches;
the repeated samples are warm. Background Rust allocations can contribute to
the allocation counts. Timing results use a separate build without the counter.

```sh
CARGO_TARGET_DIR=/tmp/gpuix-framework-target CARGO_BUILD_JOBS=3 \
  bun scripts/measure-bridge-frames.ts /tmp/gpuix-frame-cost
```

The script builds both variants, then runs each mode in a separate process.
It first checks that all modes produce the same nonempty scene image. These
image-check runs are separate from the timing and allocation samples.
It uses 100, 1,000, and 5,000 supplied rows, three repeats, and rotates mode order.
It writes raw JSON and machine/source metadata. Run without other builds or
benchmarks in progress. All windows stay off screen. Source dependency features
and release optimization are shared by all four modes; this is not a timing
comparison between independently built published binaries.

For scene inspection, run the timing binary with `FRAME_BENCH_IMAGES` set to a
directory. Do not use that run for the memory table: GPU image capture can
allocate and warm extra caches.
