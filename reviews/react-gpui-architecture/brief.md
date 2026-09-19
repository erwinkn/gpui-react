# Independent design review: high-quality React bindings on GPUI

## Task and authority

The user requested independent child-thread reviews from Fable 5.1 Extra High, DeepSeek V4.1 Flash, GPT-6 Astra Extra High, and Gemini 3.8 Flash.

Recommend the optimal, efficient, and elegant architecture for React bindings on GPUI. The user knows React and Rust but is learning GPUI and React Native internals. Optimize the result for native interaction quality, coherent semantics, rendering performance, bounded memory, extensibility, and long-term maintainability. We are NOT constrained to preserve GPUiX's API, transport, implementation, or backward compatibility. A new binding implementation or general GPUI changes are in scope for the recommendation. Migration convenience is not the primary decision criterion.

This is a READ-ONLY architecture review, not implementation. Do not edit repository files, build or launch applications, change OS settings, create commits/PRs, or contact Cherry/Pierre threads. All windows must stay in the background. Write only your assigned review artifact. Do not delegate to another model: the user requested your model's independent judgment. Do not inspect other reviewers' reports before completing your own.

The parent will synthesize recommendations and disagreements. Be explicit about your preferred design, alternatives rejected, facts verified in source, inferences, and claims requiring a prototype. Do not just agree with the latest suggestion.

## The discussion to assess

The current released fork has a native AppKit loop on the macOS main thread and React/application code in a Bun worker, all within one process. It preserves the existing GPUiX mutation renderer. There are two Rust RetainedTree instances: a worker-side validation model in NativeClient and a UI render model. Worker prepareBatch applies/parses the batch to validate synchronously and return destroyed IDs. UI applies/parses again. Synchronous worker automation combines its model with saved native bounds. Other sync getters use native snapshots.

The user questioned whether the duplicate Rust tree is needed, suggesting a JS worker that queues commits for one UI-thread tree. Discussion established that this is feasible but changes validation, callback cleanup, query, and error semantics. UI mutation/layout can still delay input. The worker also currently calls native font, measurement, and syntax services, so "no duplicate model" is distinct from "no native code executes on that thread."

We then compared modern React Native Fabric:
- JS can call C++ through JSI on the JS thread.
- Immutable C++ shadow-tree revisions share unchanged nodes; this is not just two complete mutable validation copies.
- Layout/preparation can execute outside the UI thread; host views mount on the UI thread.
- It supports synchronous layout/effect integration and selected synchronous scheduling paths. It is not exclusively an asynchronous serialized bridge.
- Revision diffing can skip intermediate presentations, while imperative operations and events need defined semantics.
- Native scroll state can update independently of React.

The user is now explicitly willing to rethink/rewrite bindings. They suspect a genuine worker/native shadow model may trade memory for better semantics, consistency, and interaction/render performance. Assess that hypothesis rigorously. Do not assume "more trees" or "fewer trees" is the objective, or that Fabric can simply be copied.

## Current code, available locally

GPUiX checkout: /Users/erwin/Code/gpuix
GPUiX release commit: 08ffd4bfd70c638be6a8025548ba4234a5ff1edd
GPUI is checked out at /Users/erwin/Code/gpuix/zed
GPUI release commit: bea32f070b9fe5286081f1ea8730ad3eca890ba2
Upstream GPUiX baseline: 7ac9880abd8e91e5bf0e4feb0fa850729cf95a68 (0.9.0).
Read README.md first; treat source as authoritative when older AGENTS architecture descriptions conflict.

Required source areas:
- packages/react/src/reconciler/host-config.ts
- packages/react/src/reconciler/batch-renderer.ts
- packages/react/src/reconciler/event-registry.ts
- packages/react/src/application.ts
- packages/native/src/renderer/host_runtime.rs
- packages/native/src/retained_tree.rs
- packages/native/src/renderer.rs: GpuixView::render, build_element, VirtualListEntry, apply_batch_to_tree
- packages/native/src/extension.rs and custom_elements/mod.rs
- zed/crates/gpui/src/element.rs
- zed/crates/gpui/src/window.rs and the layout/text/list/entity code needed to test your architectural assumptions.

Important observations to verify:
- Upstream GPUiX ALREADY has mutation batches, a retained tree, native motion, and virtual lists. A full-tree-per-frame JSON narrative is stale.
- GPUiX builds ephemeral GPUI elements from retained host data. GPUI preserves native entities and stable-ID interaction state.
- Current GPUI Element::request_layout/prepaint/paint take mutable Window and App. Arbitrary element construction/layout is not already a pure Send + Sync worker operation.
- Native virtual-list callbacks can build rows after root render through cx.processor. Native virtualization does not alone bound React/retained data.
- The same native frame includes stateful controls, IME, scrolling, focus, selection, text, overlays, and consumer GPU effects.
- The host currently yields after 32 transactions or 4 ms between transactions; one transaction can exceed that. Mutation batches are validated atomically before application. Command transactions do not yield between scene and synchronous layout-effect commands, but do not imply database rollback.
- Worker/native input limits are 256 and 4 MiB; native output is 4096 records/4 MiB. Overflow/failure is explicit.
- Initial native window stays hidden until first scene and layout-effect commands are applied and painted.

Consumer ownership:
- Cherry-specific components and effect shaders stay in Cherry, not core.
- GPUiX extension crates statically compose with one matching core/GPUI build. Loader selects one binding per JS context before React imports; host and worker select the same library. Exact composed-build fingerprint is not yet implemented.
- Generic GPU extension uses shared device/queue RGBA textures, premultiplied alpha, float RGB above alpha, masks/corners/opacity/scene order. Producer owns animation/reduced motion/cancellation.
- Core standard text, code, diff, markdown, input etc. share text inspection/selection/bounds services. Custom consumers must remain first-class.
- macOS/Bun is the validated native host; browser WASM has WebGPU/WebGL paths. Do not silently assume all native platforms, browser worker facilities, text services, or native views have equal threading rules.
- Preserve the principle that a blocked application runtime cannot stop committed native input/scroll/animation. This is a design requirement, not a guarantee that arbitrary native work never stalls.

Optional consumer source, read-only:
Cherry: /Users/erwin/.bb-machines/erwin.getbb.app/plugins/environment-git-worktree/host-data/worktrees/thr_qnpbykrmnk-1/cherry
It still uses prior patched GPUiX 0.9.0 at the inspected snapshot. Untracked extracted packages/ui, native-components, native-effects, native-runtime exist; manifests still pin c5544fb and sweep still uses the removed IridescentSweep API. Do not claim migration complete.
Representative workloads: long rich chat, windowed records with synchronized header/body/frozen columns, editor/IME/selection, hover/press/drag controls, native animation, GPU effects.

## Questions your review must resolve

1. Recommend a complete ownership/threading model. What lives in React, native worker-side data, UI-thread GPUI state, and GPU resources? Distinguish descriptions/revisions from interactive instances.
2. Compare at least:
   A. One mutable UI-owned retained tree with asynchronous mutation commits.
   B. Immutable/versioned native shadow model with structural sharing and prepared commits.
   C. A different/hybrid design you consider stronger.
   Explain why your choice wins under these requirements, not which is easiest to retrofit.
3. What exactly can move off the UI thread in GPUI? What stays UI-bound? Would you change GPUI, introduce a pure layout contract, keep layout native/UI-bound, use retained view islands, or choose another route? Avoid casually running arbitrary Window/Context-dependent code on a worker.
4. Define render/commit/layout/mount/presentation semantics; useLayoutEffect and measurement behavior; snapshot freshness; event/callback timing; controlled inputs, composition, focus and scroll intent. Explain any React semantic gaps explicitly.
5. Explain consistency when React prepares revision N+1 while native input, scroll, resize, font changes, or editor state advances. Describe version/epoch/rebase or ownership rules and stale-result rejection. Include an end-to-end trace.
6. Define scheduling, coalescing, cancellation, backpressure, command/event ordering, error recovery, and overload behavior. Do not drop semantic operations just because intermediate visuals can be skipped. Do not claim lock-free or zero-copy without an ownership/lifetime argument.
7. Discuss transport/runtime choices: N-API, JSI-like direct native handles, typed/binary mutation data, supported JS engines, code generation. Distinguish serialization from thread scheduling.
8. Address memory scaling, structural sharing granularity, caches, native/React virtualization, text shaping/layout, frame rebuild/diff costs, mount and reclamation costs, and instrumentation. State what you cannot infer without benchmarks.
9. Design the custom-native-component API so third-party Rust views/editors/effects can use GPUI capabilities without importing app-specific dependencies or breaking the consistency model.
10. Give the strongest objection to your own proposal, the conditions favoring an alternative, and a small set of falsifiable prototypes/benchmarks to select the architecture.

## React Native primary references

Verify relevant claims against primary sources and distinguish Fabric/New Architecture from the legacy bridge. Start with:
- https://reactnative.dev/architecture/landing-page
- https://reactnative.dev/architecture/render-pipeline
- https://reactnative.dev/architecture/threading-model
- https://reactnative.dev/architecture/fabric-renderer

Do not turn this into a general React/Rust tutorial. Use concrete data structures, API sketches, diagrams, and workload traces. Avoid treating the parent's earlier preference for either simple queued commits or shadow revisions as settled.

## Deliverable

Write a clear report of roughly 1,800-3,000 words, plus concise code/diagrams if needed:
- Recommendation in the first paragraph.
- Alternatives/comparison.
- Ownership and data model.
- Commit/interaction trace with consistency and scheduling rules.
- GPUI feasibility and required changes, with source references.
- Public binding/extension contract.
- Risks and decisive prototype tests.
- Conclude with your model identity, source inspected, and open uncertainties.

Write to the unique absolute report path supplied in your individual prompt. Do not modify the common brief. Your final response should give a short recommendation, artifact path, and the most important reason or caveat. Parent-child reporting should deliver it; no messages to unrelated threads.
