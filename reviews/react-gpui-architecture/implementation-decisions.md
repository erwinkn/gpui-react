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
