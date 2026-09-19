# React bindings on GPUI: independent architecture review (Fable 5.1), corrected

## Correction note

This is one bounded correction pass on my first report. I re-read source, not other reports, and changed no source. The pass strengthened alternative A, corrected the persistent-mode and memory claims, moved publication after layout effects, withdrew the presentation hold and the panic boundary from the baseline, reclassified the island cache as an opt-in proposal with prerequisites, replaced the global native version with per-instance versions, and removed invented numbers. The recommendation did not survive unchanged. Once A receives typed worker-validated ops, ordered pinned intents, versioned snapshots, and a bounded admission queue, the benefits that remain intrinsic to shared immutable revisions are not decisive for the stated workloads, and their costs are real. The corrected recommendation is A′ below. The decisive choices that do survive are: layout stays in GPUI on the UI thread, native state is UI-owned and versioned per instance, intents are id-addressed and applied in commit order, the transport carries typed data once, and measurement semantics are stated as a gap instead of hidden.

## Recommendation

Build the bindings as **one UI-owned mutable retained tree fed by ordered typed transactions, with no worker-side Rust tree** (A′). React keeps the mutation host config. The application worker validates prop schemas with generated decoders, encodes each commit into one typed buffer, and seals it with the intents that its layout effects produced. The UI thread applies transactions in commit order, each atomically, and never skips one. The UI tree, the instance table of GPUI handles, layout, hit testing, IME, selection, and presentation stay on the UI thread. Native state such as scroll offsets, editor text, IME composition, focus, hover, and animation clocks is UI-owned, and each instance carries its own version counter. Events and snapshots carry the applied commit sequence and the relevant instance versions. React sends intents that address stable ids and that are pinned to the commit that produced them. The user's hypothesis is answered as follows: a shared immutable revision model does give consistent multi-reader snapshots and O(1) coalescing under a UI stall, but in-order delta application gives equal or stronger ordering semantics for these workloads, and the revision model's costs, which are speculative render-phase nodes across N-API, persistent-map and wide-children copies, JS-held descriptions, reclamation, and persistent-mode novelty, are not paid back. The performance gains that are available come from a single-copy transport and, if a prototype proves it, an opt-in GPUI island cache. Both apply to A′.

## Facts verified in source

- Mutation mode, commit-phase materialization, and full-style resend on update: `packages/react/src/reconciler/host-config.ts:238-260,399-414`. One commit becomes one `applyBatch(JSON.stringify(queue))`: `batch-renderer.ts:67-80`.
- Reconciler ordering in `react-reconciler` 0.31: `commitMutationEffectsOnFiber`, then `resetAfterCommit`, then `root.current = finishedWork`, then layout effects (`react-reconciler.development.js:15611-15621`). Persistent mode calls `cloneInstance` inside `updateHostComponent` during `completeWork`, which is the render phase (`:9941-9952`). Abandoned renders therefore do create native descriptions in persistent mode.
- Today the worker applies each batch to a second `RetainedTree` in `prepareBatch`, then queues the same JSON (`application.ts:152-176`, `host_runtime.rs:527-535`). The commit text is JSON-encoded three times and parsed four times on its way to the UI tree (`application.ts:64-75`, `host_runtime.rs:451-478,300-304`, `renderer.rs:6303`). The layout-effect commands join the batch because the send happens in a microtask after the synchronous commit (`application.ts:77-90`).
- `GpuixView::render` locks the tree and rebuilds every non-virtualized element each frame (`renderer.rs:4395-4575`). Every commit calls `window.refresh()` (`renderer.rs:331-336`). GPUiX uses no `Entity::cached` or `use_keyed_state`.
- GPUI is UI-bound at every element phase (`element.rs:73-104`), `Window` holds `Rc` fields (`window.rs:1140-1222`), `draw` clears Taffy each frame (`window.rs:2995`), Taffy measure closures take `&mut Window, &mut App` (`taffy.rs:24-26`), `ListState` is `Rc<RefCell<..>>` (`list.rs:56`), input dispatch is synchronous on the platform callback (`window.rs:1756-1762`).
- GPUI's cached view reuses prepaint and paint only when bounds, content mask, and text style match, the entity is not dirty, and `window.refreshing` is false (`view.rs:384-395`). `mark_view_dirty` dirties the whole ancestor view path (`window.rs:1955-1967`). `Window::refresh` sets `refreshing` (`window.rs:2032-2037`). GPUI's interactivity calls `window.refresh()` on scroll wheel, press, release, click, drag, and active-state paths (`div.rs:2633,2874-2958,3042-3050,3186-3207`) and `cx.notify` on hover (`div.rs:2814,2836`). So any frame with input on an interactive element, and every commit frame today, disables view caching frame-wide.
- `draw` swaps `rendered_frame` and `next_frame`, moves input handlers, and fires focus listeners before `present` (`window.rs:2997-3047`). `present` is a separate step in the frame callback (`window.rs:1627-1640`).
- GPUiX records element bounds, selection start regions, painted text, and highlight ordinals inside paint closures, and clears the registries each frame through reset canvases (`automation.rs:40-66,146-155`, `renderer.rs:4484-4503`). `reuse_paint` replays scene ranges and listeners but does not run paint closures (`window.rs:3546-3590`).
- `LineLayoutCache` keeps a `previous_frame` and a `current_frame` and swaps them at `finish_frame`, so a shaped line survives one idle frame (`line_layout.rs:559-573`). The worker builds a fresh `WindowTextSystem` per measurement call, so it has no cache across calls (`host_runtime.rs:563-572`). `PlatformTextSystem: Send + Sync` (`platform.rs:1121`).
- After every paint the host clones the bounds of every painted element into a snapshot (`host_runtime.rs:45-112`).
- Fabric: shadow nodes are created synchronously from JS through JSI, the shadow tree is immutable with structural sharing, mount runs on the UI thread, the diff can skip intermediate trees, C++ state such as scroll offset commits through a retry loop. The render-pipeline page places commit on a background thread; the threading-model page places layout on the JS thread. Both keep layout off the UI thread by default.

## Alternatives

### A, strengthened

A is one UI-owned mutable tree that receives commits in order. Strengthened, it has these properties. Prop and schema validation is stateless, so the worker performs it with generated decoders before it sends anything; malformed data throws synchronously in the React commit, as `prepareBatch` does today. Topology validation, which covers id existence, parent and child relations, cycles, and the root, is a different class: for a correct binding the reconciler guarantees it, and the JS host config can keep parent and children links on its own instances, which it already partially does. That JS instance tree derives destroyed ids and serves the automation tree without any Rust mirror. The UI tree checks topology in debug builds and treats a violation as a binding bug, not as recoverable input. Each transaction carries a monotonic sequence, the snapshot carries the sequence of the last applied transaction and the per-instance versions, and the admission queue is bounded. Stale intents are handled by ordering: A applies every transaction, so an intent runs exactly against the tree state its commit produced. A can use any GPUI view cache that C can. Memory under a UI stall is the admission queue of deltas, which is smaller than a queue of revisions.

### B, Fabric-style immutable revisions with background layout

Rejected for the reason in the first report. GPUI rebuilds Taffy each frame and every element phase requires `&mut Window`. A precomputed-bounds mount API would exclude lists, inputs, and entity-backed extensions.

### C, shared immutable revisions, UI-thread layout

The benefits intrinsic to shared revisions, after A is strengthened, are three. First, any thread can read a consistent whole-tree snapshot without a lock and without a second copy, which serves automation, debugging, and cross-node background preparation. Second, under a UI stall the UI can jump to the newest revision in O(1) instead of applying every delta. Third, an event can reference the exact revision it was dispatched against. The costs intrinsic to C are: persistent-mode `cloneInstance` and `createInstance` run in the render phase, so abandoned concurrent renders allocate Rust nodes across N-API that must be discarded without mounting; a persistent id map and copied children arrays cost more per commit than a delta; JS instances that hold node handles, fiber alternates, pending intents, and event snapshots keep revisions alive beyond any publication slot; a wide parent copies its whole children array when one child changes; dropping the last reference of a large revision must not free a deep tree recursively on the UI thread. For these workloads, which are a windowed chat, windowed records, an editor, and native effects, the three benefits are not decisive, because per-node preparation happens at op-build time with the props in hand, automation can read the JS instance tree plus the snapshot, and stall coalescing saves only the cost of small deltas.

| Criterion | A′ (recommended) | C |
| --- | --- | --- |
| Schema errors | Synchronous, worker | Synchronous, worker |
| Topology errors | Reconciler-guaranteed, debug assert on UI | Reconciler-guaranteed, debug assert on UI |
| Ordering of intents | Every transaction applied in order, pinned | Barrier revisions must be mounted in order, others may coalesce |
| Abandoned renders | Never reach Rust | Reach Rust, must not mount |
| Memory under stall | Bounded delta queue | Bounded revision queue plus shared maps |
| Multi-reader model | JS instance tree plus snapshot | Immutable revision |
| Novelty | Mutation mode, mainstream | Persistent mode, RN only |

## Ownership and data model

```text
Application worker                                 UI thread (AppKit main, GPUI)
┌───────────────────────────────┐  typed txn (seq) ┌─────────────────────────────────┐
│ React fibers, hooks, handlers │ ──admission────► │ RetainedTree (single, mutable)   │
│ JS instance tree (parent,     │   queue, bounded │ InstanceTable by stable id       │
│  children, handlers)          │                  │  • focus/scroll handles, ListState│
│ Schema decoders (codegen)     │ ◄──Arc<Snapshot>─│  • extension instances, entities │
│ Text measure, fonts, syntax   │   seq + versions │  • per-instance versions         │
│ Snapshot mirror               │ ◄──events────────│ Per frame: elements, Taffy, hit  │
└───────────────────────────────┘  seq + versions  │  test, IME, a11y, scene, present │
                                                   └─────────────────────────────────┘
```

```rust
pub struct Transaction {
    pub seq: u64,                 // monotonic per session
    pub ops: TypedOps,            // decoded once on the UI thread, schema-checked in the worker
    pub intents: Vec<Intent>,     // sealed after layout effects, applied after ops, same slice
}
pub enum Intent {
    Focus { id: Id }, Blur,
    ScrollTo { id: Id, x: f32, y: f32, observed_scroll_version: u64 },
    ScrollToRow { list: Id, row: Id, offset_px: f32 },
    SetEditorValue { id: Id, value: String, observed_edit_version: u64 },  // also carried as a prop
}
pub struct Instance {             // UI thread only
    edit_version: u64,            // editors: advances on each user edit or IME event
    scroll_version: u64,          // scroll containers and lists
    pending_prop: Option<PendingValue>,   // newest deferred value only
    handles: InstanceHandles,     // focus, scroll, ListState, entity
}
pub struct Snapshot {             // published after each paint
    applied_seq: u64,
    viewport_version: u64,        // resize, scale, insets
    focus_version: u64,
    bounds: FxHashMap<Id, Bounds>,          // opt-in ids only
    editors: FxHashMap<Id, (u64, ..)>,      // edit_version, caret
    scrolls: FxHashMap<Id, (u64, Point)>,   // scroll_version, offset
}
```

Descriptions live in one place, the UI tree. Instances are keyed by stable id and exist while the id is in the tree. The worker keeps no Rust tree. It keeps the JS instance tree that React already gives it, and it keeps the native services it uses today: font registration, text measurement, syntax highlighting.

## Commit, seal, admission, mount, and presentation

1. **Render.** React renders. Host nodes are JS objects. Nothing reaches Rust.
2. **Mutation effects.** The host config appends typed ops to the staged transaction for this commit.
3. **`resetAfterCommit`.** Runs before layout effects. It closes the op section of the staged transaction with sequence N and leaves the transaction open for intents.
4. **Layout effects.** They run synchronously in the same task. Focus, scroll, and value intents append to transaction N. A nested `flushSync` inside a layout effect starts transaction N+1 in order; N stays open until the seal.
5. **Seal.** A microtask after the task seals every staged transaction in order and admits them to the bounded queue. Passive effects run later and their intents seal into a later transaction that has no ops. If the queue is full, the seal blocks the worker up to a deadline and then closes the session with an explicit reason. Blocking here is the backpressure signal: React has already committed, and the application cannot usefully continue until native accepts commits.
6. **Apply.** The UI command slice applies one transaction at a time: ops, then intents, with no draw between them. A draw may occur between transactions and shows a committed state.
7. **Draw and present.** GPUI runs as today.

Trace: create A in R1, `focus(A)` in an R1 layout effect, remove A in R2, both sealed before the UI consumes either. Transaction 1 is `(create A, Focus A)`. Transaction 2 is `(remove A)`. The UI applies transaction 1: A exists, its focus handle is created and focused. The UI applies transaction 2: A is removed and its handle dropped. The next draw finds the focus path empty and runs the focus-lost path. The result is deterministic: nothing is focused, which is what the DOM produces for the same sequence. The design pins intents to their transaction and never skips a transaction. Best-effort cancellation was rejected because it would leave focus on the previously focused element, which is not what the effect expressed. The cost of not skipping is bounded by the admission queue.

Intent rejection still exists for one case: an intent from a passive effect seals into a later transaction and may target an id that an intervening commit removed. The UI drops it and emits `intentRejected{id, reason, seq}`.

Callback lifetimes. Events carry `(id, applied_seq, instance versions)`. JS dispatches to the handler registered for the id at delivery time, which matches React DOM, where a handler reads current props. If the id has been unmounted, JS drops the event and counts it. Ids are never reused, so a late event cannot reach a new element. The JS instance tree is the version boundary: unmount removes the instance and its handlers when React deletes the subtree, which JS derives from its own children links.

## Consistency: per-instance versions

Each editor has an `edit_version`. Each scroll container and list has a `scroll_version`. The window has a `viewport_version` and a `focus_version`. No global counter invalidates unrelated work.

Controlled input rule. Native applies each edit first and emits `change{value, edit_version}`. A `value` prop carries the `observed_edit_version` React last saw for that editor. On apply, if `observed_edit_version` equals the instance's current version, the value replaces the text and the selection is restored. If the instance is ahead, the value is stored as the single pending prop, replacing any older pending prop, and it is discarded as soon as a newer edit occurs, because React will re-render with the newer observation. No keystroke is lost. The visible intermediate state is the native text. A synchronous reject of a keystroke is not possible in this design, as in React Native.

IME rule. A pending value is never applied during composition. At composition end the instance version has advanced, so a pending value with an older observation is discarded, not applied. A stale queued replacement therefore cannot overwrite newer composition text. Only a value that observes the post-composition version applies.

Scroll rule. `ScrollTo` carries the observed `scroll_version`. If the user scrolled since, the intent is dropped and reported, unless the app marks it `force`. `ScrollToRow` addresses a row id, so a window change cannot shift it.

## Measurement semantics

Default: a layout effect that reads geometry reads the last painted snapshot, tagged with `applied_seq` and versions. This is an explicit gap against Fabric, where layout of the new commit is available synchronously. The gap is the same one React Native had before the New Architecture, and GPUiX has it today.

Async measurement: `renderer.measure(id)` resolves after the next painted frame whose `applied_seq` is at or after the caller's commit. It does not make a layout effect wait, and a correction commit produces a visible second frame.

The presentation hold from the first report is withdrawn from the baseline. `draw` swaps the rendered frame, hitboxes, and input handlers, and fires focus listeners before `present`, so input during a hold would be tested against invisible geometry and focus callbacks would fire for unshown state. At the deadline the hold shows the intermediate frame anyway, so it cannot promise no intermediate paint. A correct version needs a measure-only draw path in GPUI that lays out into a scratch frame without swapping, without consuming element state, and with deferred draws handled. Its scope is unknown. It is a research item, not a commitment.

## Scheduling, backpressure, ordering, and errors

- **Admission queue.** Bounded by count and bytes. The seal blocks up to a deadline when full, then fails the session explicitly. No transaction is ever dropped or reordered.
- **Command slice.** As today, bounded by count and time, but it applies whole transactions and never yields inside one.
- **Event queue.** Discrete events are never dropped. Continuous events with the same `(id, type)` coalesce to the newest. Overflow closes the session explicitly, as today.
- **Priorities.** JS sets `DiscreteEventPriority` for key, click, and focus and `ContinuousEventPriority` for move, scroll, and hover before it calls the handler. Today everything is default priority.
- **Errors.** Schema errors throw in the React commit. Topology violations are binding bugs and end the session with a reason. A panic inside an element phase leaves GPUI's frame, arena, and element-state bookkeeping in an undefined state. The baseline does not claim recovery. Extensions must not panic, and a panic is a session failure.
- **Overload.** Input never waits on the application. Frame cost is bounded by virtualization and, if proven, islands. The application thread pauses only through the admission deadline.

## Transport and runtime

Serialization and scheduling remain separate. Scheduling is the queue above. Serialization is one typed buffer per commit.

- **Engine.** Bun and Node expose N-API; JSI exists only on Hermes. The design uses N-API. JS writes ops into one `ArrayBuffer` plus a string table. Rust reads it once inside the call through `napi_get_typedarray_info`. JS must not reuse the buffer until the call returns, which holds because the call is synchronous. Nothing is JSON on this path.
- **Schema.** One source generates TypeScript types, Rust decoders, and the field layout, as Fabric codegen does. A style is a bitmask of set fields plus values. Rust keeps content-interned styles and the sweep, which the existing benchmark identified as the largest lever on that fixture.
- **Baselines.** The numbers in `docs/serialization-benchmark.md` are measured baselines for the JSON path on the chat fixture. They are not predictions for this transport. The prototype below measures the new path on the same fixture.
- **Snapshot.** Bounds are recorded only for ids that opt in through a ref, a measure call, or a test id. Today every painted element records bounds and the map is cloned each frame.

## GPUI feasibility

Off the UI thread today: schema validation, op encoding, text shaping and width measurement through the shared `TextSystem`, syntax highlighting, markdown parsing at op-build time, image decoding, GPU texture production. UI-bound: element construction, Taffy, hitboxes, focus, IME, selection, accessibility, entity updates, presentation. Taffy stays on the UI thread for the reason given under B.

### Island cache: an opt-in proposal, not an established change

The first report presented content-sized cached islands as a small extension. That was wrong. The prerequisites are:

1. **Constraints.** `request_layout` receives no parent constraints. An island's intrinsic size can only be answered inside a measure closure, per probe of known dimensions and available space, and Taffy may probe min-content and max-content several times per layout. The memo must be keyed by probe inputs.
2. **Baselines.** `request_measured_layout` returns a size and no baseline. An island under baseline alignment, which GPUiX uses for `inlineFlow`, would misalign. Such islands must be excluded or the Taffy measure contract extended.
3. **Frame-wide refresh.** GPUI disables cached views for any frame in which `window.refresh()` ran. Interactivity calls it on scroll, press, release, click, drag, and active-state changes, and GPUiX calls it on every commit. Hover calls `cx.notify`, and `mark_view_dirty` dirties the ancestor path. GPUiX has one view entity, so islands need their own entities or an element-state cache, GPUiX must stop calling `refresh` on commit, and input-driven refreshes must become targeted invalidations. Entity boundaries alone do not prove changed-islands-only cost.
4. **Paint-time registries.** GPUiX records element bounds, selection start regions, painted text, and highlight ordinals in paint closures and clears them each frame. `reuse_paint` replays scene primitives but does not run those closures, so a replayed island would vanish from every registry. Bounds, selection, text inspection, and highlight state must become retained per-instance records updated on rebuild and preserved across replay, and selection or highlight changes must invalidate the islands they touch.
5. **Live native state.** Scroll offsets, motion clocks, caret blink, hover and active styles inside an island must invalidate it.

Only an opt-in prototype on the GPUI fork can show whether a net win remains after these changes. Until then, the frame-cost model is today's: one element build and one layout per non-virtualized element per frame, bounded by virtualization.

### Other GPUI items

- A shared, content-addressed line layout cache is a proposal, not an existing mechanism. Today a shaped line survives one idle frame in the window cache, and the worker shapes without any cache. A persistent cache with eviction would be new. Its value depends on hit rate, which is low for streaming text that changes every token. Prototype-gated.
- No GPUI change is needed for text shaping off-thread, GPU texture composition, or persistent focus and scroll handles.

## Memory and performance model

- **Tree memory.** One retained tree on the UI thread. The worker tree is removed. The measured bytes per element in the benchmark apply to the JSON fixture and the current representation only.
- **Queues.** Admission is bounded by count and bytes. Events are bounded as today.
- **JS.** The JS instance tree exists already; adding parent and children links adds two references per node.
- **Reclamation.** Unmounting a large subtree today walks it recursively on the UI thread. Under A′: unlink the subtree from its parent in O(1), then remove records from the map in bounded slices across command slices, and move the removed values, which own strings and prop maps, to a background thread for dropping. Instance pruning stays on the UI thread because GPUI entities are not `Send`, and it is O(instances), which is small. The same rule applies to C, where dropping the last `Arc` of a revision must not free a deep tree recursively.
- **Frame cost.** Unchanged from today until the island prototype proves otherwise.
- **Instrumentation.** Tag each transaction with seal, admit, apply, and present times. Tag frames with element build counts and, if islands exist, rebuild and replay counts.

What cannot be inferred without benchmarks: the per-commit cost of the typed path on the chat fixture, the island hit rate under hover and scroll, the shaping cache hit rate, and reclamation cost for a large unmount.

## Public binding and extension contract

The extension contract stays compiled-in with one GPUI revision. The trait changes to receive typed props and versioned services:

```rust
pub trait NativeElement: 'static {
    type Props: Decode + PartialEq + Send + Sync;     // generated from the TS schema
    type Prepared: Send + Sync + 'static;               // optional, computed at op-build time
    type Instance: 'static;                             // UI thread state, entities allowed

    fn prepare(props: &Self::Props, services: &Services) -> Option<Self::Prepared> { None }
    fn create(props: &Self::Props, prepared: Option<&Self::Prepared>, cx: &mut MountCx) -> Self::Instance;
    fn update(inst: &mut Self::Instance, old: &Self::Props, new: &Self::Props,
              prepared: Option<&Self::Prepared>, cx: &mut MountCx);
    fn render(inst: &mut Self::Instance, cx: &mut RenderCx, children: Children) -> AnyElement;
    fn destroy(inst: Self::Instance, cx: &mut MountCx);
    fn native_state(inst: &Self::Instance) -> Option<NativeState> { None }  // value or offset with its version
}
```

`Services` gives the shared `TextSystem`, fonts, syntax, and the GPU device and queue handles. `MountCx` and `RenderCx` give `&mut Window`, `&mut App`, the event sink with `(id, name, payload, applied_seq, versions)`, animation frame requests, the reduced-motion flag, and the selection and highlight services. Extensions never look up other nodes by id. GPU textures keep `paint_gpu_texture` with premultiplied alpha and scene order. Cherry keeps its components and shaders in its crate. The composition fingerprint should become an exact build hash checked by the loader in both contexts.

## Risks, the strongest objection, and decisive prototypes

The strongest objection to A′ is the one the first report made for C: only the UI thread holds a tree, so any sync whole-tree query on the worker depends on the JS instance tree, and any cross-node background preparation needs either JS data or a Rust mirror. If a future workload needs a consistent Rust model on the worker, for example a native document model shared by an editor and a search index, C's shared revisions become the right answer. The second objection is that in-order application under a long stall applies every delta; if commits are large and frequent during stalls, coalescing would help. Neither condition holds for the stated workloads.

Prototypes, with pass conditions relative to measured baselines on the same fixture and machine:

- **P1 Typed single-copy transport.** Measure bytes handled and worker plus UI CPU per commit for the JSON path and the typed path on the chat fixture. Pass: lower on both, and the existing React test suite passes unchanged.
- **P2 Ordered pinned transactions.** The `focus(A)` then `remove(A)` trace, a nested `flushSync` in a layout effect, and a 100-commit sequence during a 500 ms UI stall. Pass: outcomes exactly as specified, no lost or reordered transaction, memory bounded by the admission limits.
- **P3 Per-instance versioned input with IME.** A keystroke burst with a slow handler, then a composition that ends while a stale value is pending. Pass: no lost character, no composition overwrite, final value equal on both sides, no caret jump.
- **P4 Opt-in island cache on the GPUI fork.** With registries converted to retained records and commit-time refresh removed. Measure element build plus layout time when one of 40 visible rows changes, and during hover and scroll. Pass: a measured reduction with all interaction, selection, highlight, bounds, and IME tests passing. Failure keeps full rebuilds.
- **P5 Large unmount reclamation.** Unmount a 10,000-row list. Pass: frame time during reclamation stays within the frame budget measured at idle on the same machine, with a stated margin.
- **P6 Shared revisions, optional.** Only if P1 to P3 reveal a need for a consistent worker-side Rust model.

## Identity, sources, and open uncertainties

Model: Claude Fable 5.1, extra-high effort. This is my independent judgment across both passes. I read no other reviewer report.

Source inspected at the stated commits: `packages/react/src/reconciler/host-config.ts`, `batch-renderer.ts`, `event-registry.ts`, `reconciler.ts`, `application.ts`, `types/host.ts`; `node_modules/react-reconciler/cjs/react-reconciler.development.js` for commit ordering and persistence hooks; `packages/native/src/renderer/host_runtime.rs`, `retained_tree.rs`, `renderer.rs`, `extension.rs`, `custom_elements/mod.rs`, `custom_elements/input.rs` (survey), `automation.rs`, `text_measure.rs`, `element_tree.rs`; `zed/crates/gpui/src/element.rs`, `window.rs`, `taffy.rs`, `view.rs`, `text_system.rs`, `text_system/line_layout.rs`, `elements/list.rs`, `elements/div.rs`, `app.rs`, `app/context.rs`, `executor.rs`, `platform.rs`; README, AGENTS, `docs/serialization-benchmark.md`, `docs/native-extension-decisions.md`, `docs/gpu-textures.md`; Cherry `packages/native-components/src` registration and a grep survey; the four React Native architecture pages.

Open uncertainties:

- Whether targeted invalidation can replace GPUI's frame-wide `refresh` on input without regressions in Zed-derived behavior. P4 decides.
- Whether Bun's N-API typed-array path has the expected per-call cost. P1 decides.
- Whether blocking the worker at the admission deadline is preferable to failing fast as today. Both are explicit; the choice is a product decision.
- The measure-only draw path that a correct presentation hold would need has unknown scope in GPUI.
- Browser and Windows hosts were not assessed. The transaction model does not require threads, but the frame-loop assumptions are macOS-verified only.
