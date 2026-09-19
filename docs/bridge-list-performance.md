# Native list count updates

The new binding now updates the affected GPUI height-index range when a logical
list grows or shrinks. It preserves measured rows, focus ownership, and the
logical scroll anchor. It does not reconstruct the index or invalidate an
unchanged supplied row window on a count-only update.

These are release measurements on an Apple M5 Max with 128 GiB memory, macOS
26.6.2, and Rust 1.97.1. Each case has twenty warmup operations and two hundred
measured operations, alternating append and removal of one row. The binding
case retains sixty supplied native row views. Allocation counts use a test-only
allocator on the executing thread.

| Native operation | Logical rows | P50 | P95 | Allocated bytes per operation |
| --- | ---: | ---: | ---: | ---: |
| Binding before | 1,000 | 39.63 µs | 42.46 µs | 460,424 |
| Binding after | 1,000 | 1.46 µs | 1.92 µs | 8,720 |
| Binding before | 100,000 | 2,421.92 µs | 2,541.71 µs | 20,039,688 |
| Binding after | 100,000 | 3.04 µs | 3.21 µs | 11,792 |
| Direct GPUI hinted splice after | 100,000 | 3.13 µs | 3.33 µs | 11,792 |

The before versions are the previous implementation `6f867ea` and GPUI `bea32f070b`. The new GPUI
version is `25402064a7`. The test measures
native mutation work. It excludes React reconciliation, JSON encoding/decoding,
worker transport, layout, paint, and physical presentation. The small difference
between the two final native paths is measurement variation; it is not evidence
that the binding is faster than direct GPUI. These results are not an app FPS
claim or the complete framework performance comparison.

The old path called `splice`, then `with_uniform_item_height` on the whole list.
The latter traversed the index, allocated a temporary item vector and a new
index, and converted measured rows back to unmeasured entries. The new GPUI APIs
`splice_with_uniform_height` and `splice_focusable_with_uniform_height` assign
hints as they insert rows. Unaffected items retain their native measurements.
The existing initializer also now preserves measured entries and fills only
missing hints.

The binding additionally avoids clearing and reinstalling focus handles or
remeasuring supplied rows when their mapping and style are unchanged. Full-list
appends preserve measurements outside the changed range. A typed style
comparison checks whether inherited layout might have changed; it does not
retain another style model.

This exposed a related cache invalidation defect. A transaction can update one
retained row's props and insert a sibling. Structural synchronization must not
discard the retained row's prop notification. The host now distinguishes the
immediate topology update from changes inside retained child branches. A native
regression test failed before this fix. The GPU scenario also verifies a changed
row, a sibling append, and a negative anchor in one transaction.

Correctness checks cover measured-item retention, hint totals, focus handles,
anchor shifts, removal, an allocation-growth guard, and the existing GPU list
scenarios. The allocation guard compares one append at 1,000 and 100,000 rows;
it does not depend on machine timing. All 32 GPUI list tests pass.

The manual `list_cost` benchmark was retired when rows became host-owned data
rather than entities; the measurement-retention regressions run in the normal
controls test suite through a `Host`. [Raw results](./benchmarks/bridge-list-count.json) include the direct
unhinted lower bound and the old whole-index pattern. The unhinted lower bound
does less work and is not a substitute for the equivalent hinted comparison.

Full native frame cost, large transaction admission, destruction, rich content,
and the old published renderer comparison remain separate measurements.
