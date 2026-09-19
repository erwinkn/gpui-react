# Native frame and heap comparison

Release measurements from the [frame comparison fixture](../fixtures/bridge-performance/README.md)
on an Apple M5 Max, macOS 26.6.2, Rust 1.97.1, taken after the flat host tables,
style ids, id reuse, the lazy document geometry, and the GPUI layout engine fix.
Values are medians over three repeats of the p50 of 100 measured operations. Timings
come from the build without the counting allocator; live bytes come from the counting
build and exclude JavaScript, GPU memory, and Objective-C allocations. `raw` is a
handwritten GPUI view without document selection and is a lower bound, not an
equivalent application.

| Scene | Rows | Mode | Mount | Mount allocations | Draw | Rust heap after mount | Rust heap after first draw |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| list | 100 | raw | 0.03 ms | 242 | 0.25 ms | 21 KiB | 813 KiB |
| list | 100 | bridge | 0.13 ms | 423 | 0.32 ms | 38 KiB | 991 KiB |
| list | 100 | legacy | 0.12 ms | 271 | 0.38 ms | 49 KiB | 1,603 KiB |
| list | 1,000 | raw | 0.12 ms | 2,214 | 0.25 ms | 194 KiB | 988 KiB |
| list | 1,000 | bridge | 0.51 ms | 3,218 | 0.32 ms | 299 KiB | 1,253 KiB |
| list | 1,000 | legacy | 0.55 ms | 2,084 | 0.39 ms | 593 KiB | 2,272 KiB |
| list | 5,000 | raw | 0.50 ms | 10,952 | 0.24 ms | 964 KiB | 1,759 KiB |
| list | 5,000 | bridge | 2.10 ms | 15,596 | 0.29 ms | 1,460 KiB | 2,415 KiB |
| list | 5,000 | legacy | 2.14 ms | 10,095 | 0.40 ms | 2,437 KiB | 4,694 KiB |
| flow | 100 | raw | 0.03 ms | 242 | 0.33 ms | 21 KiB | 1,198 KiB |
| flow | 100 | bridge | 0.12 ms | 380 | 0.36 ms | 25 KiB | 1,277 KiB |
| flow | 100 | legacy | 0.10 ms | 269 | 0.66 ms | 49 KiB | 2,693 KiB |
| flow | 1,000 | raw | 0.13 ms | 2,214 | 2.01 ms | 194 KiB | 6,702 KiB |
| flow | 1,000 | bridge | 0.38 ms | 3,086 | 2.29 ms | 197 KiB | 7,454 KiB |
| flow | 1,000 | legacy | 0.50 ms | 2,082 | 7.24 ms | 593 KiB | 24,560 KiB |
| flow | 5,000 | raw | 0.49 ms | 10,952 | 12.28 ms | 964 KiB | 41,002 KiB |
| flow | 5,000 | bridge | 1.78 ms | 15,092 | 14.64 ms | 963 KiB | 46,183 KiB |
| flow | 5,000 | legacy | 2.04 ms | 10,093 | 82.62 ms | 2,437 KiB | 118,153 KiB |

With one GPUI entity per node, the bridge at 5,000 rows held about 5.0 MiB after mount
in the list scene and drew the flow scene in about twice the raw time. Retained memory
after mount is now level with the old renderer, and draws are faster than it in every
case. The remaining draw gap to `raw` is document selection: every text registers a
hitbox and a paint entry with its document, which the raw view does not do.

Bridge mount decodes JSON text straight into typed props in one pass. This fixture
runs that decode on the same thread; in the deployed host it runs on the application
worker. The old renderer parses its batch bytes directly into its tree.

The earlier report showed the flow scene growing by about 3.5 KB per text node after
the first draw for both the bridge and the old renderer. That was GPUI's layout
engine: `TaffyTree::clear` drops nodes but not their contexts, so every measured
element's closure, with its captured text layout, survived until the next frame
overwrote its slot. The fork now drops them at the end of the frame. Live bytes after
the first draw fell by about 17 MiB for the raw view and 20 MiB for the bridge at
5,000 unvirtualized rows.

## Worker side

`fixtures/bridge-counter/js-bench.tsx` measures the same scene on the application
worker with a recording transport: React render and commit, sealing to JSON, the
wire size, and the JavaScript heap. Medians of five runs, Bun 1.4.2.

| Case, 5,000 rows | React render and commit | Seal | Wire | JS heap after mount |
| --- | ---: | ---: | ---: | ---: |
| Mount, before | 10.2 ms | 4.3 ms | 1.21 MB | 10.0 MB |
| Mount, after | 7.8 ms | 1.25 ms | 1.21 MB | 8.5 MB |
| One-line update, rows not memoized | 15.4 ms | 0.02 ms | 156 B | |
| One-line update, rows memoized | 1.6 ms | 0.01 ms | 156 B | |
| Remove all | 1.4 ms | 0.01 ms | 65 B | |

"Before" sealed with a validating replacer callback, which made `JSON.stringify`
seven times slower than plain serialization, and re-encoded the string to count
bytes. Props are now validated once when they are recorded, the ref object is
created only when a ref asks for it, and unchanged props are detected without
building filtered copies. The unmemoized update is React reconciling 5,000 row
elements; the reconciler's own share of it is about 1 ms. Raw data is in
[bridge-js-cost.json](./benchmarks/bridge-js-cost.json).

Raw medians are in [bridge-frame-cost.json](./benchmarks/bridge-frame-cost.json).
Reproduce with:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  bun scripts/measure-bridge-frames.ts /tmp/gpui-react-frame-cost
```
