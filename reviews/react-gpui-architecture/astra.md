I recommend a hybrid design: immutable Rust host descriptions with structural sharing, a React worker that owns application logic, and UI-owned GPUI entities for interactive state. Prepare data and revision differences outside the UI thread. Keep GPUI as the layout authority, with independently invalidated native view subtrees and native collection components. This takes the useful revision model from Fabric without assuming that arbitrary GPUI elements can run on a worker. Its main cost is an explicit limit on synchronous React layout semantics.

The inspected checkouts match the brief: GPUiX `08ffd4bfd70c638be6a8025548ba4234a5ff1edd` and GPUI `bea32f070b9fe5286081f1ea8730ad3eca890ba2`. Source observations below describe those versions. Proposed contracts describe a new implementation.

The current renderer already sends mutations. React retains speculative host nodes in JavaScript until placement, then sends one batch. Native code parses operations and resolves styles before it mutates the tree. The worker host calls that same function on a second mutable tree, then sends the batch for UI application. This provides immediate validation and destroyed IDs, but repeats work. It does not provide worker layout. See [host-config.ts](/Users/erwin/Code/gpuix/packages/react/src/reconciler/host-config.ts:241), [batch-renderer.ts](/Users/erwin/Code/gpuix/packages/react/src/reconciler/batch-renderer.ts:62), [prepare_batch](/Users/erwin/Code/gpuix/packages/native/src/renderer/host_runtime.rs:529), and [apply_batch_to_tree](/Users/erwin/Code/gpuix/packages/native/src/renderer.rs:6303). I also verified mutation transport, motion state, and virtual lists in baseline `7ac9880a`.

The alternatives have different costs:

| Design | Benefit | Main limit | Decision |
|---|---|---|---|
| A. One mutable UI tree, asynchronous commits | Small implementation; one mutable description model | UI must apply every required delta; synchronous validation and queries need different contracts; large commits delay input | Best for small, bounded applications |
| B. Immutable shadow revisions with worker layout and prepared mounts | Shared data; speculative preparation; consistent layout queries for a defined layout system | GPUI has no general worker layout contract; native controls require state integration; mount still costs UI time | Best if a restricted layout system or substantial GPUI changes are acceptable |
| C. Immutable descriptions with UI-owned GPUI view subtrees | Shared revisions and preparation without duplicating interactive state or general layout rules | UI layout remains a possible bottleneck; fresh synchronous layout effects are limited | Preferred for rich chat, editors, tables, and third-party GPUI components |

Fabric supports immutable shadow nodes, structural sharing, revision diffing that can skip intermediate trees, and native state updates such as scroll offset. Host views remain separate objects. These are useful precedents for C. [Fabric render pipeline](https://reactnative.dev/architecture/render-pipeline).

Fabric is not exclusively an asynchronous bridge. Its threading document describes layout on the JavaScript thread and selected synchronous UI-thread render paths. Its New Architecture document describes measurement and effect updates without intermediate visible layout. Those guarantees require scheduling and layout integration; an immutable tree alone cannot provide them. [Threading model](https://reactnative.dev/architecture/threading-model), [synchronous layout and effects](https://reactnative.dev/architecture/landing-page#synchronous-layout-and-effects).

The ownership model should be explicit:

| Owner | Data and operations |
|---|---|
| React worker | Fibers, application data, callbacks, speculative host handles, application commit order |
| Shared immutable Rust data | Host descriptions, typed props, child sequences, text, prepared documents, revision roots |
| Native preparation workers | Parsing, syntax analysis, safe text preparation, description diffing, cancellable resource preparation |
| UI thread | GPUI App and Window, entities, focus, capture, IME, editor buffers, scroll anchors, animation clocks, layout, hit testing, scene construction |
| GPU/backend owner | Device resources, queue submission, texture lifetime, frame completion |

Use the React reconciler's persistence mode. Native create and clone operations construct descriptions only. They must not create live editors, subscribe to input, or start animation during speculative render. Abandoned work releases its descriptions. Stable identity belongs to the host instance; each description has a separate version.

```rust
// Proposed data model. These are not existing APIs.
struct HostKey { surface: u64, slot: u64, generation: u64 }
struct Description {
    key: HostKey,
    version: u64,
    props: Arc<TypedProps>,
    children: PersistentSequence<Arc<Description>>,
    subscriptions: Arc<SubscriptionVersions>,
}
struct Revision { number: u64, root: Arc<Description> }
struct PreparedCommit {
    base: u64,
    next: Arc<Revision>,
    changed_instances: Vec<HostKey>,
    resources: PreparedResources,
}
// A separate UI-only table owns live GPUI component instances.
```

There is one immutable description graph with several roots, rather than two independently mutated validation trees. UI instances reference shared props. Keep parent relations in revision-specific indexes, not mutable back-pointers in shared nodes. Use chunked child sequences so an append need not copy a huge child array. React may still enumerate siblings when it clones a host parent; structural sharing does not remove that cost.

Publish native-owned `Arc` values through a bounded channel. No JavaScript object or callback crosses threads. A short queue lock is acceptable. Finalizers release description handles, but must schedule UI-resource destruction on the UI thread. This proposal does not claim lock-free operation or zero-copy JavaScript strings.

GPUI provides a firm feasibility boundary. [`Element`](/Users/erwin/Code/gpuix/zed/crates/gpui/src/element.rs:53) requires mutable `Window` and `App` for layout request, prepaint, and paint. [`TaffyLayoutEngine`](/Users/erwin/Code/gpuix/zed/crates/gpui/src/taffy.rs:24) stores measurement closures with those arguments. [`ListState`](/Users/erwin/Code/gpuix/zed/crates/gpui/src/elements/list.rs:56) uses `Rc<RefCell<_>>`. Moving these objects to a worker is not a supported architecture.

Move pure document parsing, syntax spans, prop validation, resource decoding, and revision diffing outside the UI thread. Text preparation is possible where the backend permits it. `PlatformTextSystem` is `Send + Sync`, and GPUiX already measures text through a worker-local `WindowTextSystem`. However, shared text-system locks can still delay UI shaping. A thread-safe API does not prove low contention. See [platform text contract](/Users/erwin/Code/gpuix/zed/crates/gpui/src/platform.rs:1121) and [worker text measurement](/Users/erwin/Code/gpuix/packages/native/src/renderer/host_runtime.rs:550).

Keep arbitrary custom rendering, live editor measurement, virtual-list range resolution, focus, IME, selection geometry, overlays, and final scene work on the UI thread. Native effects must use that same scene order.

Create GPUI entity boundaries for panels, native documents, collection viewports, and editors. Do not create an entity for every React component. GPUI already supports cached entity views, but they require definite outer sizes. The cache also checks bounds, clipping, text style, and invalidation. Scrolling changes bounds and can defeat reuse. Intrinsic-height chat rows therefore need document preparation and row measurement caches, not indiscriminate `.cached()` calls. [View caching](/Users/erwin/Code/gpuix/zed/crates/gpui/src/view.rs:224).

A necessary general GPUI change is support for replayable frame metadata. GPUI's cache replays its input handlers, hitboxes, text-layout references, and scene records. GPUiX separately clears and rebuilds selection, text inspection, and bounds records through paint callbacks. Cached scene replay skips those callbacks. Store these records as ordered frame attachments with cache replay and transaction rollback support. Selection and search changes must invalidate affected records and washes. Otherwise cached text can remain visible while becoming unselectable or absent from automation. [GPUI replay](/Users/erwin/Code/gpuix/zed/crates/gpui/src/window.rs:3482), [selection reset](/Users/erwin/Code/gpuix/packages/native/src/text/paint.rs:102), [bounds reset](/Users/erwin/Code/gpuix/packages/native/src/automation.rs:50).

I would not make a second general layout engine the default. If benchmarks require worker layout, add an opt-in GPUI contract for pure layout inputs, immutable measurement resources, and prepared layout adoption. Preserve GPUI's scale conversion and pixel rounding. Unsupported children require explicit intrinsic-size contracts or UI layout. Recomputing the same layout on the UI thread would lose much of the benefit. This path needs a prototype, not a promise that Taffy alone solves it.

Define six separate milestones. React render builds a candidate. React commit accepts descriptions and updates refs. Preparation produces native resources and a difference from a known mounted revision. Mount updates UI instances at a safe boundary. Layout resolves current native state into geometry. Submission sends a completed scene to the GPU. Physical presentation is a separate backend observation where available.

`flushSync` completes React work; it does not promise native layout or presentation. Publish synchronous geometry as a coherent snapshot carrying revision, frame, viewport epoch, and native-state versions. Never combine a new description tree with old bounds and call the result current geometry. An asynchronous `measure(ref, {after: revision})` returns the actual measured revision. An exact-revision query creates an ordering barrier or returns `superseded`.

Run `useLayoutEffect` in normal React commit order. Seal the outgoing commit group only after its synchronous effects and nested commits complete. Focus and scroll commands issued there travel with that group. The current host uses a microtask for this purpose; the installed reconciler calls `resetAfterCommit` before layout effects. [Application grouping](/Users/erwin/Code/gpuix/packages/react/src/application.ts:83).

This preserves command ordering, but does not supply fresh synchronous geometry for the candidate. A snapshot getter must expose its age. `useEffect` also does not prove that native pixels appeared. React's documented layout-effect behavior includes measurement and corrective rendering before paint. C cannot offer that general guarantee across arbitrary GPUI components while keeping the UI independent of blocked JavaScript. Use native anchor relationships, scroll groups, and layout policies for interactions that must stay aligned. This is a deliberate React semantic gap. [React useLayoutEffect](https://react.dev/reference/react/useLayoutEffect).

The consistency rule is that React owns declarative intent and native components own transient interaction. A new description must not overwrite native state merely because it carries an older observation. Track viewport, scale, font, document, and component-state dependencies separately. Avoid one global epoch that invalidates all preparation on every pointer move.

For example:

1. UI displays revision N. The editor is at edit version 40; a list has a native anchor.
2. React prepares N+1 with a message append and an acknowledgement of edit 40.
3. Native input advances the editor to 41. A wheel event moves the list anchor. Resize advances the viewport epoch. These operations do not wait for React.
4. Parsing for N+1 remains valid. Width-dependent preparation fails its dependency check and is discarded or recomputed. The stale editor acknowledgement cannot replace version 41.
5. UI mounts N+1, preserves the current anchor by item key, and performs layout at the current size. Every deferred row callback reads the same pinned revision as the root.
6. UI publishes a complete frame snapshot and ordered acknowledgements. React then receives edit 41 and prepares its next response.

The pinned frame revision matters because current virtual rows re-enter through `cx.processor` after root rendering. Their source cannot change halfway through a frame. [build_virtual_child](/Users/erwin/Code/gpuix/packages/native/src/renderer.rs:3476).

Controlled input needs edit sequence numbers, not string equality. Accept an acknowledgement without resetting caret, undo, or composition. Apply an application replacement only against its declared edit version; reject stale replacements with the current snapshot. During composition, defer replacements unless an explicit command ends composition. Native validation handles constraints that must reject a keystroke immediately. Asynchronous JavaScript cannot synchronously cancel an input already processed. The current input uses a bounded list of pending strings, which illustrates the distinction but is weaker than a version contract. [Input synchronization](/Users/erwin/Code/gpuix/packages/native/src/custom_elements/input.rs:719).

Focus and scroll are ordered intents with command IDs and target generations. Scroll-to-item uses a stable item key. A conditional intent can require the observed interaction version; an explicit unconditional command may override newer user movement. Header, body, and frozen columns share native scroll state. GPUiX already demonstrates horizontal sharing through [`ScrollGroups`](/Users/erwin/Code/gpuix/packages/native/src/scroll_groups.rs:5).

Events carry native sequence, mounted revision, target generation, subscription version, and relevant interaction version. Dispatch to the callback associated with that mounted subscription. Retain old callback versions until the UI event watermark passes them. Thus a delayed event can call a closure from an older React commit. A callback change or React unmount must not immediately erase handlers still referenced by queued native events. Discrete events receive React priority; native default actions do not wait for them. A drag that must continue while JavaScript stalls needs a native controller with committed constraints.

Scheduling needs two classes of work. Replaceable descriptions may coalesce. Commands, exact measurements, edit acknowledgements with required effects, and resource lifecycle operations retain order. If revision N creates a target for a focus command, N is a barrier even when N+1 removes that target. A native component constructor must have no application side effects that depend on every intermediate description becoming visible.

Prepare against the actual mounted revision. If the base changes, recompute the difference before admission. Cancel superseded preparation cooperatively. UI mount must not perform whole-tree JSON parsing or unbounded document analysis. Large mounts need explicit content limits, virtualization, or staged preparation with an atomic visibility switch. Work that creates GPUI entities still consumes UI time. A root-pointer swap does not make that work constant-time.

Current host slices stop after 32 transactions or four milliseconds between transactions. One transaction can exceed that budget. Input queues allow 256 records and 4 MiB; native output allows 4096 records and 4 MiB. Overflow or failure is explicit. Mutation parsing is atomic; the outer command transaction executes commands sequentially and has no general rollback. Preserve that distinction. [Host execution](/Users/erwin/Code/gpuix/packages/native/src/renderer/host_runtime.rs:291).

Bound admitted revision bytes, preparation jobs, semantic command records, and event bytes. Keep the mounted revision, one newest replaceable candidate, and a bounded queue of command barriers. Reserve capacity before accepting a commit; expose asynchronous pressure through the root scheduler. Prototype the reconciler integration rather than throwing after React has advanced and assuming it can retry. Coalesce pointer and scroll observations only when they represent replaceable state. Do not coalesce edit operations or discrete actions silently.

Bounded memory, lossless unlimited events, and an indefinitely blocked worker cannot all be guaranteed. On saturation, fault the application channel explicitly, preserve the native view and editor state, and require resynchronization. The UI must never wait for JavaScript. Schema failure leaves the prior revision active. A command failure reports a result without claiming rollback of earlier commands. A native panic remains a process risk. The initial window should remain hidden until the first accepted scene and initial commands have produced a frame.

Use an engine-neutral Rust core with typed host operations and opaque handles. N-API is sufficient for Bun and Node adapters; it need not carry JSON. A JSI adapter can serve an engine that supports it, but is not a prerequisite for direct native calls. Binary batches can reduce per-call cost; handles can avoid retransmitting immutable documents. Both need measured conversion costs and explicit lifetimes. N-API's ABI guarantees do not guarantee identical engine behavior. [Node-API](https://nodejs.org/api/n-api.html), [Fabric's JSI and code generation](https://reactnative.dev/architecture/fabric-renderer).

Generate TypeScript props, Rust validators, events, and commands from one schema. Keep the same semantic protocol for browser adapters, but report capabilities separately. Browser thread access, fonts, GPU handles, and worker facilities require their own validation. The macOS/Bun host does not establish support elsewhere.

The native-component API should separate pure preparation from UI instances. A descriptor supplies typed props and optional `prepare(props, resources, cancellation)` returning immutable data. Its UI factory creates an instance with GPUI context. The instance applies props, handles typed commands, renders, exports versioned state, and releases resources. Preparation cannot receive `Window`, `Context`, or JavaScript callbacks. UI extensions retain full GPUI access and use common text, bounds, accessibility, selection, and frame-metadata services.

Keep consumer dependencies and shaders in consumer crates. Statically compose one matching core/GPUI build. Add an exact build fingerprint covering core, GPUI, schemas, extensions, and backend features; require agreement in host and worker before attachment. The current catalog checks API and extension versions but has no such fingerprint. [Extension contract](/Users/erwin/Code/gpuix/packages/native/src/extension.rs:29).

Keep the existing GPU composition principle: shared device and queue, premultiplied textures, masks, corners, opacity, and scene order. Float RGB can exceed alpha. The producer owns animation, cancellation, and reduced motion. Frame references retain textures until safe release; backend completion governs resource reuse. [GPU texture contract](/Users/erwin/Code/gpuix/docs/gpu-textures.md).

Memory must scale with mounted content and bounded retained history. Native virtualization alone leaves React Fibers and descriptions for all children. Window both layers, pin focused editors, and page collection metadata. If a blocked worker cannot supply an unseen row, show a defined placeholder. Unlimited ready content and bounded memory are incompatible. Native data-owning documents can render loaded content without new React work.

Share text, styles, parsed documents, and unchanged child chunks. Cache shaping by text, font identity and epoch, style, and width constraints. Account for both JavaScript and native copies. Use commit deltas for focus and instance bookkeeping; avoid scanning all descriptions during each scroll frame. Limit snapshot leases and old revisions; move large immutable-data reclamation off the UI thread. Slice UI-instance teardown after immediate logical removal. Arbitrary application data and suspended React work still require application memory limits.

The strongest objection to C is that it adds revision machinery while leaving the most expensive layout on the UI thread. It also lacks Fabric-style general synchronous layout effects. I would choose A if bounded, windowed workloads meet latency targets without substantial preparation. I would choose B if UI layout remains dominant after native document preparation and view partitioning, or if exact synchronous layout effects are a mandatory product requirement.

Four prototypes can decide:

1. Compare A, C, and a restricted B on a 10,000-message chat, a 100,000-record table with 100 mounted rows, and an editor. Measure p50/p95/p99 input-to-submission, UI build/layout/paint, queue age, FFI costs, allocations, and peak memory. On the chosen 120 Hz target, require p99 UI work within 8.33 ms. Separate submission from physical presentation.
2. Block JavaScript for two seconds while typing with IME, scrolling linked panes, dragging through a native interaction controller, and running an effect. Fail on lost composition, scroll tearing, or UI waiting for application work.
3. Force resize, font changes, edits, cancellation, queue saturation, and create-focus-remove sequences during preparation. Fail on stale geometry admission, duplicate commands, wrong callback versions, or unbounded retained revisions.
4. Cache a text subtree through unchanged frames, then select, search, scroll, and remove it. Verify pixels, text inspection, bounds, accessibility, and input. Measure whether intrinsic rows and moving bounds leave enough cache reuse to justify C.

Reviewer: GPT-6 Astra, Extra High, as assigned. I read the complete brief, README architecture and relevant API sections, all required source areas, GPUI layout/list/text/entity/view code, and the four primary Fabric references. I did not read other reviews, delegate, edit source, build, or launch applications. Performance gains, portable worker text behavior, persistence-renderer overhead, and commit-group integration remain unverified until prototypes run.


---

Addendum: assumption check.

1. Strong A removes transport, parsing, native-state ownership, and view partitioning as reasons to prefer C. Immutable revisions still provide stable topology snapshots for concurrent readers, cheap retention through structural sharing, speculative cancellation, and diffing between chosen revisions. A can obtain these properties through copied snapshots, journals, or versioned topology, with different costs. C is not intrinsically faster at mounting or scrolling. My preference is narrower: choose C for concurrent preparation and revision inspection; choose strong A if those benefits do not repay allocation and lifetime complexity.

   Parsing and type validation can be atomic without any topology copy: construct a complete typed operation buffer before publication. Reference validity, generations, parent membership, and cycle checks require topology knowledge. Keep authoritative validation on UI, against the ordered base plus a temporary overlay of batch changes, before applying operations. The overlay need not duplicate the whole tree, although worst-case validation can traverse it. Destroyed IDs and callback retirement can return through ordered acknowledgements. Synchronous worker topology validation requires a sufficient topology index, a construction API that proves the relevant constraints, or a UI response. Parsing atomicity alone is not topology validation. The current [mutation methods](/Users/erwin/Code/gpuix/packages/native/src/retained_tree.rs:275) even tolerate some missing references.

2. I overstated the required implementation location. A GPUiX-owned per-entity metadata cache with explicit ordered replay is viable. For subtrees without escaping deferred draws, a wrapper can replay the cached records at its paint position. GPUiX can restrict caching to these subtrees without adding general frame attachments.

   General coverage needs more information. GPUI [replays cached deferred draws](/Users/erwin/Code/gpuix/zed/crates/gpui/src/window.rs:3355) later, in priority order, sometimes without calling an element. Replaying all metadata beside the originating view would place overlay text incorrectly. GPUI must expose equivalent replay-segment identities and traversal notifications, with effective clipping and placement. Alternatively, the binding must control every affected deferred draw and reproduce those semantics. Metadata needed during prepaint requires transaction notifications or provisional storage until [rollback](/Users/erwin/Code/gpuix/zed/crates/gpui/src/window.rs:3718) is impossible. Recording only during final paint avoids rollback hooks. Nested cached segments must replay once in actual frame order. These semantics are necessary; GPUI-owned storage is my preferred integration, not the only valid design.

3. A pure-layout/adoption design is viable and offers stronger semantics. Extract a shared GPUI layout contract for flex/grid/text and explicit native-leaf measurement snapshots. Use the same constraints, font resources, rounding, and layout results for measurement and adopted rendering. Unsupported native children need declared intrinsic-size behavior or a documented UI-only boundary.

   Compute candidate layout before layout effects, using native code on the application thread or a completed preparation job. Effects synchronously read that candidate and can produce another candidate. Seal the group after those effects settle. UI continues operating its previous revision while JavaScript works, then adopts the completed group without displaying intermediate candidates. Application JavaScript never needs to run on UI.

   Candidate geometry is exact for its captured inputs; it is not necessarily current screen geometry. At adoption, validate viewport, fonts, intrinsic-size state, and every native state value used by measurement. Rebase only dependency-independent transforms. A mismatch invalidates the candidate and requests another layout/effect pass. Ordinary `useLayoutEffect([])` does not automatically rerun: dependency-aware measurement hooks or explicit reconciler integration are required. External effect side effects cannot be rolled back. Fresh live geometry still requires coordination; immutable candidates do not remove that constraint. Continuous invalidation can delay adoption indefinitely. Native layout relationships should remove avoidable scroll and caret dependencies.

   I would prefer this design over C after a prototype shows exact measurement/adoption agreement, no intermediate candidate frames, correct retries under resize/font/IME races, and continuing native interaction during blocked JavaScript. It must also support representative third-party components, bound retries and memory, and meet the stated 8.33 ms p99 UI budget. If C misses that budget because of layout while adoption meets it, I would switch. If both pass, demonstrated need for synchronous candidate measurement would decide, rather than current API difficulty.
