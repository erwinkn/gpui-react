# Native frame and heap comparison

Release measurements from the [frame comparison fixture](../fixtures/performance/README.md)
on an Apple M5 Max, macOS 26.6.2, Rust 1.97.1, taken after the compact wire, the
flat host tables, style ids, id reuse, placement folded into create, packed `Text`
and `Container` rows with per-kind extras, the packed document text records, the
lazy document geometry, and the GPUI layout engine fix. Values are medians over
three repeats of the p50 of 100 measured operations. Timings come from the build
without the counting allocator; live bytes come from the counting build and
exclude JavaScript, GPU memory, and Objective-C allocations. `raw` is a
handwritten GPUI view without document selection and is a lower bound, not an
equivalent application. Both bridge modes mount the transaction the JavaScript
bridge sealed for the scene: `bridge` decodes the JSON text, `binary` the
schema-driven binary wire. All three modes draw the same image.

| Scene | Rows | Mode | Wire | Mount | Mount allocations | Draw | Rust heap after mount | Rust heap after first draw |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| list | 100 | raw |  | 0.02 ms | 242 | 0.20 ms | 21 KiB | 757 KiB |
| list | 100 | bridge | 17 KB | 0.12 ms | 422 | 0.26 ms | 35 KiB | 929 KiB |
| list | 100 | binary | 9 KB | 0.08 ms | 421 | 0.26 ms | 35 KiB | 929 KiB |
| list | 1,000 | raw |  | 0.09 ms | 2,214 | 0.20 ms | 194 KiB | 932 KiB |
| list | 1,000 | bridge | 165 KB | 0.39 ms | 3,217 | 0.26 ms | 268 KiB | 1,164 KiB |
| list | 1,000 | binary | 87 KB | 0.20 ms | 3,210 | 0.26 ms | 268 KiB | 1,164 KiB |
| list | 5,000 | raw |  | 0.44 ms | 10,952 | 0.21 ms | 964 KiB | 1,703 KiB |
| list | 5,000 | bridge | 825 KB | 1.62 ms | 15,595 | 0.25 ms | 1,304 KiB | 2,200 KiB |
| list | 5,000 | binary | 435 KB | 0.84 ms | 15,585 | 0.26 ms | 1,304 KiB | 2,200 KiB |
| flow | 100 | raw |  | 0.02 ms | 242 | 0.30 ms | 21 KiB | 1,142 KiB |
| flow | 100 | bridge | 17 KB | 0.09 ms | 379 | 0.34 ms | 22 KiB | 1,212 KiB |
| flow | 100 | binary | 9 KB | 0.08 ms | 378 | 0.34 ms | 22 KiB | 1,212 KiB |
| flow | 1,000 | raw |  | 0.10 ms | 2,214 | 1.67 ms | 194 KiB | 6,622 KiB |
| flow | 1,000 | bridge | 165 KB | 0.36 ms | 3,085 | 1.90 ms | 166 KiB | 7,264 KiB |
| flow | 1,000 | binary | 87 KB | 0.17 ms | 3,078 | 1.89 ms | 166 KiB | 7,264 KiB |
| flow | 5,000 | raw |  | 0.42 ms | 10,952 | 9.02 ms | 964 KiB | 40,906 KiB |
| flow | 5,000 | bridge | 825 KB | 1.48 ms | 15,091 | 9.95 ms | 807 KiB | 45,533 KiB |
| flow | 5,000 | binary | 435 KB | 0.67 ms | 15,081 | 10.15 ms | 807 KiB | 45,533 KiB |

With one GPUI entity per node, the bridge at 5,000 rows held about 5.0 MiB after mount
in the list scene and drew the flow scene in about twice the raw time. Now the bridge
holds less after mount than the raw view in the flow scene, because the raw view builds
a `ListState` in both scenes. In the list scene the bridge keeps 340 KiB more: the
5,000 row descriptions the virtual list retains, at 48 bytes of row plus the string.
The remaining draw gap to `raw` is document selection: every text registers a
hitbox and a paint entry with its document, which the raw view does not do.

## The wire

The binary wire is written by the worker as React commits, straight from the
React props against the schema each native component declares
(`#[derive(ComponentProps)]`): a presence mask, then the declared fields in schema
order as their wire types, with no keys. Native decodes it positionally into the
same typed props through the same serde derives. At 5,000 rows it halves the
mount decode against JSON, 1.5 ms to 0.7 or 0.8 ms, and the wire drops from
825 KB to 435 KB. Retained state is identical to the byte: the wire changes only
what is in flight. The remaining mount allocations are the typed payloads
themselves, one box, one string, and one style handle per text, and the apply
is about 0.28 ms.

The JSON floor was the tokenizer: skipping the 825 KB text costs 0.68 ms with
`serde_json` and 0.70 ms with a SIMD parser, so a faster parser was not an
alternative to a smaller wire. Native reading the JavaScript objects through
N-API costs 2.0 ms at best. Both are recorded in the
[implementation decisions](../reviews/react-gpui-architecture/implementation-decisions.md).

Per-node retained cost at 5,000 rows, with the heap behind each reference counted:

| Record | Before | After |
| --- | ---: | ---: |
| `Text` row | 80 B, plus a boxed paint record when measured | 48 B; key and paint record in a per-kind side map |
| `Container` row | 32 B, plus an `Rc<ScrollHandle>` and a boxed rare block | 16 B; handles, label, group, and paint record in side maps |
| Document text options | 24 B | 8 B |
| Document cache record | 88 B, plus a 16-byte empty match list per text | 64 B, no allocation without a search hit |
| Document paint entry | 152 B | 136 B |

The empty draw after removing 5,000 rows takes about 2.4 ms with a document root and
0.9 ms with a container root, in both modes. It is document teardown in GPUI, not
retained bridge state: the bytes freed by the clear are identical for bridge and raw.

The earlier report showed the flow scene growing by about 3.5 KB per text node after
the first draw for both the bridge and the old renderer. That was GPUI's layout
engine: `TaffyTree::clear` drops nodes but not their contexts, so every measured
element's closure, with its captured text layout, survived until the next frame
overwrote its slot. The fork now drops them at the end of the frame. Live bytes after
the first draw fell by about 17 MiB for the raw view and 20 MiB for the bridge at
5,000 unvirtualized rows.

## Worker side

`fixtures/counter/js-bench.tsx` measures the same scene on the application
worker with a recording transport: React render and commit, sealing to the wire,
the wire size, and the JavaScript heap. React's production build, Bun 1.4.2,
medians of five in-process runs, checked across three processes; run-to-run noise
is about 0.3 ms. The binary encoder does its props work inside the commit, so it
shows under "render and commit", not under "seal". Earlier versions of this table
were measured with React's development build, which doubles the render time.

| Case, 5,000 rows | Wire | React render and commit | Seal | Cold first mount | Wire size | JS heap after mount |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Mount, list | JSON | 3.6 ms | 0.46 ms | 9.6 + 1.0 ms | 825 KB | 5.7 MB |
| Mount, list | binary | 3.6 ms | 0.03 ms | 9.1 + 0.3 ms | 435 KB | 5.7 MB |
| Mount, flow | JSON | 3.7 ms | 0.44 ms | 9.6 + 1.0 ms | 825 KB | 5.7 MB |
| Mount, flow | binary | 4.1 ms | 0.03 ms | 9.9 + 0.3 ms | 435 KB | 5.7 MB |
| Mount, list, rows memoized | JSON | 4.3 ms | 0.50 ms | 9.8 + 1.0 ms | 825 KB | 7.1 MB |
| Mount, list, rows memoized | binary | 5.2 ms | 0.03 ms | 10.8 + 0.3 ms | 435 KB | 7.0 MB |
| One-line update, rows not memoized | JSON | 1.44 ms | 0.01 ms | | 121 B | |
| One-line update, rows not memoized | binary | 1.50 ms | 0.01 ms | | 39 B | |
| One-line update, rows memoized | either | 0.30 ms | 0.00 ms | | | |

Sub-millisecond differences between wires in that table are where garbage
collections land, not encoder cost. The bench forces a collection before each
mount, and each path's allocation profile then decides whether the next
collection falls inside the commit or inside the seal. With the collector's
threshold raised (`BENCH_BALLAST=32`, 32 MB of retained objects) and eight
warm repeats, the wires tie at 5,000 rows:

| Encoder, list, 5,000 rows, GC pacing controlled | Render and commit | Seal | Total |
| --- | ---: | ---: | ---: |
| JSON at seal | 2.1 ms | 0.4 ms | 2.5 ms |
| Binary at seal over recorded operations, two walks | 2.1 ms | 0.4 ms | 2.5 ms |
| Binary in the commit, one walk | 2.5 ms | 0.02 ms | 2.5 ms |
| Same three with memoized rows | 2.4 to 2.8 ms | | 2.9 ms each |

The encoder costs about 0.4 ms wherever it runs, and the filter-and-copy walk
it replaces is not measurable at this size, so the single walk buys nothing on
its own. The apparent memoized-rows regression was the same pacing effect. The
JavaScript encoder starts at the interpreter tier, so a cold first mount pays
about 0.5 ms more in the commit than a warm one does, and still less than
JSON's cold seal.
The bridge's own host records are about 0.78 MB of the JavaScript heap; the rest
is React's fiber tree, the element objects, and the props objects. Raw data is in
[bridge-js-cost.json](./benchmarks/bridge-js-cost.json).

Raw medians are in [bridge-frame-cost.json](./benchmarks/bridge-frame-cost.json).
Reproduce with:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  bun scripts/measure-frames.ts /tmp/gpui-react-frame-cost
```
