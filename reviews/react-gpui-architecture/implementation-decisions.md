# Implementation decisions

The user authorized autonomous implementation, testing, branch rename, and commits on 2026-09-19. This record accompanies checkpoints; it does not require another approval pause. The durable goal is to deliver usable new Rust and JavaScript packages, not stop at an API sketch.

## Documentation checkpoint

| Decision | Alternative | Confidence | Failure case |
| --- | --- | --- | --- |
| Preserve earlier independent reviews with explicit historical notices. | Remove recommendations superseded by the user's decision. | High | A reader who skips the notices could mistake an old proposal for the accepted design. |
| Record the scenario matrix before implementation. | Rely on conversation history. | High | The matrix must be updated when implementation evidence changes a requirement. |
| Rename the local branch to `bridge/minimal-react-gpui`, preserving the previous remote branch. | Keep the Cherry-specific branch name. | High | Consumers must receive the new branch and checkpoint refs in the final handoff. |
| Develop a new crate and package in this repository, using existing code selectively. | Refactor the old renderer in place or start another repository. | High | Reusing a module without reviewing its dependencies could retain the old coupling. |

The documentation checkpoint changes no runtime behavior. Source links were checked. The records distinguish observed regressions, proposed APIs, and untested performance expectations. I stand behind this checkpoint as a record of the accepted direction, not evidence of a finished implementation.

## Accepted implementation boundary

- React runs on the application worker. The native platform loop never waits for application JavaScript.
- One UI-owned native host model holds mounted descriptions and view handles. No worker Rust mirror exists.
- Ordinary GPUI `Render` views can implement a small binding trait. Optional capabilities expose native events, commands, asynchronous queries, and child composition.
- GPUI owns layout, interaction, and painting. Fresh synchronous native queries from React are not supported.
- Committed operations and dependent commands retain order. Speculative React work does not mount native resources.
- Native interaction state has one owner. Delayed input acknowledgements cannot replace newer edits.
- Core contains general translation/runtime code. Cherry components, themes, plugin controllers, and Pierre domain models remain external.
- Real-use failures get failing regression tests before their fixes. Existing tests are reviewed for valid observable requirements before adaptation.
- All native windows stay in the background. Consumer repositories are read-only unless changes are coordinated.

## Progressive validation

1. Typed registration and mutation protocol; lifecycle, malformed transactions, admission limits, and event identity.
2. Real GPUI view wrapped from React; source worker, blocked-worker native progress, props, events, commands, queries, and cleanup.
3. Containers/text/children; React interruption, keyed moves, refs, commit/effect ordering, and multiple instances.
4. Native editing, focus, IME, scroll, variable-height lists, atomic anchors, delayed data, and resource lifetime.
5. Rich content and external editor/diff examples using real consumer code where the public boundary permits it.
6. Installed/relocated packages, documented usage, performance traces, complete regression checks, and final consumer handoff.

This order is a development sequence. Later items remain required work.

## Binding and reconciler checkpoint

| Decision | Alternative | Confidence | Failure case |
| --- | --- | --- | --- |
| Start with one JSON encoding per transaction and typed Serde component props. | Introduce a binary codec before measuring the new path. | Medium | Large prop payloads can still cost time. The protocol and driver remain separate so encoding can change without rewriting components. |
| Fail the root when the bounded commit queue overflows. | Add a React admission scheduler or an unbounded retry queue. | Medium | Sustained overload stops the root. Accepted commits are never silently discarded. A measured admission policy can replace this explicit failure later. |
| Retain callback versions until ordered native retirement. | Always invoke the newest callback. | Medium | The native host must honor event/ack order, including events emitted during native commands. Full host tests are still required. |
| Use `AnyView` and ordinary GPUI entities, with optional capability registration. | Require a new native render interface. | High | Advanced text inspection and part replacement need explicit native services; wrapping alone does not supply them. |
| Keep component props in the component and decode transient updates before application. | Keep a second complete typed prop model beside every component. | High | Atomic transaction validation must be completed before native side effects. The UI mutation owner is the next implementation step. |
| Use React 19.2 and reconciler 0.33 for the new package. | Preserve the older package's broad React compatibility claim. | High | Earlier React versions are not supported by this package. |
| Use a shallow prop comparison and React's immutable-prop convention. | Serialize unchanged values to detect in-place mutations. | High | A caller mutating the same nested object is unsupported. Callback-only updates send no native props. |
| Keep the package independent of the old renderer. | Import old native/React package internals. | High | Useful components require reviewed extraction or adapters rather than a broad import. |

The first checks are three native binding tests and ten actual React reconciler
tests. They are foundation tests, not full application validation. Two added
regressions failed before correction: callback-only updates resent native props,
and a root that failed on queue saturation could leave `flush` blocked behind an
earlier request. The corrected tests pass. Native host, mutation ownership,
component integration, and packaged distribution remain active goal work.

I stand behind this checkpoint as a tested boundary implementation. I do not
claim that it yet fulfills the complete runtime goal.

## Native ownership and host checkpoint

| Decision | Alternative | Confidence | Failure case |
| --- | --- | --- | --- |
| Keep a temporary overlay of affected topology during validation. | Clone the whole tree or partially mutate live views before validation finishes. | Medium | A transaction that changes many children still copies their child-ID vectors. Performance must be measured for large mounts. No props or live view state are cloned. |
| Synchronize changed native child slots before commands/queries and at transaction end. | Rebuild child handles after every individual insertion. | Medium | An extension that reads child composition inside `set_props` must not assume later mutations already applied. Commands see the preceding complete composition. |
| Allocate host IDs at commit and enforce monotonic native creation. | Keep permanent deleted-ID tombstones or allow reused IDs. | High | Speculative React descriptions have no native ID until mount. Public refs are assigned after placement. A nested-tree regression verifies creation order. |
| Retire subscriptions in GPUI's ordered effect queue and retain the entity until retirement. | Clear subscriptions immediately. | High | Immediate clearing lost events emitted before removal; a failing test demonstrated it. The complete worker example now verifies command-before-removal delivery. |
| Hide React subtrees by removing their view handles from the rendered child list while retaining instances. | Use an outer wrapper or inherit the old visibility test. | High | Hidden content occupies no layout space. This matches the intended suspended-content behavior and avoids changing visible layout with wrappers. |
| Keep the native host in a separate crate and require explicit composition objects in JS. | Import the old runtime or select a fallback binary. | High | The app must pass the same compiled composition in both entries. Loader ergonomics and installed archives still need broader tests. |
| Reuse the existing MacPlatform lifecycle and scheduling API. | Modify GPUI again or drive AppKit from JavaScript. | High | The new host still needs signal and full failure-lifecycle coverage before release. No GPUI source changed in this checkpoint. |
| Use a background native timer as the initial blocked-worker probe. | Claim visible frame cadence from an occluded window. | High | This proves native executor progress, not keyboard/scroll behavior or physical display smoothness. Those remain separate tests. |

Validation now includes nine core Rust tests, three host queue tests, fourteen
React/transport tests, and the real counter composition. The native example
passes source and relocated compilation, three sequential sessions, startup and
missing-worker errors, invalid component props, and a command immediately before
unmount. During a 300 ms application-worker block the native counter advanced
from 7 to 67; its single background draw is not reported as a display-rate result.
The packaging check also exposed Bun writing temporary `.bun-build` files into
the checkout. A failing assertion now protects the workspace; compilation runs
from the owned temporary directory and relocation still passes.
Another failing test measured an extra GPUI render after a state-only query.
Root notification now follows root topology changes only. Component prop and
state updates use their own ordinary GPUI notifications. Read-only queries and
callback route changes do not request a redraw.

I stand behind the tested native owner and host checkpoint. It is still not the
complete requested replacement: controls, input/IME, lists, text services,
consumer examples, platform coverage and package release remain active work.

## Native input checkpoint

| Decision | Alternative | Confidence | Failure case |
| --- | --- | --- | --- |
| Use construction-only `initialValue` and `initialMultiline`; use revision-checked commands for later text replacement. | Recreate a controlled `value` prop with an echo/acknowledgement queue. | Medium | Applications that format every keystroke need an explicit stale-result policy. Changing editing mode requires remounting. This is documented, rather than presented as React DOM input compatibility. |
| Reject application replacement during IME composition. | Queue replacements or force composition termination. | Medium | A pending application action may need to retry after composition finishes. The control never retries a stale command itself. |
| Reuse the existing native editor's editing algorithms in a separate controls crate. | Rewrite grapheme movement, IME ranges, selection geometry, clipboard, undo, and autoscroll. | High | This transfers real implementation constraints too: undo keeps up to 200 text snapshots, and input is not intended to replace Pierre's large-document editor. Source notices remain included. |
| Expose a small typed style set and reject unknown fields. | Import the old renderer's broad style schema with partly unimplemented behavior. | Medium | Existing GPUiX style objects need an explicit adapter. Richer styles can be added with native tests; this checkpoint does not claim full style compatibility. |
| Keep input properties and live state in one GPUI entity. | Retain a prop buffer plus an inner editor state entity. | High | Initial values are consumed at construction. The bridge and worker never retain another native text buffer. Undo and GPUI text-layout caches serve separate native functions. |
| Increment the revision for selection and composition changes as well as text. | Version text alone. | High | A delayed selection-sensitive operation rejects after a caret move, even if the text is unchanged. This prevents it from replacing newer interaction state. |
| Preserve the editor's native keyboard and mouse behavior, then fix failures against focused tests. | Copy its old assumptions without checking them, or redesign all editing behavior. | High | The input's old wheel handler blocked parents at boundaries; the GPU test failed before the fix. The old caret used wall time with a GPUI clock anchor; the deterministic test failed before that fix. |
| Expose paint-tagged geometry as an asynchronous observation. | Force layout inside a query or copy a second text snapshot for each paint. | High | The geometry can describe an earlier revision. It has an explicit revision and is null before the first paint. |
| Test the installed platform input handler and native GPU output on the main OS thread. | Treat command-only input tests or a mock text renderer as sufficient. | High | This validates macOS behavior in an offscreen window. It does not certify other platforms, a physical IME candidate panel, or display latency. |
| Reject JSON numbers that overflow native `f32` style values. | Let Serde silently cast finite JSON values to infinity. | High | A failing test demonstrated `1e100` becoming infinite padding. The typed decoder now rejects it before component changes. |

The checkpoint has 22 passing native control tests, a GPU-backed input scenario,
and the production React worker fixture in source and relocated compiled forms.
The GPU scenario checks keyboard dispatch, selection, undo, platform IME ranges,
text pixels, accessibility values, consumed-wheel callback propagation, and parent
boundary scrolling. The production bridge checks retained input identity, typed
events, explicit stale-command errors, selection commands, and props that preserve
native text. The earlier fourteen React/transport tests also pass.

Defaults retained from the editor include a 500 ms caret period, a 700 ms undo
coalescing interval, a 200-entry undo limit, and a 16 ms drag-autoscroll timer.
They describe native editing behavior; they are not performance measurements.
The current input emits change, selection, submit, and captured-key events.
General focus/blur events and cross-component selection services remain later
work, as do the larger control library, lists, rich content, consumer integration,
and distribution. The standard controls live outside the minimal binding crate.

I stand behind this input checkpoint and its stated limits. The full replacement
is still under implementation; this checkpoint does not claim final completion.

## Containers, lists, and paint metadata checkpoint

| Decision | Alternative | Confidence | Failure case |
| --- | --- | --- | --- |
| Use GPUI's uniform-height hint API when the logical count changes. | Add a general GPUI splice API that accepts hints for only inserted rows. | Medium | The current API revisits the height index and changes measured entries into hinted entries. Correctness cases pass, but count-change cost and cache retention still need performance work before final release. This is a known open item, not a completed optimization. |
| Start with an explicit text leaf. | Import the old document selection/search engine with its retained-tree dependencies. | Medium | `Text` currently has native paint inspection but no cross-component selection or search. Those remain required goal work. Separate native text children remain separate layout items; a full line should use one text value for now. |
| Use direct native children as list rows, with an optional logical count and supplied window. | Add an application data model or a second row-description tree. | High | Each logical row needs one stable native root. A fragment or Suspense boundary that changes the number of direct host children must sit inside that row root. Applications must retain or reload content they need; native virtualization does not supply missing data. |
| Add a default `ReactChildren::children_changed` hook. | Rebuild all children on every prop change, poll all heights every frame, or require JS to send a remeasure after every descendant update. | High | Containers with native caches need invalidation before dependent commands. A GPU regression demonstrated a changed offscreen row using its old height for a same-commit negative anchor. The host now groups affected direct branches without copying their props. Native changes outside bridge transactions still use normal GPUI invalidation APIs. |
| Preserve the reader's keyed row identity across a full-list reorder. | Accept GPUI's replacement-range anchor for that operation. | High | The GPU test first jumped from row 0/offset 60 to row 0/offset 0. The binding now maps the existing row handle to its new index and calls GPUI's normal `scroll_to`. Top-pinned feeds still show prepended rows; windowed logical index changes remain application-owned. |
| Use normal GPUI hit testing for generic containers and text, with explicit `blockMouse` on containers. | Unconditionally use `block_mouse_except_scroll` as the old renderer often does. | High | That method also excludes ancestor hitboxes. Two GPU failures showed text and a nested layout container swallowing their parent's click. The new controls use GPUI's normal path by default; overlays can opt into native mouse blocking. The source comment states this distinction from the old renderer's hitbox rule. |
| Share native horizontal scroll handles through a window-scoped group. | Mirror offsets through React, or keep separate handles with a frame correction. | High | Group members must have equal content and viewport widths. The registry holds weak handles and removes expired groups on resolution. GPU tests compare header/body geometry in 72 frames with fractional deltas, separate groups, and detachment. |
| Tag painted measurements with native root, draw, transaction, viewport, and scale. | Use only a component prop revision. | High | A parent's padding or font can change a leaf's bounds without changing its own props. A paint scope delegates GPUI's element lifecycle without adding a layout box. Nested hosts restore the outer scope; native transactions outside paint see no active scope. These tags do not claim physical presentation. |
| Keep programmatic commands and missing-row requests separate from data loading. | Add fetch generations and cancellation to the renderer. | High | The application must reject stale asynchronous data responses and commit rows with their intended anchor. The renderer preserves accepted operation order and requests missing rows, but cannot manufacture application data. |

Two proposed assertions were corrected after inspecting GPUI, rather than
changing native behavior to satisfy them. An offscreen row's exact new height
need not be known until layout measures it; estimates are intentional. The
replacement test restores a negative anchor that actually requires that row's
new height and failed before the fix. Also, GPUI may paint between two separate
native calls. The stale-geometry test now queries inside the same atomic
transaction as the layout change, then verifies the later frame tag and bounds.

Validation includes eleven core tests, twenty-two control tests, three host
queue tests, fourteen React/transport tests, and three GPU-backed scenarios.
Source and relocated compiled workers validate input plus a 100,000-row logical
list with sixty supplied rows, a correct first native frame, an ordered window
replacement, and keyed identity. The GPU cases also cover the 50,000-row wheel
delta, prepend overflow transition, tail following, native scroll callbacks,
axes, negative anchors, keyed row moves, outer boxes, mouse routing, and hidden
input regions. No GPUI source changes are included in this checkpoint.

The callback-generation contract still needs a dedicated old-pixels/new-native-
state test in the broader lifecycle pass. Current generations follow native
subscription application and GPUI effect order; they do not freeze component
state until OS presentation. That distinction must stay explicit in the final
contract. Document text services, native input during a blocked worker, large
consumer fixtures, measured performance, and package release remain goal work.

I stand behind this correctness checkpoint and its stated limits. It is not the
final performance or release checkpoint.

## List measurement and allocation checkpoint

| Decision | Alternative | Confidence | Failure case |
| --- | --- | --- | --- |
| Measure native list mutation cost separately from frame cost. | Mix transport, layout, and painting into one benchmark. | Medium | This result cannot establish application frame rate or total bridge overhead. Those measurements remain required. The test records both direct GPUI and binding operations with the same height hints. |
| Preserve measured rows and existing hints when filling missing uniform hints. | Treat the initializer as an implicit remeasure operation. | High | Callers that need remeasurement must use the explicit GPUI remeasure API. A draw-based regression first failed with zero retained measured rows instead of three. |
| Add two general GPUI splice methods with uniform height hints. | Traverse the full height index after every splice, or access GPUI internals from the binding. | High | The methods add a small API obligation to the GPUI fork. They share the existing splice implementation and preserve its focus and anchor behavior. The source and upstream issue search did not provide this operation. |
| Compare the list's own style and supplied range before invalidating its rows. | Invalidate all supplied rows on every prop update. | Medium | Layout changes inherited from a parent still need a separate real-use test. This checkpoint does not claim complete coverage of inherited font changes. No second style model is stored. |
| Keep retained-child prop invalidation even when a sibling changes in the same transaction. | Let the structural child callback replace all branch invalidations. | High | A parent that preserves unchanged child measurements would use stale geometry. The new core regression failed before the fix; the GPU case combines a height change, sibling append, and negative scroll anchor. |
| Count allocations only on the measured thread with a test-only allocator. | Add runtime instrumentation or use process memory deltas. | High | This omits other threads and allocator metadata. It measures requested allocation bytes, not resident memory. The runtime allocator is unchanged. |
| Require less than fourfold allocation growth for one append when retained rows grow from 1,000 to 100,000. | Assert a machine-specific time limit or exact allocation count. | Medium | A future index implementation may need a revised structural guard. The margin permits tree-depth variation while rejecting a full-index traversal. The test failed before the fix with 264,688 versus 19,823,984 bytes. |
| Clear the notification log after the invalidation test's initial tree construction. | Count construction callbacks together with the subsequent update. | High | Initial nested composition can itself invalidate ancestors. The test still requires one notification per affected branch for the measured update. It now also requires a retained child's prop notification during a structural change. |

The measured 100,000-row binding update fell from P50 2,421.92 microseconds and
20,039,688 allocated bytes to 3.04 microseconds and 11,792 bytes. Direct GPUI's
equivalent hinted splice measured 3.13 microseconds and 11,792 bytes. The small
timing difference between the two new paths is noise. Both figures exclude
React, transport, layout, paint, and presentation.

Validation passed all 32 GPUI list tests, 12 binding tests, 25 controls tests,
strict binding and controls Clippy checks, and the input, container, and list
GPU scenarios. The manual benchmark is separate from the normal test suite.
Source and relocated compiled worker fixtures also passed after the rebuild.
The benchmark and raw results are in `docs/bridge-list-performance.md` and
`docs/benchmarks/bridge-list-count.json`.

I stand behind this checkpoint. The open work includes full-frame performance,
native interaction during a worker stall, lifecycle and failure cases, document
text services, consumer fixtures, and installed package distribution. The user
authorized autonomous tested commits; this audit records the decisions without
adding a new approval step.

## Blocked-worker interaction checkpoint

| Decision | Alternative | Confidence | Failure case |
| --- | --- | --- | --- |
| Use a fixture-only native driver and shared atomic flags to prove the worker is blocked. | Send each input through JavaScript or infer progress from a timer count. | High | A test that needs the blocked worker to inject input cannot test this boundary. The flags carry no UI data and do not enter production crates. |
| Force native draws and inspect GPU images in a hidden window. | Raise an active window or claim a display cadence from an occluded window. | High | This does not establish physical presentation latency or frame rate. It verifies GPUI input, layout, and paint during the stall. |
| Use GPUI keystroke dispatch and the platform input handler for IME. | Inject system-wide keyboard events. | High | This does not test OS keyboard routing or a specific IME application's candidate window. It exercises native editor handling without focus theft. |
| Enable GPUI and macOS test support only through the composition fixture's optional feature. | Expose input injection and image capture on the production host API. | High | Test builds have more dependencies. Production host and controls crates gain no automation API or test flags. The first pixel attempt showed that GPUI test support alone does not enable the macOS image backend. |
| Wait for both a caret pixel transition and its native timer notification. | Sample exactly one clock phase. | High | A GPU readback can cross the phase boundary before the queued timer task runs. The compiled fixture caught this test race. The new check retains both requirements with a two-second deadline. |
| Replace the React event callback before the worker drains queued native edits. | Check only eventual event delivery. | High | Old subscriptions must survive until native retirement. The source and relocated cases require all queued edits on the original callback, with increasing native revisions. |
| Invalidate dependent geometry whenever a typed native command is invoked, including an error return. | Invalidate only successful commands or add rollback state. | High | A no-op failed command can cause extra invalidation. A partially completed native command can change size before failing, so skipping it is incorrect. Invalid schemas still invoke nothing and cause no cache invalidation. The regression failed with an empty parent notification list before the fix. |
| Add a fixture TypeScript configuration and local React type dependencies. | Rely on Bun execution without checking the example types. | High | The initial strict check found missing fixture type dependencies. The fixture now has a repeatable `typecheck` script. |

Test-only timing choices are a four-second native animation, 25 ms pixel
sampling, a two-second blink deadline, a five-second start deadline, and an
eight-second worker deadline. They are failure bounds, not runtime scheduling
promises. The animation assertion checks both its native phase and changed GPU
pixels. The caret assertion also requires logical window focus throughout.

Thirteen core tests and strict Clippy checks pass. The complete source and
relocated fixture suite passes with the optional interaction driver. Strict
TypeScript checking passes for all fixture entries. The worker produced no
callbacks while blocked; native typing, selection deletion, undo, IME, list
scrolling, hover, caret and animation continued. A delayed replacement with the
old input revision was rejected afterward. No GPUI source change was needed.

I stand behind this checkpoint. Signal and shutdown cases, inherited list
geometry, document text services, external consumer fixtures, broader
performance measurements, and distribution remain required work.

## Shutdown and resource lifetime checkpoint

| Decision | Alternative | Confidence | Failure case |
| --- | --- | --- | --- |
| Retain one native signal reader for the process and a weak reference to the active session. | Unregister handlers after each host, or depend on JS signal listeners. | Medium | The API takes ownership of SIGINT and SIGTERM handling after first use. It does not forward custom JS listeners. The old host already used this method because removing signal-hook's final action can leave the signal ignored. Tests verify graceful active-host shutdown and default SIGTERM behavior afterward. |
| Treat active-host SIGINT and SIGTERM as graceful completion of `runApplication`. | Reject the launcher promise or preserve shell signal exit status. | High | Callers that require a signal exit status must set it themselves. This preserves the earlier host's contract, including worker cleanup. A real test first ended directly with SIGTERM before cleanup. |
| Add the general GPUI `Window::on_close` callback. | Use only `on_window_should_close` plus an app-quit handler, or attempt native cleanup after the loop returns. | High | A should-close handler does not cover direct `remove_window`. After the loop, the window is already gone. The signal regression recorded `mounted, drop` and missed `unmount`. The source and upstream issue search showed no existing live-window close callback. The new hook covers native removal and shutdown without a renderer-specific GPUI dependency. |
| Make close callbacks synchronous, persistent for the window lifetime, and one-shot. | Add cancelable or asynchronous observer machinery. | Medium | Long cleanup can delay window close. Callbacks registered after closing starts do not run, and callbacks cannot cancel closure. Components should cancel window-dependent tasks here; longer app-level shutdown can use GPUI's existing app-quit mechanism. These limits are documented. |
| Run the close callback after the removing update releases its root borrow, but before removing the window. | Invoke it immediately inside `remove_window`. | High | Immediate invocation could try to borrow a root that is still handling an event or command. GPUI tests update the root from the callback and require cleanup before the existing window-closed notification. |
| Capture the bridge root weakly in the native close callback. | Add another owner of the whole host. | High | The runtime's existing root handle keeps it alive until teardown. The callback adds no cycle or second tree. It calls the same explicit `Host::clear` used by normal unmount. |
| Use temporary file records for test readiness and lifetime. | Wait for worker console output or infer cleanup from successful exit. | High | Worker console output can wait for the main JS thread while AppKit owns it. The first test attempt waited without sending a signal; this was a test defect, not a runtime failure. File readiness then exposed the actual signal and unmount failures. |
| Terminate a blocked worker after the existing two-second grace period. | Wait indefinitely for React cleanup or pretend it ran. | High | React cleanup cannot execute while JS is blocked. Tests require native cleanup but explicitly do not claim React effect cleanup in those cases. Responsive workers must run layout and passive cleanup and the process exit handler. |
| Exercise event overflow through a native producer of 10,000 events. | Only test the queue data structure. | High | This intentionally exceeds the queue during one native operation. It does not prove frame fairness under large atomic work. The source and compiled cases require an explicit overflow failure and exactly-once native teardown. |

Validation passed all 278 GPUI library tests, three host queue tests, fourteen
React and transport tests, strict host and fixture Clippy checks, and strict
fixture TypeScript checks. The complete source and relocated compiled fixture
suite passed. Both forms cover normal unmount, SIGINT, SIGTERM, blocked-worker
signal and native window close, event overflow, worker failure after mount,
early worker exit, and default SIGTERM behavior after the host ends. Native
lifetime records require `mounted`, `unmount`, and `drop`, each exactly once.
The existing list, input, interaction, repeated-session, and startup-failure
fixture checks also pass with the new GPUI close hook.

I stand behind this checkpoint. The goal remains active. Required next work
includes inherited list geometry, document text services, external editor/diff
and GPU component fixtures, broader performance measurements, and installed
package distribution. No Cherry or Pierre component files were changed.
