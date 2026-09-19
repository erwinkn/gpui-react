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

## Host library artifact checkpoint

| Decision | Alternative | Confidence | Failure case |
| --- | --- | --- | --- |
| Build `gpui-react-host` as a Rust library only; the application composition owns the `cdylib`. | Keep both library types on the host or require separate target directories. | High | Building the standalone host and the test composition in one target directory reused an unqualified `libgpui_react_host.rlib` from another GPUI feature graph. The composition then failed with incompatible types from the same source paths. The ordinary Rust library uses qualified artifacts and avoids that collision. |
| Remove the host's N-API build script and leave `napi_build::setup()` in the composition. | Keep linker arguments on a crate that no longer emits a dynamic library. | High | Cargo warned that the host's `rustc-cdylib-link-arg` had no target. The composition already supplies those arguments. Its source and compiled runtime tests confirm the N-API exports still link and load. |
| Add a build-graph regression that alternates application and standalone host builds in one target directory. | Clear Cargo output before every test. | High | Clearing output would hide the original collision. The regression uses normal cached builds, including the fixture's optional GPUI test features. |

The failing composition build was retained in
`/tmp/bridge-document-composition-build.log`. The alternating-build regression,
three host tests, and strict host Clippy checks pass. The full source and
relocated fixture suite passes, including native interaction while JavaScript
is blocked and shutdown/resource cleanup. No GPUI source change was needed.

I stand behind this checkpoint. It fixes the composition boundary without
adding a second runtime or a default native addon. The user authorized tested
commits without further approval. Document work and the remaining consumer,
performance, and distribution requirements stay active.

## Native document and inherited list geometry checkpoint

| Decision | Alternative | Confidence | Failure case |
| --- | --- | --- | --- |
| Keep document services on ordinary uncached GPUI views. | Add paint-cache metadata replay in the same change. | Medium | GPUI cached-view replay skips the callbacks that supply selection, search, and inspection metadata. The current controls do not cache views. A custom component that uses cached views must avoid that path for participating text until replay is tested. |
| Preserve selected source text as a snapshot, including after virtualization or removal. | Map live selection ranges through every source edit or discard selection on unmount. | Medium | Copy can return the text selected before an edit. This is explicit in the API. A wash paints only when the selected bytes at that logical key and range still match. An editor with live range transformation keeps that policy in its own native model. |
| Extend virtualized drag selection through overlapping painted entries. | Keep the full source document in the text service. | Medium | A jump that skips all overlap cannot recover unseen text. The native drag clock limits movement to half a viewport per completed paint. Missing or unloaded content still belongs to the application's data model. Select All covers registered text; full export does not use this service. |
| Use a 16 ms native drag clock, distance gain of 15 per second, and a half-viewport step limit. | Introduce a shared motion scheduler or pass drag frames through JavaScript. | Medium | These are control behavior choices, not measured optimal constants or display-rate promises. The paint limit preserves virtualized overlap and prevents accumulated invisible steps. Release and unmount cancel the task. |
| Fix eligible scroll regions at press time, using native registered viewport bounds in reverse paint order. | Add every region crossed later by the pointer, or store a separate scroll-container tree. | Medium | The original dynamic region scan scrolled an unrelated sibling. The new test covers stacked siblings and nested container fallback. Arbitrary overlapping custom scrollers may need more explicit source ownership; that case is not claimed tested. |
| Search each complete logical text separately with Rust regexes. | Join all text into one document string, or reproduce JavaScript regex semantics. | High | A query cannot span logical paragraphs. Regex syntax and zero-length behavior follow the Rust regex crate. Empty matches do not create highlight rectangles. Invalid patterns reject during prop validation. |
| Validate a query during every props decode, then reuse the previous matcher and range caches when its matching fields are equal. | Add a global compiler cache or skip validation for cursor-only changes. | High | A cursor update still pays regex construction at admission. It does not rematch text. The Arc identity regression verifies range reuse for colors, cursor, geometry, and offsets. A measured admission bottleneck may justify a bounded compiler cache later. |
| Keep separate content and query dependencies; omit cursor and colors from result invalidation. | Use one props revision or the match count as the cache/event key. | High | Same-count query changes must report, while moving the active match must not rescan content. Tests cover both behaviors. Content revision tracks registered text, order, and options, not geometry. |
| Include query settings and frame tags in search events, and preserve painted query metadata in snapshots. | Read current props while returning older highlight geometry. | High | A test returned new offset 8 with the earlier paint's offset-2 matches before the fix. Snapshots now retain the painted matcher by shared handle and the painted offset. A delayed event identifies its own query and frame. |
| Supply an optional absolute match prefix on each logical text. | Require React to update a document-wide offset after every native scroll. | High | A row number is not a match prefix when rows have different match counts. Applications own full-source counts and prefixes for the query. The GPU regression scrolls directly to global active match 20 without a worker update. |
| Join React string/number interpolation once in the `Text` wrapper. | Retain split primitive text nodes and reconstruct groups during selection and search. | High | Nested React elements inside `Text` are rejected. Styled logical text uses native GPUI text runs. The source and relocated worker tests require one native value for interpolated text. Old split-node grouping state and its tests were removed because that representation is absent from this API. |
| Adapt the tested selection state with shared GPUI strings and exact change results. | Copy full source strings and hash all selected bytes on each drag event. | High | The ownership regression failed before this change. Selected spans now share source bytes, and joined text is produced only for an explicit query or copy. Selected content can retain source memory after its view disappears; clear releases it. Comet source and license notices remain. |
| Use Unicode word boundaries and whole graphemes for double click, with GPUI character hit testing. | Use alphanumeric character scans and nearest caret offsets for every mouse operation. | High | Before the fix, a combining-mark word selected one byte and a right-half emoji click selected nothing. The unit and GPU regressions now pass. This does not claim a full independent bidi text-geometry validation. |
| Derive highlight and selection rectangles from GPUI shaped lines and wrap boundaries. | Reshape text or maintain an independent layout model. | High | Every painted text retains native geometry until the next paint. This is measurement data, not a second host tree. Tests cover hard lines, wrapping, font/width changes, and 300 lines without the old fixed-line cap. Broad rich-document costs remain to be measured. |
| Register text and scroll regions during paint; use prepaint only for native text hitboxes. | Keep speculative list prepaint entries as the visible document. | High | GPUI can discard speculative rows. Document content follows completed paint order, including native component text and list rows. Text outside a document adds no document hitbox. Nested documents restore the enclosing native scope. |
| Delay pointer capture and document focus until a drag starts, or a double/triple click selects. | Capture immediately on every text press. | High | The first implementation swallowed an ordinary clickable label. The regression now requires that parent click to arrive, while drag release outside the document still works. |
| Send small selection notifications and leave current-state reads asynchronous. | Send the joined selection text on every pointer move. | High | A handler must query when it needs text. Selection revisions and painted selection revisions remain separate. New listeners receive future events, not a replay of an earlier result. |
| Invalidate list height measurements when inherited native text metrics change during layout. | Compare only list props or repair its anchor after layout. | High | The old negative-anchor test put row 1 at y=40 after a parent font change; the required position was y=20. The wrapper uses GPUI's normal `remeasure_items` before layout and adds no layout box. It tracks geometry-dependent text style fields; direct list prop changes remain conservatively invalidated. |

Validation passed 43 controls unit tests, two list measurement tests, one list
allocation regression, strict controls and host Clippy checks, and fourteen
React/transport tests. The manual list benchmark remains separately ignored.
The input, container, list, and document GPU examples pass with offscreen
windows. The document image was inspected. The document example also checks
nested scroll fallback, sibling isolation, and release cancellation.

The source and relocated compiled worker suites pass after rebuilding the
composition. The new document case checks interpolation, query provenance,
UTF-16 selection, snapshot text, selection events, and clear. Existing list,
blocked-worker input/IME/scroll/animation, repeated-session, startup-failure,
and full lifecycle cases also pass. Strict fixture TypeScript checking passes.

Failing tests were captured before fixes for inherited font anchors, missing
virtual match-prefix props, drag autoscroll, click routing, Unicode words,
right-half glyph selection, source text copying, mixed-generation snapshots,
and unrelated sibling scrolling. The API and native component helper are
documented in the root, crate, package, and fixture READMEs. No GPUI, Cherry,
or Pierre source files changed in this checkpoint.

I stand behind this checkpoint under its stated contracts. It does not complete
the overall goal. Remaining required work includes real editor/diff and GPU
component compositions, native geometry-dependent controls, broader frame and
memory comparisons, the old-pixels/new-subscription race, installed/released
packages, and the final scenario audit. Cherry will receive one completion
handoff after that work, not an early integration claim.

## External GPU component checkpoint

| Decision | Alternative | Confidence | Failure case |
| --- | --- | --- | --- |
| Keep the example producer and React wrapper in a separate fixture crate and private JS package. | Add an effect-specific control or shader payload to core. | High | The example only produces a solid color on Metal. It proves the extension path and composition rules, not a complete effect library or browser driver. Cherry shaders remain outside the framework. |
| Use an ordinary GPUI `Render` view and delegate the bridge traits to normal native methods. | Reintroduce the old custom-element factory and renderer context. | High | Component authors must still use the same compiled GPUI build as the composition. Cargo metadata confirms one GPUI package and no `gpuix-native` dependency for this fixture. |
| Submit GPU clear commands on the window's shared queue and use private RGBA16Float textures. | Upload CPU pixels, wait for every producer command, or clamp RGB to alpha. | High | The format preserves HDR contributions above alpha. The tested red value 2 with alpha 0.25 and parent opacity 0.25 yields `[128, 0, 239, 255]` over blue. The producer performs no readback or CPU wait. |
| Allocate a new texture when size or color changes; reuse it for label-only updates. | Overwrite a resource that an earlier scene can still reference, or implement a frame-retirement pool in this fixture. | Medium | Large animated outputs may need pooling. This example does not claim an optimal allocation policy for video. Shared scene handles preserve prior resources until release, and removal clears native ownership. |
| Retain actual GPUI surface records in the lifetime test. | Assume the previous frame is still internally retained after every forced draw. | High | The first test failed because the test draw loop had already retired that frame. The corrected test keeps the recorded scene references explicitly, requires the old handle to survive resize/removal, then drops those references and verifies release. It does not require GPUI to keep already retired frames. |
| Make `initialColor` construction-only and expose native transition/cancel commands. | Echo the changing color through React on every frame. | High | Callers use a command to replace live color. Unrelated props and resize preserve native state. The source and relocated worker tests cover this contract. |
| Use GPUI's native clock and frame requests, with both component and application reduced-motion settings. | Use a JS timer or only the component prop. | High | The first test for GPUI's setting failed during an active transition. The example now finishes that transition on the next native draw and stops frame requests. The test changes only GPUI application state, not OS preferences. |
| Stop motion and report a distinct native error when texture production or composition fails. | Panic during paint or emit the same error on every frame. | Medium | The example does not implement GPU device recovery. It reports repeated identical errors once and preserves the last painted snapshot as older evidence. |
| Extend the existing native interaction driver to inspect this producer while React is blocked. | Infer independence from an elapsed-time query after the worker resumes. | High | The driver requires new texture generations and changed GPU pixels during the shared-flag stall. Readback can force extra native draws; generation counts and CPU draw counts are not display FPS. |
| Seed the new fixture lockfile from the tested controls dependency graph. | Keep a fresh resolution that upgraded many unrelated dependencies. | High | The first fixture build resolved newer dependencies. The final tests use the existing versions plus the new fixture packages. This avoids an unrelated dependency upgrade in the checkpoint. |

The color validation test, strict component and composition Clippy checks, and
strict TypeScript fixture check pass. The offscreen GPU example passes HDR,
opacity, clipping, corners, multiple-texture, text, click, resize, resource,
transition, retarget, cancellation, reduced-motion, and idle-frame assertions.
The initial GPU image was inspected.

The full source and relocated compiled worker suites pass. During the relocated
blocked-worker check, the texture pixel changed from `[25, 51, 102, 255]` to
`[46, 51, 82, 255]` and its resource generation advanced by 29. Existing input,
IME, scroll, hover, caret, motion, document, list, repeated-session, failure,
and shutdown checks still pass. These values prove native progress in that run;
they are not a presentation or throughput benchmark. No GPUI patch was needed.

I stand behind this checkpoint under its stated scope. The overall goal remains
active. The next consumer work is the shared Pierre viewport and optional
adapters, followed by the remaining geometry, event race, performance, package,
and completion-audit requirements. No Cherry completion message has been sent.

### Pierre ownership and compatibility agreement

Pierre thread `thr_ijspk7r9pv` authorized edits to its sibling `pierre-native`
checkout under `crates/pierre-view`, a new optional Rust binding crate,
`crates/pierre-runtime`, and root Cargo manifests/locks. The repository currently
has unrelated untracked source. Do not stage or commit that source. Return a
scoped changed-file list or patch. Pierre owns its TypeScript, website, test
scripts, and deployment files.

The owner requires one shared viewport implementation, preservation of native
document/input/diff/paint behavior and payloads, and an optional legacy adapter
for the current WebGPU/WebGL playground. The owner approved a Rust-only update
that pins legacy `gpuix-native` and the new binding to one full published
framework commit. Verify one GPUI crate in the resolved graph. Leave npm/release
pins and default JS entry points unchanged. Validate legacy native and WASM
composition builds plus the new native adapter before the handoff. Pierre will
then run the complete playground interaction suites. The new bridge itself
still has only macOS/Bun bootstrap support. No Pierre files have changed yet.

## Pierre shared viewport checkpoint

The owner approved the Rust extraction and the common published framework pin.
The implementation stays in the Pierre checkout. GPUiX contains only the
consumer integration probe. This audit records choices beyond that agreement.

| Decision | Alternative | Confidence | Failure case |
| --- | --- | --- | --- |
| Preserve Pierre's external document model and undo ownership. Native code owns IME preview, selection, scrolling, geometry, and paint. | Move committed editing and undo into Rust during the binding extraction. | Medium | Pierre cannot commit new text while its document-model worker is blocked. The standard bridge Input has a native buffer, but that guarantee does not transfer to this existing editor. The handoff states the gap. |
| Keep legacy patch validation, duplicate/base-version no-ops, and tolerant legacy view parsing. | Introduce a new strict document protocol. | Medium | A malformed or out-of-order patch can still be ignored. This is existing Pierre behavior, not an atomic edit protocol supplied by the new bridge. |
| Keep construction-only `initialSpec` as a convenience, with command-based source updates. Recommend a mount layout-effect command for large initial documents. | Put live source into props or add special initial-prop handling to the reconciler. | Medium | The bridge sends complete props on an update. Keeping a large `initialSpec` prop can resend it on a style change even though native code ignores it after creation. The preferred command example avoids this extra work. |
| Cancel an IME preview when its text source or session changes. Rebuild it when only source presentation changes. | Keep old row backups, rebase an arbitrary source edit through composition, or queue replacement documents. | Medium | This protects native source/row ownership, but does not cancel an OS candidate window. It is not a complete concurrent editor protocol. Failing tests captured same-session replacement, row patches, and an annotation insert during composition before the fix. |
| Keep the large update command inline in its enum. | Add another `Box` to silence Clippy's size warning. | Medium | A focus command has the enum's larger allocation size. The bridge already boxes decoded commands; another box adds an allocation on document updates. This has not been benchmarked. View Clippy reports this new warning and two existing style warnings; the composition passes strict Clippy. |
| Keep `snapshot()` as an explicit full-text query. | Retain another serialized snapshot or send full text on every event. | Medium | Frequent full-text polling costs a copy per query. Normal event payloads retain the existing small Pierre format. Queries do not force layout and can report state newer than paint. |
| Make the shared view the sole owner of the typed live `Spec`; the legacy adapter retains only a pending source plus declared view/patch props. | Retain the old adapter's second full typed specification. | High | The old renderer still retains its own generic declared props. This extraction removes the adapter's full typed copy, not all application or renderer storage. An ownership test checks that initial text and row allocations move into the view. |
| Implement `Render`, `Focusable`, and `EventEmitter` on the ordinary viewport. Put optional React trait implementations beside that type. | Add a wrapper entity and relay every event, or couple the default viewport to GPUiX. | High | Rust's orphan rule prevents a separate crate from directly implementing a foreign binding trait on a foreign viewport type. Features keep the default view free of renderer dependencies. The thin composition crate registers the optional implementation. |
| Keep legacy native/WASM and the new native composition separate, with legacy as the workspace default. | Replace default JS entries or require the new host in browsers. | High | A workspace-wide build can unify optional features and include more code. The documented build commands select each composition separately. The new bootstrap remains macOS/Bun only. |
| Pin all framework Rust dependencies to published `3dd67a230be63030e78332e874f9cec206a1f7c1`. | Mix the old renderer pin with a newer GPUI/binding graph. | High | GPUI types must come from one crate identity. Cargo metadata reports one GPUI package, and both composition builds pass. npm/release pins and default JS entries remain unchanged. |
| Use ordinary GPUI event subscriptions, with an optional painted-text observer for the legacy adapter. | Keep the old renderer callback and text registry imports in the shared viewport. | High | Event delivery now follows GPUI effects. Tests cover native payloads, sequence ordering, the old `change.value` envelope, and a queued new-host event at removal. The new canvas does not automatically join the bridge Document selection registry; Pierre keeps its editor selection behavior. |
| Parse colors with the same csscolorparser/clamp conversion used by the old core helper. | Depend on the old renderer for one color helper. | High | This preserves the existing sRGB paint conversion. It does not add new theme-token or HDR semantics to Pierre text paint. |
| Consume only default scroll when the viewport moves; retain propagation and honor consumption by a child. | Stop propagation on every wheel as before. | High | The offscreen test failed on the old code. The fixed viewport preserves ancestor callbacks, prevents double scrolling, and chains at the boundary through the existing GPUI API. No GPUI patch was needed. |
| Add the missing `unicode-linebreak` dependency before extracting source. | Treat the initial unresolved import as an extraction regression. | High | The saved baseline failed to compile without this dependency. The change is included in the scoped patch. |
| Add an opt-in native frame test component to the external composition. | Activate windows, change GPUI's occlusion behavior, or claim later paint from state-only queries. | High | Hidden and fully covered macOS windows stop display callbacks. The test component requests native draws every 16 ms and stops at unmount. It is absent from the default build and proves neither display FPS nor native frame scheduling while visible. |
| Separate logical focus checks from active-window focus events. Register the native observer before changing focus. | Require a hidden window to emit focus or subscribe after the event and expect replay. | High | Initial test assumptions were wrong. The source worker now checks logical focus; the offscreen native test sets logical activation without activating the OS window and checks the actual focus event. The legacy test enables the key listener that creates its focus handle. |
| Test both normal and frame-probe source/relocated executables. Leave complete playground interaction suites to the Pierre owner. | Change Pierre's TypeScript and run its integration migration here. | High | Native/WASM builds and focused probes do not prove every editor, WebGPU, and WebGL interaction. The agreed handoff keeps these tests explicit and leaves defaults unchanged. |
| Return a scoped patch and changed-file hashes without staging Pierre's untracked repository. | Make an initial commit that could absorb unrelated source. | High | The patch requires the saved source baseline. It applies cleanly to that baseline, and the owner receives the exact changed-file list. |

The 12 Rust tests and the offscreen GPU example pass. Legacy native and WASM
release builds pass; wasm-bindgen produces the browser module. The normal new
composition and the optional frame probe pass source and relocated executable
checks. The probe also verifies later document paint and annotation height.
Strict TypeScript checks pass. Full playground interaction suites remain with
Pierre, as agreed.

I stand behind this extraction and its stated limits. I do not claim that the
whole framework replacement is complete. The remaining geometry, event-race,
performance, and distribution work remains under the active goal. No Cherry
completion message has been sent.

### Restoring the external IME test helper

Pierre's complete interaction suite found that the extracted runtime lacked
`TestGpuixRenderer.simulateInputMethod`. The earlier consumer checkout had a
generic N-API helper in `test_renderer.rs`, beyond the GPUI take/restore methods
already retained here. The new direct Pierre probe first failed with the same
missing-method error.

| Decision | Alternative | Confidence | Failure case |
| --- | --- | --- | --- |
| Restore the exact generic helper contract, including optional UTF-16 selection offsets and the JSON `selected`/`marked` result. | Change Pierre's test semantics or add editor-specific simulation. | High | The installed native handler defines editing behavior. The helper stays test-only and does not activate windows. Its source came from the owner-identified prior checkout. |
| Test the helper on the built-in native input and on the external Pierre viewport. | Check only that the method exists. | High | The built-in test checks preedit, surrogate offsets, selection, commit, restoration, event delivery, and undo. The external probe checks the existing event envelope and document-model split. A missing focused handler remains an explicit error. |
| Keep native and JS production input paths unchanged. | Add an IME control API to the new application host. | High | This is a compatibility repair for an existing test API. The new bridge already tests its native input through the real GPUI platform handler. |

The generic release-build input probe and strict fixture TypeScript check pass.
The external composition must be rebuilt at the published helper commit before
Pierre resumes its blocked IME and edge suites. Both Rust adapters will retain
one common published framework pin. I stand behind this narrow compatibility
repair; it does not complete the remaining framework acceptance work.

### Native callback order and the previous frame

The new `event_frames.rs` regression dispatches native pointer input after a
complete `Host::apply` and before another draw. It checks the exact boundary
that was previously only described in the scenario review.

| Decision | Alternative | Confidence | Failure case |
| --- | --- | --- | --- |
| Keep ordinary GPUI mutable-view semantics and route callbacks by native effect order. | Retain callbacks and component descriptions for each painted frame. | Medium | After a commit, an old hitbox can generate an event whose listener reads newer native state and whose callback is newer. The API now states this. The binding does not promise a callback from the last displayed description. |
| Test both event-time state and the render-time value captured by a normal GPUI listener. | Assert only the subscription ID. | High | The test proves that the old hitbox remains in use before draw and that the new bounds apply after draw. A queued earlier event still keeps its old subscription. |
| Retire the old route before a replacement host can receive input. | Reuse the old host ID or transfer its pending frame callbacks. | High | A press on the removed view's old frame produces no event for the replacement. A new draw installs the replacement's distinct host ID and callback. |
| Exercise real GPUI dispatch inside a controlled App update without an automatic intervening test draw. | Infer the race from direct event emission alone. | High | This validates the input/commit/draw boundary, not physical OS presentation timing. No input runs inside `Host::apply`; it runs after the transaction returns. |

The complete binding test suite and strict all-target Clippy check pass. No
production code or GPUI change was needed. I stand behind this explicit event
contract. Geometry-dependent native components, broader performance comparisons,
and package distribution remain required work.

### IME repair validation at the published pin

Both Pierre Rust adapters now pin published framework
`af4ee6d7682d922334eb92291b7ec92bc5251ddf`; GPUI remains
`feda54e61a9469cf484c387c341382d3172cecb6`. Cargo still resolves one GPUI crate.
The legacy native build and external IME probe pass. The normal new composition
passes its source and relocated worker probes. The legacy WASM build and
wasm-bindgen output also pass at this pin. No npm or JS default entry changed.

Pierre separately reports that its initial 636 unit/parity tests, both full
WebGPU/WebGL browser suites, native editing/layout/UI/comment/search/marker
suites, collection editing, and legacy application source/relocated worker
checks pass. Its remaining IME and edge suites can now use the restored helper.
This records the owner's report; it does not claim those final suites have
already run against the new helper artifact.
