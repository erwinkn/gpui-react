# Test the agreed architecture against real use

Status: design requirements and source review, 2026-09-19. No new implementation or performance result is claimed here.

**The agreed architecture remains the baseline. The cases below do not establish a need for a second native tree or worker layout.** They do establish requirements for native state, transaction order, measurements, resource lifetime, and bounded work.

The earlier [review synthesis](synthesis.md) records the alternatives we considered. This document records the user's subsequent decision and the tests that should govern any departure from it.

## 1. The baseline we are testing

```text
Application worker                         Native UI thread

React reconciler                           One retained host model
  → committed host mutations                 → ordinary GPUI Render methods
  → typed operations + ordered commands      → GPUI layout, input, and paint
  → bounded transaction queue ────────────→
                              ←──────────── native events + measured snapshots
```

GPUiX defines the retained host data and its GPUI rendering. GPUI entities can hold that data. GPUI does not supply a DOM with generic insert/remove/set-prop operations; GPUiX supplies that translation. The host model must not then be duplicated in a second generic description tree.

One entity per host node is a possible implementation, not a decision. Small records under larger entities can satisfy the same design. Choose the granularity from measured costs.

The worker retains React's own data, callbacks, pending transactions, and enough host identity information for reconciliation. It does not need a full Rust tree. The UI retains props, child order, native handles, and component resources. Layout caches, list height indexes, and editor buffers have distinct jobs; they are not duplicate copies of the React host tree.

**A worker and a second native tree are separate decisions.** Cherry already supplies evidence for keeping application JavaScript off the native event loop. That does not justify a second Rust tree.

**The asynchronous boundary is an accepted design choice.** Native events reach JavaScript without waiting for a handler. React sends resulting changes back for later native application. React cannot synchronously query live UI state. A query returns asynchronously; an already delivered snapshot is just local data. The native loop never waits for JavaScript to make a layout or input decision.

Code that needs geometry during the current native frame belongs in a native component with a React wrapper. GPUiX must make that component path easy to use. General synchronous native measurement from `useLayoutEffect` is outside this baseline, rather than an unresolved requirement.

GPUI continues to own layout. A native component may use GPUI's normal layout, prepaint, and paint hooks. This includes custom components; it does not require an external layout engine or a replacement drawing system.

Source checkpoints: GPUiX `08ffd4bfd70c638be6a8025548ba4234a5ff1edd`; GPUI `bea32f070b9fe5286081f1ea8730ad3eca890ba2`; Cherry `383aeab877363c2c417745b4fc1994c6b4b1a157`. The current host still has a worker-side Rust staging tree. Removing it is proposed work, not an existing result. [Current host contract][host-api], [current staging tree][host-source], [GPUI view implementation][gpui-view].

## 2. Scenario map

“Fits” means the baseline can express the behavior. It does not mean a new implementation has passed the test. “Contract” means a rule must be specified and tested. “Limit” means the baseline does not promise the requested behavior.

| Case | Observable requirement | How the baseline handles it | Assessment |
| --- | --- | --- | --- |
| S01. JavaScript is busy | Scrolling, native typing, hover, caret, and committed animation continue. | Native loop owns interaction. Application worker sends updates when it can. | Fits; retain the tested host separation. |
| S02. Interrupted React work | An abandoned render must not create visible nodes, start native resources, or remove working callbacks. | Publish native operations only for accepted commits. Preserve stable host identity on updates and moves. | Fits; lifecycle tests required. |
| S03. Related updates | New rows and their requested scroll anchor must first appear together. | Apply an ordered transaction before drawing; resolve list commands after row synchronization. | Contract; already required by Cherry. |
| S04. Refs and command order | Create → focus → remove must not focus a later replacement or leave dead focus. | Refs address a root/session and stable host ID. Apply commands in order; retire IDs and resources explicitly. | Contract; fresh measurements are separate. |
| S05. Delayed input updates | A delayed React echo must not erase newer native text, selection, or IME composition. | Native editor owns live state. Version acknowledgements and distinguish them from application replacements. | Contract; current input tests do not prove this race safe. |
| S06. Native event policy | Editor shortcuts, menu arrows, wheel consumption, and capture work while JS is busy. | Bind immediate behavior in native state. Send semantic notifications to React afterward. | Fits; arbitrary synchronous JS cancellation is a limit. |
| S07. Linked and nested scrolling | Header/body/footer agree within each frame; nested scrollers and observers receive the right wheel behavior. | Shared native handles, axis constraints, default-action consumption, and normal propagation. | Fits; several real regressions supply tests. |
| S08. Large lists | Large logical counts do not require equally large React or GPUI element trees. Distant jumps request missing rows. | Keep GPUI ListState and a bounded React row window. Separate logical count, supplied rows, and measured heights. | Fits; content absent from native memory still needs the worker. |
| S09. History and streaming | New content preserves the reader's anchor; Latest follows the end; stale page requests cannot replace a newer selection. | Native logical anchors plus application request generations. Commit row window and anchor together. | Fits; data preparation remains an application concern. |
| S10. Geometry-dependent components | A popup matches its trigger; connectors meet resized cards; selection controls follow selected text in the same frame. | Express constraints and geometry dependencies through native GPUI components. | Fits; specific bindings may be needed. |
| S11. General fresh layout reads | A component needs current-frame geometry to choose its output. | Put that logic in a native component. React gets measurements asynchronously when it needs observations. | Accepted boundary; no synchronous live UI query from React. |
| S12. Delayed events | A queued click targets the correct node and callback after props change or a node unmounts. | Tag event provenance; define callback retention and retirement. Never reuse an ID within a live session. | Contract; one tree alone does not define it. |
| S13. Burst commits and overload | Queues stay bounded; accepted changes are not silently lost; native input gets time to run. | Typed batches, bounded admission, ordered application, and explicit failure. Budget between transactions. | Contract and performance test; one large transaction can still stall UI. |
| S14. Local changes in large content | Typing or one animation does not rebuild every loaded message. | Bound mounted content, prepare expensive data outside frames, and measure GPUI entity/render costs. | Performance question; entity boundaries alone do not prove reuse. |
| S15. Text, selection, and search | Copy, highlights, selection anchors, and inspection agree on the content actually painted. | Shared text services and stable identities; maintain distinct content/query dependencies and paint-order metadata. | Fits; cache reuse must preserve this metadata. |
| S16. Motion and presence | Retarget, cancel, resize, reduced motion, and removal have defined behavior. No JS frame clock is required. | Native state owns active motion; explicit presence can retain an exiting visual while disabling its interaction. | Fits; full motion/presence system remains proposed work. |
| S17. Native extensions and GPU resources | Editor, video, charts, and effects share the runtime and obey normal clipping, input, opacity, and teardown. | External component crates use the common GPUI build and general extension API. | Fits; component state is not a second host tree. |
| S18. Failure, restart, and packaging | Worker failure closes cleanly; late messages cannot affect a new session; moved bundles load one compatible runtime. | Session generations, bounded shutdown, explicit composition, and source/compiled/relocated tests. | Fits; native crashes remain process crashes. |

## 3. Cases that need precise contracts

### S03–S04: a commit includes dependent commands

Cherry had a concrete failure: a new row window could paint using the old scroll anchor. The fix makes the relationship explicit.

```text
Transaction 42:
  replace the supplied row window with rows 49980…50029
  update the logical item count
  request scrollTo(index=50000, offset=0)

UI application:
  update retained host data
  synchronize GPUI ListState, including row changes
  apply the scroll request
  run GPUI layout and paint
```

Nothing here requires JavaScript to read the new layout. The anchor is logical; GPUI resolves pixels against native measurements. A focus request from a layout effect is similar: it names a node rather than asking JS to calculate geometry.

For `create(A) → focus(A) → remove(A)`, preserve command order. A must not remain focused after removal. The system may never present A, but focus notifications can still be observable. Do not silently drop intermediate commands merely because the final pixels would be the same.

The current transport groups mutations and synchronous layout-effect commands using a microtask. A replacement must test nested commits, effect cleanup, synchronous updates triggered by effects, and multiple roots. “One batch per React commit” is not a complete grouping rule. [Host API][host-api], [atomic anchors][cherry-native], [host first-frame evidence][cherry-host].

### S05: controlled input while the worker is delayed

```text
Native editor                       React worker
edit 10: "a"     ────────────────→   handles edit 10
edit 11: "ab"                        still processing
                 ←────────────────  acknowledges edit 10: "a"
```

The final native text must remain `"ab"`. Comparing the arriving string with the last declared prop is insufficient: a changed prop can still describe an older edit.

A proposed contract distinguishes:

- **Acknowledgement:** identifies the observed native edit. It must not overwrite a newer edit.
- **Conditional replacement:** applies a new application value only if the expected edit version still matches.
- **Explicit reset:** intentionally replaces current text under a documented composition policy.

The exact API is undecided. Normalizing input, resetting after submit, restoring a draft, undo, and IME each need tests. If a replacement waits for composition to finish, recheck its version afterward. Also version selection commands where their offsets depend on a particular buffer.

This is a state ownership problem under every proposed tree design. The current adapter compares `last_prop_value`; its ordinary edit and external-value tests do not establish the delayed-echo guarantee. [Input adapter][input-source], [input tests][input-tests].

### S10–S11: three different uses of layout

| Request | Baseline behavior |
| --- | --- |
| “Make this menu as wide as its composer.” | Native layout relationship. No JS round trip. Cherry's `matchWidth` binding does this. |
| “Tell the inspector the latest completed bounds.” | Deliver an asynchronous result labeled with frame/commit and relevant viewport information. |
| “Measure this new layout synchronously in JS, rerender from it, and never show the first result.” | Outside the accepted React API. Move the dependent logic into a native component. |

Cherry's workflow canvas already shows the first approach for a harder case: nodes resolve geometry in native prepaint, then connectors read those anchors during paint. Selection toolbars use native selection geometry. These are normal GPUI component techniques, not worker layout or a replacement renderer. [Geometry and component history][cherry-native].

The new binding's frame tests now cover ordinary GPUI deferred elements,
nested deferred hosts, and frame metadata across later draws. A general GPUI
element context carries each host's metadata through deferred prepaint and
paint. It adds no layout box and does not run a second layout engine. Cache
replay does not repeat measurement callbacks. This validates the metadata
boundary. Deferred document text now keeps its selection/search scope and
finalizes its registry at native draw completion. The offscreen GPU test covers
floating text outside parent bounds, nested documents, native selection,
clipboard, and removal. The selection-toolbar example now checks native double-click selection,
current-frame placement after wrapping/font changes, button hit testing,
clipboard, changed source, and clearing. Its position comes from the same GPUI
text layout used to paint the selected text. Menu and connector cases remain
open.

React documents the browser measure/correct-before-paint pattern in [useLayoutEffect](https://react.dev/reference/react/useLayoutEffect). Keeping the hook's React execution order does not give this renderer a fresh synchronous native measurement API.

For an arbitrary JS calculation, use an asynchronous query and permit a later correction, or keep an application surface hidden until it is ready. Such staging is a component policy, not a general promise that layout effects prevent intermediate native frames. The agreed design does not use a synchronous UI wait to emulate the browser contract.

Separate **React committed**, **native applied**, **native laid out**, and **frame submitted**. An input acknowledgement or an idle transport does not imply the application response has painted. Geometry snapshots must not silently combine new topology with old bounds. The current synchronous automation API does combine those sources; that is a documented current limitation, not the target contract. [Current getter semantics][host-api], [Cherry test synchronization correction][cherry-host].

### S12–S13: asynchronous transport is part of the renderer

Suppose a visible button uses handler H1. React has produced H2, but UI has not applied that transaction. A click occurs on the H1 scene. The event policy must say which handler runs.

Recommendation to test: associate events with the native subscription generation that produced them. Retain that callback until native confirms retirement and earlier events have drained. Handle unmount explicitly; never route an old event to a new node that reused a key. This costs bounded callback metadata, not another native description tree. A latest-handler policy is also possible, but must be intentional and tested.

The implemented binding now has a direct native regression for this boundary:
[`event_frames.rs`](../../crates/gpui-react/tests/event_frames.rs). It uses native
effect order. Events queued before a subscription change keep the old callback;
later emissions use the new callback, including input on an older painted
hitbox. Native entity state can be newer than that hitbox. Unmount retires the
old route before a replacement can receive input. This is GPUI's ordinary
mutable-state behavior; the binding does not promise callbacks from the last
painted description.

Mutations have a related problem: React may advance its committed Fiber state before native accepts a transaction. On queue saturation, dropping the transaction or merely throwing leaves the two sides inconsistent. Preserve and retry the complete ordered transaction within a bounded queue, or explicitly fail the root/session. Do not claim React will automatically stop committing when a native queue fills.

Worker-side schema validation can check types and decode payloads. Topology validation also needs the live native model: parent existence, child order, cycles, valid references, and extension contracts. Validate the affected operations before mutating visible state. “Atomic” here means no frame or input dispatch sees a partial transaction; it does not promise rollback after an arbitrary Rust panic or side effect.

A UI time budget only helps between atomic transactions. Huge mounts need bounded content, smaller independent commits where semantics permit them, or measured preparation work off-thread. Parsing off-thread does not remove UI allocation, attachment, destruction, or layout costs. [Current limits][host-api], [existing serialization measurements][serialization].

## 4. Cherry regression ledger

These cases came from Cherry's recorded implementation and test history. The application began with published GPUiX 0.9.0 and then accumulated patches. Later defects must not all be attributed to the published release.

| Actual issue | Where it arose | Architectural lesson and regression to retain |
| --- | --- | --- |
| Fixed column and header lagged during scroll. | The original Cherry app used a JS timer to correct native offsets; 0.9.0 lacked shared-scroll bindings. | Share native scroll state. Cherry sampled 72 frames with JS callbacks paused and reported zero alignment error after the fix. Test moving frames, not just final positions. [Scroll report][cherry-scroll] |
| Excess draws and long native event-pump pauses. | GPUI test-support behavior affected production draws; the embedded pump serviced the run loop twice and drained too much work. | Preserve production scheduling and native loop ownership. A shadow tree would not repair either cause. [Scroll report][cherry-scroll] |
| Native animation/input stalled with blocked application JS. | Embedded macOS launcher; earlier pacing repairs did not provide execution independence. | Keep the application worker. Historical blocked-worker probes demonstrated native progress; background CPU draws are not physical display FPS. [Host evidence][cherry-host] |
| Distant list jumps did not request missing rows. | Baseline binding lacked a range request for a programmatic anchor outside supplied rows. | Model logical count separately from supplied content. Retain the 5,000-row jump and current virtual-list tests. [Scroll report][cherry-scroll], [list tests][list-tests] |
| New content appeared at the previous anchor, or a mount-time scroll request was lost. | Large-history integration exposed ordering and first-frame binding gaps. | Row updates and scroll intent must apply before the same native layout. Keep first-frame row-99 and atomic-window tests. [History report][cherry-performance], [host evidence][cherry-host] |
| Registering row focus damaged height estimates; large scrolls landed on the wrong row. | Later filled 100,000-row component work used a row-splice operation for focus updates. | Update focus ownership without replacing list geometry. Retain the exact 50,000-row delta and height/anchor invariants. [Native patch history][cherry-native] |
| Nested scroll moved both containers; a later fix suppressed callbacks. | GPUI scroll consumption defect, followed by a fork regression using propagation stop. | Consume the default scroll action only when movement occurs; preserve same-element/ancestor observers and boundary chaining. Core commit `792c01b` repairs the callback regression. [Native history][cherry-native], [current event tests][event-tests] |
| Resize gestures were unreliable with a temporary capture overlay. | Original Cherry component strategy. GPUiX already supported capture armed on the pressed element. | Bind down/move/up to the resize handle. Capture and geometry must stay coherent during the drag. [Upstream-use history][cherry-upstream] |
| Menu width corrected late; bounds omitted padding/border or referred to the wrong box. | Missing same-layout width binding and bounds-tracker defects. | Bind native constraints; record the actual painted outer box. Retain narrow-window, wrapped-toolbar, border/padding, and resize cases. [Native history][cherry-native] |
| Hidden pages kept unwanted input regions; nested error overlays could paint behind a modal. | Component experiment and later overlay-priority binding. | Separate layout participation from paint/input participation. Preserve deferred layer ordering. The attempted broad GPUI hidden-prepaint change was reverted after a hover regression. [Native history][cherry-native] |
| Composer fade had a dark band. | GPUI alpha interpolation defect exposed by a transparent gradient. | Fix general renderer math and retain pixel tests. Tree topology is unrelated. [Native history][cherry-native] |
| Live text inspection returned empty data; repeated screenshots could stall about one second. | Wrong-thread reads of paint logs; separate Metal capture drawable-pool defect. | Query UI-owned inspection state on UI; capture offscreen without consuming unpresented display drawables. These were inspection/capture defects, not proof of ordinary-use frame stalls. [Native history][cherry-native], [capture evidence][cherry-performance] |
| Glimm highlights were lost, and its payload enlarged generic GPUI paint structures. | Later Cherry effect prototype and blending path. | Keep the effect in Cherry; use general premultiplied RGBA texture composition, including float RGB above alpha. Core now has a public texture fixture; Cherry's new adapter integration is not claimed complete. [Texture contract][textures] |
| New pages briefly showed empty/loading content; late requests could replace a newer navigation. | Application data and publication policy. | Prepare required pages, cancel obsolete results, and publish rows plus anchor together. The renderer cannot supply data that has not arrived. Injected 400–500 ms page delays test this. [History report][cherry-performance] |
| Input delivery was mistaken for completed application work; a pump test discarded fractional coordinates. | Test synchronization and test filtering defects. | Wait for the relevant committed/painted condition. Preserve ordered-event assertions without relying on integer coordinates. [Host corrections][cherry-host] |

Related upstream regression tests also cover abandoned Suspense renders, removed text-node leaks, cross-root identity, image state, structural text grouping, and list prepends at the transition from short to overflowing content. They are useful requirements, but the inspected evidence does not establish that all first occurred in Cherry. [Mutation tests][mutation-tests], [identity tests][identity-tests], [list tests][list-tests].

## 5. Performance and extension requirements

**Keep all three limits visible:** how much application data is loaded, how many React host nodes exist, and how many GPUI elements a frame builds. Cherry's large-history design bounds these separately. Native virtualization alone does not make a huge React tree cheap. React memoization alone does not make a native frame cheap. [Large-data design][cherry-performance].

Do not assume an unchanged GPUI entity skips element construction. Current uncached views still call `render`; cache reuse has additional conditions. GPUiX also records text, bounds, and selection during paint. Replaying cached graphics without those records can make correct pixels unselectable or invisible to automation. First measure the ordinary GPUI path. Propose cache integration only for a demonstrated cost, with metadata and invalidation tests. [GPUI view source][gpui-view], [text/paint rules][project-rules].

Native animation has two costs: updating its value and building/layout/painting the affected UI. Moving the clock to Rust solves only the first. An exit animation can retain a detached visual subtree temporarily, with a generation and explicit lifetime; this does not require mirroring the entire mounted tree. Full presence, measured collapse, and general motion graphs remain proposals in Cherry's [animation design][cherry-motion]. Test interruption, re-entry, focus transfer, reduced motion, idle frame requests, and cancellation.

Keep Cherry components and the Pierre editor outside core. They can own domain data, parser caches, undo history, textures, or decoder state. Use the shared extension API for styles, events, text, accessibility, and teardown. A native editor's document model is justified by editing; it is not a duplicate React host description.

The native-component interface is part of the primary design. An extension author should define typed props, native state and GPUI rendering, typed events, and optional commands or asynchronous queries. GPUiX should supply identity, commit ordering, event transport, teardown, and registration. The author should not need a new event queue, a private reconciler hook, or changes to core for each component. A React wrapper should feel like an ordinary component. For example, a `ComposerMenu` wrapper can supply items and a selection callback while its native implementation resolves width, placement, and keyboard feedback in the current frame. This is a proposed example, not an existing public type. A minimal external-component fixture should test this authoring contract as well as runtime correctness.

The current texture contract uses the window's device and queue, normal scene ordering, inherited opacity, clipping and corners, and retained frame resources. Float textures preserve RGB values above alpha before composition. Test resize/removal while prior frames retain textures. This is one justified general GPUI extension already backed by a [composition fixture][composition], not a reason to carry application shaders in core. [Texture API][textures].

Runtime selection must happen before consumers load the default binding. Host and worker must use the same compiled composition and extension catalog. Source success is insufficient: test installed packages, browser exports, compiled worker entries, and relocation. Platform capability gaps must be explicit; current native-host evidence is macOS/Bun evidence. [Extension contract][extensions], [Cherry ownership boundaries][cherry-boundaries].

## 6. Tests to run before replacing the current implementation

The first prototype should exercise the chosen baseline, not implement three complete architectures. Compare it with both the current fork and a small direct-GPUI fixture for the same native workload.

| Priority | Fixture | Required result |
| --- | --- | --- |
| 1 | Worker blocks while native input and animation run. | Native input, scrolling, editor text, and committed animation progress. Test queue saturation separately from a finite stall. |
| 1 | Create/update/move/remove, abandoned render, effect commands, and nested commits. | No speculative native mount; no partial frame; stable identity; ordered focus/scroll; complete cleanup. |
| 1 | Two native edits followed by a delayed echo; external reset during IME. | No lost newer edit, broken composition, selection corruption, or accidental undo reset. |
| 1 | New row window plus anchor; delayed/reordered data responses. | Correct first frame, no old-anchor flash, no stale page publication. |
| 1 | Linked table panes, nested div/list scrolling, resize drag. | Per-frame alignment, correct axes, callbacks retained, boundary chaining and capture intact. |
| 1 | Backlog, oversized/invalid transaction, late event, close/reopen. | Bounded memory; no silent transaction loss; explicit failure; no stale-session action. |
| 2 | 100,000 logical rows and 50,000 messages with bounded host windows. | Correct distant jumps, focus heights, selection, search, follow-tail, resize, and full export independent of the UI window. |
| 2 | A small update beside expensive rich content. | Measure native apply/build/layout/paint, bytes, allocations, retained memory, and destruction. Explain any excess over direct GPUI. |
| 2 | Anchored menu, workflow connectors, selection toolbar, nested modal. | Geometry, hit testing, focus, clipping, and accessibility agree in each completed frame. |
| 2 | External editor/effect plus compiled relocated runtime. | One runtime composition; correct resource lifetime and GPU pixels; source/package behavior agrees. |

Use deterministic race tests for correctness and separate release-build traces for performance. Report frame-time distributions and worst stalls, not just averages. Distinguish CPU draw, GPU submission, and presentation; background tests cannot certify physical display smoothness. Keep windows in the background.

Existing tests supply regression cases. They do not validate a new single-model implementation until they run against it.

## 7. What would justify a departure?

Require a failing case, a measured cause, and evidence that a smaller change cannot meet it:

1. **UI mutation application dominates despite bounded content and typed preparation.** Then test a different publication or representation strategy. Compare its mount/diff/destruction costs too.
2. **A real concurrent native reader requires frequent coherent access to the whole host model.** Then compare targeted immutable snapshots with shared immutable descriptions. Do not retain a second tree merely to service occasional queries.
3. **A future product requirement rejects the accepted native-component boundary and requires arbitrary fresh JS layout feedback before presentation.** This would require a deliberate change to the API contract. It is not a current requirement or a reason to build a second tree now.
4. **GPUI frame work remains too broad.** First test native subtree reuse or a focused GPUI correction. Shared descriptions alone do not make GPUI skip layout or paint.

The immediate decision is therefore to keep the simple translation architecture and make these tests its acceptance contract. The strongest unresolved issues are input versioning, transaction admission/order, event lifetime, and the exact asynchronous query guarantees. The no-synchronous-live-query boundary is decided.

[host-api]: ../../README.md#native-owned-application-loop-on-macos
[host-source]: ../../packages/native/src/renderer/host_runtime.rs
[gpui-view]: ../../zed/crates/gpui/src/view.rs
[input-source]: ../../packages/native/src/custom_elements/input.rs
[input-tests]: ../../packages/react/src/__tests__/input.test.tsx
[list-tests]: ../../packages/react/src/__tests__/virtual-list.test.tsx
[event-tests]: ../../packages/react/src/__tests__/events.test.tsx
[mutation-tests]: ../../packages/react/src/__tests__/mutation-lifecycle.test.tsx
[identity-tests]: ../../packages/react/src/__tests__/element-identity.test.tsx
[serialization]: ../../docs/serialization-benchmark.md
[project-rules]: ../../AGENTS.md
[textures]: ../../docs/gpu-textures.md
[composition]: ../../fixtures/native-composition/README.md
[extensions]: ../../README.md#native-extension-crates
[cherry-host]: /Users/erwin/.bb-machines/erwin.getbb.app/plugins/environment-git-worktree/host-data/worktrees/thr_qnpbykrmnk-1/cherry/apps/gpuix/docs/host-runtime.md
[cherry-native]: /Users/erwin/.bb-machines/erwin.getbb.app/plugins/environment-git-worktree/host-data/worktrees/thr_qnpbykrmnk-1/cherry/apps/gpuix/native/README.md
[cherry-scroll]: /Users/erwin/.bb-machines/erwin.getbb.app/plugins/environment-git-worktree/host-data/worktrees/thr_qnpbykrmnk-1/cherry/apps/gpuix/docs/scroll-performance.md
[cherry-upstream]: /Users/erwin/.bb-machines/erwin.getbb.app/plugins/environment-git-worktree/host-data/worktrees/thr_qnpbykrmnk-1/cherry/apps/gpuix/docs/upstream.md
[cherry-performance]: /Users/erwin/.bb-machines/erwin.getbb.app/plugins/environment-git-worktree/host-data/worktrees/thr_qnpbykrmnk-1/cherry/apps/gpuix/docs/performance.md
[cherry-motion]: /Users/erwin/.bb-machines/erwin.getbb.app/plugins/environment-git-worktree/host-data/worktrees/thr_qnpbykrmnk-1/cherry/apps/gpuix/docs/native-animation-design.md
[cherry-boundaries]: /Users/erwin/.bb-machines/erwin.getbb.app/plugins/environment-git-worktree/host-data/worktrees/thr_qnpbykrmnk-1/cherry/apps/gpuix/docs/native-package-boundaries.md
