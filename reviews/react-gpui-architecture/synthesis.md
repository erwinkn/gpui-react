# React on GPUI: architecture review

> Historical review. The user subsequently selected the simple translation architecture: one retained native host model, native interaction and layout in GPUI, and no second worker-side Rust tree by default. The [scenario matrix](scenario-matrix.md) records that decision, concrete Cherry regressions, and the evidence required before departing from it. The recommendation below is preserved as review history.

Three reviewers recommend shared immutable native descriptions. Fable's final review recommends one mutable description tree on the UI thread, fed by ordered typed transactions, with no worker Rust tree. All four keep native interaction state on the UI thread and reject keeping today's two mutable Rust trees as the intended design.

I favor shared revisions as the leading long-term design to prototype, because stable native descriptions support concurrent preparation and selected-revision publication. Fable supplies a credible simpler alternative. The reviews do not establish which is fastest or whether shared revisions repay their allocation and lifetime costs. They also do not settle where layout should run. We should test these choices before fixing the public measurement API.

This is a source review, not an implementation or performance result. The inspected GPUiX commit is 08ffd4bfd70c638be6a8025548ba4234a5ff1edd. The GPUI commit is bea32f070b9fe5286081f1ea8730ad3eca890ba2. No framework or consumer source changed during this review.

## What each reviewer adds

| Reviewer | Main recommendation | Most useful distinction |
| --- | --- | --- |
| Fable 5.1, Extra High | One mutable UI tree, ordered typed transactions, no worker Rust tree | Typed transport, versioned native state, and caching do not require immutable revisions. Their extra machinery may not pay for windowed workloads. |
| DeepSeek V4.1 Flash | Shared immutable revisions with a UI instance registry | Compare mutation application against revision preparation and mount costs. Do not treat JSON removal as proof that immutable trees win. |
| GPT-6 Astra, Extra High | Shared immutable descriptions, native components, UI layout by default | Defines native-state versions, command barriers, event lifetimes, cache metadata, and the limits of synchronous layout effects. Its addendum examines a pure-layout alternative. |
| Gemini 3.8 Flash | Shared immutable descriptions, off-thread preparation, UI interaction state | Separates replaceable visual revisions from ordered commands. Several concrete cache and scheduling claims needed correction. |

I checked the central claims against source and requested corrections. Fable changed its recommendation after comparing shared revisions with a stronger typed-transaction design. The other three kept shared revisions, with narrower claims. Some original memory, cache, and measurement guarantees were unsupported. The comparison below excludes them. None of these reviews is a benchmark.

## The proposed system

~~~text
Application worker                          UI thread

React Fibers and callbacks                  GPUI App and Window
        |                                   Native input and IME
Typed native create/clone calls             Scroll, focus, selection, motion
        |                                             |
Immutable description revisions ---------> Instance registry
        |              shared data                    |
Pure preparation                            Layout, hit testing, scene
        |                                             |
Prepared documents/resources                GPU submission

           <----- versioned events and frame snapshots
~~~

There are still several representations, with different jobs. React retains its Fibers. Rust retains description revisions. GPUI retains live entities, element state, layout data, and scene data. The improvement is that the worker and UI can share description nodes instead of maintaining two independently mutated copies.

For example, changing a button label creates a new text description and changed ancestor descriptions. An unchanged sidebar can remain shared. The live editor, its undo history, and the list's scroll anchor remain in the UI instance registry. They are not cloned with the description tree.

This is related to Fabric's immutable shadow revisions and separate mounted host views. Fabric also supports native-originated state updates and diffs between selected revisions. Its scheduling and layout integration are additional mechanisms, not consequences of immutability alone. [Fabric render pipeline](https://reactnative.dev/architecture/render-pipeline), [threading model](https://reactnative.dev/architecture/threading-model).

The native description API must be safe during speculative React rendering. Creating a description must not open an editor, subscribe to platform input, or start an animation. Abandoned React work can allocate Rust descriptions; it must not mount native instances. React's persistence mode is a suitable candidate, but its sibling traversal and FFI costs need measurement.

## Three designs worth comparing

| Design | What crosses to UI | Main benefit | Main cost |
| --- | --- | --- | --- |
| A. One mutable description tree | Typed, validated operation batches | Simple update model; no persistent description history | UI applies ordered topology changes; concurrent readers need snapshots or other coordination |
| C. Shared immutable descriptions, UI layout | Complete revision plus prepared data | Stable concurrent reads, speculative preparation, selected-revision diffing | Allocation, retained revisions, mount diffing; layout remains UI work |
| B. Shared descriptions with pure layout and adoption | Revision plus validated layout results | Can move more work off UI and expose candidate geometry to React | Requires a new GPUI layout contract and rules for native state, custom components, and stale results |

A deserves a fair comparison. It can also use typed transport, worker-side prop validation, native interaction state, versioned commands, and GPUI caches. It does not require JSON parsing on UI. Topology validation still needs an authoritative tree or sufficient topology information; decoding a typed buffer does not prove parent references and cycles are valid.

C has an intrinsic advantage when several tasks need stable descriptions while React prepares another revision. Complete revisions also make visual coalescing direct. A can obtain similar behavior with snapshots or journals, at a different cost.

Fable argues that a windowed chat or table produces small enough deltas, and that per-node preparation needs props rather than a whole native tree. On those assumptions, A can deliver the useful gains with fewer lifetime rules. I agree that this is plausible. Its claims about small deltas and the lack of tree-wide preparation needs remain workload assumptions. Conversely, preferring C does not prove that these workloads need concurrent tree readers or frequent revision coalescing.

I favor C as a framework design candidate, with A as the required comparison. I would not reject B merely because current GPUI lacks it. The user has explicitly allowed general GPUI changes. B should receive an early, restricted prototype before we accept C's measurement limits as permanent.

## Layout is the decisive open question

Current GPUI elements use mutable Window and App in layout request, prepaint, and paint. Its Taffy adapter stores measurement callbacks with those arguments. Arbitrary native components therefore cannot use the present layout path on a worker. Some text shaping already runs on the worker through a shared text system, with locks whose contention still matters. [Element lifecycle](/Users/erwin/Code/gpuix/zed/crates/gpui/src/element.rs:53), [measurement callbacks](/Users/erwin/Code/gpuix/zed/crates/gpui/src/taffy.rs:24), [worker measurement](/Users/erwin/Code/gpuix/packages/native/src/renderer/host_runtime.rs:550).

With C, ordinary React layout effects cannot generally read the just-committed native layout synchronously. A synchronous getter can return a labeled snapshot. An asynchronous measurement can return fresh native geometry. Neither automatically provides the measure-correct-before-paint behavior described by React. [React useLayoutEffect](https://react.dev/reference/react/useLayoutEffect).

Several reviews initially proposed drawing without presenting, then waiting for a correction. GPUI's draw already swaps frame records, installs input handlers, and runs focus notifications. Holding physical presentation alone can leave input targeting geometry that the user cannot see. A timeout also means the no-intermediate-frame guarantee can fail. This needs a new lifecycle contract, not just a timer. [Window::draw](/Users/erwin/Code/gpuix/zed/crates/gpui/src/window.rs:2920).

B offers another route. Compute an isolated candidate layout, let React measure and correct that candidate, and publish it after effects settle. The UI continues to operate the old coherent revision. GPUI would need one shared layout implementation with immutable measurement inputs and a supported way to adopt its output. Running a second independent layout system and recomputing everything on UI would lose much of the benefit.

Candidate geometry is exact for captured inputs. It is not necessarily current screen geometry. Resize, font changes, or native edits can invalidate it before adoption. The system needs dependency checks, retry limits, and rules for effects whose layout assumptions became stale. Normal useLayoutEffect does not automatically rerun because a native dependency changed. This is the difficult part of B, and the reason it remains a prototype rather than an established answer.

For tooltips, linked scroll regions, and caret placement, native layout relationships can often remove the need for a JS measurement loop under either design.

## Native state must survive delayed React work

Consider an input at native edit version 40. React starts a revision using that observation. The user types again, and native state advances to 41 before the revision arrives.

The arriving acknowledgement of edit 40 must not replace edit 41. A current acknowledgement should preserve caret, undo, and composition. An application replacement needs an explicit expected edit version; an unconditional reset must be a distinct operation. Deferring a replacement during IME is not sufficient by itself. Its version must be checked again when composition ends.

Use versions for the relevant state: editor edits, list interaction, viewport, fonts, and document content. One global version would invalidate unrelated work. Preserve separate cache dependencies too. A search cursor change must not invalidate the searchable content groups. [Current search revisions](/Users/erwin/Code/gpuix/packages/native/src/retained_tree.rs:39).

Pin one description revision for the whole native frame, including virtual rows built after root rendering. Geometry, hit testing, selection, and event targets must refer to that same accepted scene. An immutable root pointer alone does not establish that consistency.

## Coalescing needs an explicit command contract

Suppose React commits R1, which creates input A and requests focus. Before UI consumes it, React commits R2, which removes A.

There are two valid policies. A best-effort focus request can be rejected because A is absent when processed. A guaranteed ordered command requires R1 to remain available long enough to execute it. Mounting R1 need not mean presenting its pixels. The API must distinguish these policies; silent cancellation cannot satisfy an ordered-command promise.

I recommend replaceable visual revisions plus explicitly ordered commands. Seal a commit group only after its synchronous layout effects have added their commands. Revisions required by those commands are barriers to coalescing. Return an explicit result for rejected or cancelled commands.

Event policy also needs a decision. A queued click may come from a scene whose callback has since changed in React. A revision-based event policy keeps subscription versions until native acknowledges that no older event can arrive. A current-handler policy uses the latest callback. Both have tradeoffs; simply deleting handlers on React unmount can lose events from a scene still on screen. I prefer versioned subscriptions for predictable event provenance, with bounded retention and explicit retirement.

## Caching is separate work

Creating a GPUI Entity does not automatically cache its rendering. The existing cached view path requires a definite outer size and checks bounds, clip mask, text style, and invalidation. Moving content and intrinsic-height chat rows can defeat those conditions. Parent invalidation also matters. [GPUI view cache](/Users/erwin/Code/gpuix/zed/crates/gpui/src/view.rs:224), [ancestor invalidation](/Users/erwin/Code/gpuix/zed/crates/gpui/src/window.rs:1955).

Fable found another obstacle: Window::refresh sets a flag that bypasses view caching. The current macOS GPUiX mutation path calls it, and several GPUI interaction paths also call it. Useful cache reuse therefore needs a checked invalidation design, not just more entities. [Mutation invalidation](/Users/erwin/Code/gpuix/packages/native/src/renderer.rs:1273), [window refresh](/Users/erwin/Code/gpuix/zed/crates/gpui/src/window.rs:2032).

GPUiX has another constraint. It clears and rebuilds text selection, highlight, text inspection, and bounds records through frame callbacks. Cached GPUI scene replay skips those callbacks. A cache that preserves pixels but loses selectable text or automation bounds is incorrect. [Selection reset](/Users/erwin/Code/gpuix/packages/native/src/text/paint.rs:102), [bounds reset](/Users/erwin/Code/gpuix/packages/native/src/automation.rs:50).

A restricted GPUiX metadata cache is possible. General coverage needs correct replay order for nested and deferred draws, clipping, and speculative layout rollback. A general GPUI frame-metadata API is one possible solution. Intrinsic-size caching is another research item; it must preserve flex/grid constraints, min/max-content sizing, baselines, and native state dependencies.

Until these tests pass, no review can promise that a frame costs only the changed subtrees.

## Transport, memory, and extension rules

N-API can pass typed data and opaque native handles. Switching engines or adding JSI is not necessary to remove JSON. Compare direct create/clone calls with batched typed data; per-call overhead and JS-to-native string copies still count.

Share unchanged text, styles, documents, and child sequences. Account for changed paths, wide child arrays, indexes, JS handles, event callbacks, preparation jobs, and old revisions. A single latest-revision slot does not bound everything that can retain a revision. Nor does structural sharing guarantee lower peak memory.

Use a bounded, safe publication mechanism first. A short mutex around pointer exchange can be sufficient. A raw atomic pointer is not a complete lifetime design. Large final drops must not occur accidentally on UI. A reclamation queue also needs a saturation policy and must account for references held by frames and native instances.

Virtualization must cover both React and native data. Native scrolling can continue while JS is blocked, but it cannot invent rows that the application has not supplied. A windowed React collection needs a defined placeholder or retained data when scrolling reaches an unavailable range. A native document component can render already loaded content without requesting another React row.

Native extensions should have three explicit parts: typed immutable props, optional pure preparation, and a UI-owned instance. Only the instance receives live GPUI context. Keep one schema for Rust and TypeScript. Keep consumer shaders and dependencies in consumer crates, and verify an exact composed-build fingerprint in host and worker. Preserve the current shared-device texture composition contract, including premultiplied alpha, float RGB above alpha, clipping, opacity, and resource lifetime.

## Tests that decide the design

1. Compare strong A and C with the same typed transport and native components. Use rich chat, a windowed table, an editor, and a deliberately wide host tree. Measure UI apply/build/layout/paint, input latency, queue age, allocation, and peak memory.
2. Block application JS while scrolling, typing through IME, using linked panes, and running native effects. Also test delayed input echoes, create-focus-remove commands, callback replacement, overflow, and teardown. Native independence means no JS wait, not zero native work.
3. Prototype subtree caching. Check pixels, selection, search ordinals, bounds, input, accessibility, deferred overlays, and font/viewport changes. Measure actual cache reuse and frame cost.
4. Prototype pure layout and adoption for flex/grid/text plus one native stateful leaf. Verify candidate measurement against adopted output under resize, font, and IME races. Keep the old scene interactive while React is blocked. Measure retries and retained memory.

Choose refresh-rate targets for the test hardware. An 8.33 ms frame period is a useful 120 Hz target, not a measured result or a guarantee from this review. Record input-to-submission separately from physical presentation.

The result to seek is a measured choice between A, C, and B, with explicit React semantics. Shared descriptions are the leading data model. Native ownership of interaction is the firm requirement. Layout placement and cache design remain the decisions that most need evidence.

## Reports

- [Fable 5.1 Extra High](fable.md)
- [DeepSeek V4.1 Flash](deepseek.md)
- [GPT-6 Astra Extra High](astra.md)
- [Gemini 3.8 Flash](gemini.md)
- [Common review brief](brief.md)
- [Parent source checks](parent-source-checks.md)
