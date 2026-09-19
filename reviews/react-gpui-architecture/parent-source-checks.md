# Parent source checks for synthesis

Read-only observations, recorded before the independent reports:

- gpui/src/element.rs:73: request_layout receives mutable Window/App. Prepaint and paint do too.
- gpui/src/taffy.rs:24: NodeContext measurement callbacks are FnMut(..., &mut Window, &mut App). Current TaffyLayoutEngine owns those callbacks; pure Taffy mathematics does not make this whole adapter worker-safe.
- gpui/src/window.rs:4899: request_measured_layout captures a callback with mutable Window/App; compute_layout is expected during prepaint.
- gpui/src/platform.rs:1121: PlatformTextSystem is Send + Sync.
- gpui/src/text_system.rs:51: TextSystem already shares platform service and caches through Arc/locks.
- gpui/src/text_system.rs:380: WindowTextSystem wraps an Arc<TextSystem> and a separate line-layout cache. Current NativeClient already constructs a temporary WindowTextSystem for worker measurements. Thus some shaping/measurement is already separable, but contention, font epochs, cache lifetime, and platform behavior must be assessed.
- gpuix/native/src/extension.rs: current native factories run on UI thread; their render context exposes GPUI and Window/NativeView services. An arbitrary current extension cannot simply become a pure background layout participant.
- gpuix/native/src/renderer/host_runtime.rs:405,529: the current worker staging tree is mutable and re-applies the same mutations. It is not a structurally shared immutable render revision system.
- gpuix/native/src/renderer.rs:4390+: GpuixView synchronizes focus, prunes custom/scroll/list/motion state, builds elements, and emits some events as part of render. A rewrite must separate revision preparation from these live state effects.
- Native publication cost includes lifecycle, focus/hit testing/accessibility/selection consistency and reclamation; an Arc root swap alone does not bound that work.

Questions to use in review, not preselected answers:
- Can a worker-side measurement return a coherent candidate layout without promising that the UI is already displaying it?
- If fresh synchronous native measurement is proposed, who can wait on whom, how are pending commits flushed before the query, and what prevents an intermediate presentation while JS completes a layout effect?
- What exact state/version key invalidates prepared work after resize, font changes, or native editor updates?
- Can a continuous stream of environment changes starve publication? What is the fallback?
- How are event handler versions retained while an older visual revision remains interactive?
- Which commands survive visual coalescing, and how are creation/removal side effects handled for revisions never mounted?


## Checks after the first three reports

- GPUI Entity identity is not subtree caching. view.rs:224 documents Entity::cached(style) with definite outer dimensions. ViewElement::new defaults to cached_style None. Cache keys include bounds, content mask, and text style, plus dirty-view and refreshing checks. Cached paths replay prepaint/paint ranges, not cloned AnyElement objects.
- AnyElement is arena-owned frame data. A proposed persistent Option<AnyElement> and element.clone() cache is not a usable implementation.
- window.rs:2920 Window::draw mutates input handlers, swaps rendered_frame, dispatches focus changes, and sets needs_present. It is not a pure measurement pass. present() is private; present_if_needed is build-configuration gated. An async measure call cannot defer normal React useLayoutEffect until its Promise resolves.
- Window::reuse_prepaint/reuse_paint replay GPUI frame records. GPUiX selection_frame_reset and bounds_frame_reset clear external thread-local registries each paint. A cached scene can skip the callbacks which repopulate those registries. Correct cache integration must replay metadata in order, including deferred draws and rollback semantics, or avoid caching those subtrees. A general GPUI metadata attachment API is one proposed solution, not established as the only possible implementation.
- Installed React persistence code enumerates children in appendAllChildren and appendAllChildrenToContainer. Structural sharing does not imply an O(depth) total update when host parents have wide child sequences.
- AGENTS.md:641 records a historical 850ms mount, including626ms applyBatch Rust parsing and26ms stringify. It then describes the raw-prop fix. Do not call626ms current serialization cost or use it as the present worker/UI profile.
- React Native documentation supports immutable revisions, structural sharing, native state updates, and skipping intermediate presentations. Its threading model also includes synchronous UI paths. It does not establish that GPUI can provide equivalent layout effects with a root-pointer swap.

## Follow-ups sent

Gemini: correct arena caching, definite-size cache limits, metadata replay, layout barrier semantics, unsupported memory/timing guarantees, safe publication/reclamation, command barriers, mutable-tree coherence, and current-GPUI versus fundamental constraints.
DeepSeek: correct async measurement/effect claim, latest-root plus FIFO mismatch, input echo versions, memory guarantees, strong typed-delta alternative, semantic search revisions, runtime/platform claims.
Astra: compare against strong typed-delta A, test whether general frame attachments are necessary versus one solution, and analyze a pure layout/adoption contract given permission for substantial redesign.
Fable: original independent review still active at last status check. No findings from peers sent to it.


## Fourth report and assumption checks

- Fable independently recommends the same shared-description/UI-instance split. Its extra emphasis is intrinsic-size cache support and typed transport. Follow-up requested stronger A comparison, speculative Rust allocation lifetimes, revision retention, post-effect sealing, command/event policy, presentation-hold input consistency, cache metadata/constraints, and state-specific versions.
- Astra addendum accepts a strong typed-operation A, narrows immutable benefits to stable concurrent snapshots, speculative preparation and chosen-revision diffing. It corrects GPUI-owned frame attachments from necessary implementation to preferred general implementation. A restricted GPUiX-owned metadata replay cache is viable; deferred draws need order/replay/rollback integration.
- Astra describes an untested pure-layout/adoption alternative: candidate layout before effects, old native revision remains interactive, effects settle, then dependency-validated adoption. This is stronger than reading prior-frame geometry but not the same as exact live geometry. Native state changes can invalidate candidate-dependent JS corrections, and normal layout effects do not automatically rerun.
- DeepSeek revised its measurement claim and memory analysis. It selects best-effort stale-target cancellation, despite naming that policy revision barriers. It still publishes before layout-effect commands, retires callbacks immediately, and uses an unconditional replace flag for normalization. Parent synthesis will not adopt those details without a stronger contract.
- Gemini's first correction still claimed uncached Entity skips rendering and invented a shouldYield host hook. Source shows shouldYield comes from Scheduler.unstable_shouldYield, not host config. Sent a final narrow correction request. Also asked it to remove universal layout trilemma/120Hz/zero-drop-stall claims.
- GPUI mark_view_dirty at window.rs:1955 marks ancestor views. Cached parent rebuild sets refreshing for its region. Need to test actual nested-cache invalidation; changed-entities-only frame work is not a source-backed general guarantee.
- Existing serialization benchmark records 30.1ms parse-and-apply,42.6MB retained tree,592B/element for a 72,010-node fixture after optimizations. It explicitly says codec choice was the smallest lever. These are documented past fixture results, not fresh measurements or projected rewrite gains.

## Proposed synthesis direction

Agreement supports shared immutable host descriptions as a candidate, not a measured winner. Keep layout placement as a separate design axis. Recommend exact contracts for pure preparation, native instance state, revision/event/command lifetimes, and runtime-independent typed transport. Prototype strong typed A versus shared C, then a restricted pure-layout/adoption B before fixing the public measurement API. Keep native extension interoperability and independent native input as hard requirements. No code implementation is authorized by this review request.


## Final review outcome

Fable's correction pass changed its recommendation to strong typed A with no worker Rust tree. Its strongest argument is that transport, per-node preparation, versioned native state and cache improvements also apply to A. It assumes windowed workloads have small enough deltas and little need for concurrent native tree readers. These are plausible but unmeasured assumptions. Parent synthesis now reports the final 3-to-1 split, not the initial agreement.

Fable also identified broad refresh invalidation. Verified renderer.rs apply_batch calls request_invalidate; macOS invalidate_window calls window.refresh. window.rs refresh sets refreshing=true outside draw, and view.rs cached paths reject refreshing. GPUI active-state and other interaction handlers also call refresh. General cache benefit requires invalidation work in addition to entity boundaries.

All correction passes finished. Parent does not adopt remaining categorical claims in reports, including O(1) whole-mount cost, guaranteed smaller queues/trees, unconditional native panic recovery, or speculative frame budgets. Future work should treat all runtime/cache/layout proposals as hypotheses until implemented and measured.
