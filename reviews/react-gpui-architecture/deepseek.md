# React bindings on GPUI: an independent architecture review (revised)

**Reviewer model:** DeepSeek V4.1 Flash. **Basis:** GPUiX checkout `08ffd4bf`, GPUI `bea32f07` (submodule on `cherry/native-extensions`), React Native New-Architecture docs. Read-only review; no repo source was modified.

## Recommendation

Build React on **one immutable, structurally-shared native shadow tree**, produced on the application (worker) thread and handed to the UI thread **by shared `Arc` identity rather than a serialized mutation batch**, while the UI thread owns a separate **instance registry** of GPUI state (`FocusHandle`, `ScrollHandle`, `ListState`, text-editor/IME entities, motion state, custom-element adapters, effect resources) keyed by stable host id. The worker is the sole authority for *descriptions*; the UI thread is the sole authority for *realized interactive state, layout, and presentation*. This is option B done properly combined with A's instance ownership, and it is preferable to plain A primarily because A still applies mutations (hash-inserts, dirty walks, style interning) to a UI-thread tree at the moment input must stay responsive. But the advantage is a **hypothesis with a specific cost model**, not a categorical win; see the corrected memory section and prototypes.

## Correction note (this revision)

1. **Synchronous layout effects are not currently implementable as promised.** `present()` is private (`window.rs:3086`); `present_if_needed` is `#[cfg(any(feature = "bench", all(test, feature = "profiler")))]` (`window.rs:3116`); and the frame callback runs `window.draw(cx); window.present();` inseparably (`app.rs`/`window.rs:1636-1650`). `draw` is not a pure layout probe: it swaps `rendered_frame`, installs the input handler, fires focus listeners, and sets `needs_present` (`window.rs:3048`, `2920-3060`). The `measure(epoch)` round-trip is therefore **not a supported lifecycle today**. I now state the semantic gap and give two concrete options (below) instead of promising Fabric-style effects.
2. **The latest-root slot plus an independently queued `commit(epoch)` is unsafe.** I add revision barriers, a mounted-base rule, id-lifetime rules, and ordered-command validation.
3. **Controlled inputs need edit versions and explicit replacement intent**, not a last-declared-string comparison.
4. **Memory is not categorically reduced.** I remove that claim and separate structural sharing from its actual overheads; I also correct my misuse of the historical 626 ms `applyBatch` measurement and reframe A vs C around the underlying models, not JSON-vs-handles.
5. **One per-node version was wrong.** I keep separate semantic versions for render vs. search so the documented query/cursor cache invariant survives. The "elements can never move" and "browser has no threads" statements are qualified to current build contracts, and GPU resources are described as backend-owned.

## Verified constraints (qualified to the current build)

- **Element construction is draw-time and arena-bound.** `AnyElement` is `ArenaBox<dyn ElementObject>` allocated through `with_element_arena`, which uses the active `App` arena during draw (`element.rs:588`, `arena.rs:213`, `window.rs:356`). In this GPUI build, constructing GPUI elements off the draw thread is unsupported. (Not a law: a future GPUI could expose a separate owned element representation.)
- **Layout is `Window`-bound.** `TaffyLayoutEngine` lives in `Window.layout_engine`; `request_measured_layout` takes `Fn(.., &mut Window, &mut App)`; `Window::compute_layout` takes the engine out and passes `self, cx` (`window.rs:4870-4936`). Arbitrary layout cannot run on a worker without a GPUI change.
- **Text shaping, syntax, and font registration are window-free** and currently run on the worker (`host_runtime.rs:484-500`; `TextSystem` uses internal locks, `text_system.rs:51`).
- **The same JSON batch is parsed twice today** (`NativeClient::prepare_batch` → worker `staging`; `GpuixRenderer::apply_batch` → UI tree; both `apply_batch_to_tree`, `renderer.rs:6303`). This is a protocol property, not a necessary one.
- **Worker and host share one loaded-library instance**: `NativeClient::new` upgrades a `Weak<Session>` from the process-global `SESSIONS` map populated by `NativeHost` (`host_runtime.rs:18,248,419`). This is what could make an `Arc` handoff physically possible; it must be re-proven on Node `worker_threads`.
- **Host element ids are monotonic within a renderer session** (`nextId` = `++container.ids.nextElementId`, `host-config.ts`), so an id is never reused after destruction. This is load-bearing for command/event staleness rules.
- **The host never waits on JS**: bounded command slices, a bounded event queue, explicit terminal error on overflow (`host_runtime.rs`).
- **GPU resources are renderer/backend-owned.** `Window::gpu_context()` yields device/queue, and custom effects own their textures, but resource-creation threading rules are backend-specific. The embedded macOS path currently exercises them on the UI thread; I no longer generalize that to all backends.
- **Browser:** the current wasm-bindgen binding runs in one JS context. Earlier phrasing ("no threads") was too broad; the accurate statement is that the *current browser build* has no separate React worker.

## Ownership and data model

| Layer | Owner | Lifetime | Contents |
|---|---|---|---|
| React fibers | Worker JS | React | state, props, closures |
| Persistent revision | Worker Rust, `Arc`-shared | until both threads drop it | `kind`, `Arc<StyleDesc>`, `Arc<str>` text, `Arc<[EventKind]>`, props, `Arc<[Arc<Node>]>` children, `render_version`, `search_version` |
| Worker parent index | Worker Rust | transient per commit | `HashMap<u64, ParentLink>` — topology only, no prop copies |
| Instance registry | UI thread | across revisions, keyed by host id | focus/scroll handles, `ListState`, editor entity + `edit_seq`, motion, adapter, effect resources |
| Mounted base | UI thread | last painted revision | `Arc<Node>` plus `HashMap<u64, Arc<Node>>` last-seen (for `Arc::ptr_eq` prop diffing) |
| Command/event queues | both | bounded | ordered commands; FIFO events |

**Two versions per node, deliberately not merged.**
- `render_version`: bumped by any structure, style, or prop change that can repaint.
- `search_version`: bumped only by text/content/structure changes and by the *appearance or disappearance* of a nested `highlight` declaration; **not** by style, `activeIndex`, colours, or query text.

This preserves the README/AGENTS invariant that a find-bar keystroke or cursor move must not re-walk and re-fold content groups: the group cache keys on `search_version`; the match cache additionally keys on `matcher_hash()`. Path-copying propagates the two versions independently, exactly as the current `mark_changed`/`mark_render_changed` distinction does. Merging them would reintroduce the cache-miss regression the invariant exists to prevent.

## Commit, command, and event trace with revision barriers

The shared slot always holds a **complete** revision, so the UI can jump straight to the newest one; there is no incremental patch to lose.

1. React renders on the worker; host-config callbacks queue the batch; placement callbacks materialize accepted subtrees.
2. `resetAfterCommit` → worker applies the batch to its persistent tree, producing `R+1` (path-copy from changed ids to root). On failure, `R` stays published; nothing crosses.
3. Worker **publishes the full `R+1` root into the slot** and enqueues a `commit` command `{epoch: R+1, base_epoch: R}`.
4. UI slice: reads the slot; if it conflicts with its own pending work it still installs the newest root. It records `mounted_base = R+1` **only after the revision is actually used to build a frame**.
5. Interactive commands (`focusElement`, `scrollTo`, `scrollToItem`, controlled-value commit, motion cancel) carry `{authored_epoch, host_id, intent}`. A command is applied only if `host_id` exists in `mounted_base`. If not, it is **dropped as stale, not applied to a later id** — safe because ids are monotonic and never reused.
6. Domain events flow UI→worker FIFO with the host id. The worker drops an event whose id has no live handler (current behavior).

**The A@R1 → focus(A)@R1 → remove(A)@R2 race.** With base-checked commands: the UI installs R2; `mounted_base` no longer contains A; the queued `focus(A)` fails the base check and is dropped. The worker already removed A's handlers when it applied the destroy synchronously to its own tree, so no stale handler survives. The UI prunes A's instance by **absence from the new revision**, not by trusting a destroyed-id list. `destroyed_ids` remain a worker-only concern (JS handler cleanup).

**Absence-based pruning is the rule; destroyed-id lists are an optimization.** A UI that jumps R0→R2 never saw A mount, so a destroy-list-driven prune would be a no-op or an error; revision diffing is correct.

## Controlled inputs, normalization, and IME

Comparing only the last declared string is insufficient: two distinct edits can produce the same string, and normalization changes it. Required protocol:

- The native editor owns `value` and `edit_seq: u64`, incremented on **every** user-initiated edit, including composition updates.
- A change event carries `(value, edit_seq)`. The app derives its declared value from a specific `edit_seq` and commits `declared(value, derived_from_edit: edit_seq, replace: bool)`.
- The editor applies a declared value only if it is **not composing**, and either (a) `replace == true` (explicit replacement intent: normalize, clamp, reset), or (b) `derived_from_edit >= edit_seq_at_emit` — i.e. the declaration is not an echo of a superseded edit. Otherwise it queues the declaration until composition ends or the app sends `replace: true`.

Trace: native edit1 → `"a"`, `edit_seq=1`, emit. Native edit2 → `"ab"`, `edit_seq=2`, emit. React processes edit1 late and commits `declared("a", derived_from=1)`. Editor is at `edit_seq=2`, `replace=false` → **drop the stale echo**. App normalizes edit2 to `"AB"` and commits `declared("AB", replace=true)` → applied; programmatic application bumps `edit_seq` to 3 and emits no change event. During IME composition, non-replacement declarations queue; `replace=true` may abort composition only when the app explicitly asks.

Without this, an IME composition is clobbered by a round-tripped echo. This is the same class of problem Fabric solves with native-originated state updates that bypass React; the protocol above is the GPUiX equivalent and needs a prototype to validate against real IME sequences.

## Layout effects: an explicit semantic gap

A `useLayoutEffect` that measures a just-committed element and applies a correction **cannot** be made synchronous in the current GPUI API. `draw` is not a probe, and `present` is not separable. Two honest options:

- **Supported today (recommended default): adopt an `onLayout`-style post-paint callback.** The UI emits measured bounds after a frame is presented, tagged with the revision epoch. React applies corrections on the next commit. This is exactly the legacy-RN `onLayout` model: one frame of possible intermediate state, no false promise. GPUiX already has the pieces (`host_after_paint`, bounds registry, epoch tagging).
- **Proposed GPUI change, if true synchronous-correction UX is a hard requirement:** expose a supported draw/present split (`Window::draw_deferred` setting `needs_present`, with presentation deferred to the next platform tick, plus an `on_presented` hook). This is a bounded GPUI change, but it has a real hazard: `draw` swaps `rendered_frame` and hit-testing uses it, so a drawn-but-not-presented frame makes hit-testing disagree with visible pixels during the wait for arbitrary JS. Suppressing intermediate paint while native interaction continues therefore requires either committing to that mismatch for the duration of the measurement, or deferring hit-test application too — both are semantic decisions that must be made in GPUI, not glossed in the binding. I recommend not blocking the architecture on this; ship `onLayout` first and add the GPUI split only if the UX demands it.

## Scheduling, overflow, and failure

- **Revisions:** last-wins, may skip intermediate *visual* states (Fabric permits this). Dropping an intermediate revision is safe only because every revision is complete, and because worker-side `destroyed_ids` cleanup is applied synchronously on the worker, not derived from the visual channel.
- **Commands:** ordered and never silently coalesced. They are bounded (`MAX_COMMANDS = 256`, 4 MiB) and validated (`NativeClient::send`). If the worker cannot enqueue or drain, the request fails explicitly; the worker must `await whenIdle()`.
- **Events:** bounded (`MAX_EVENTS = 4096`, 4 MiB). On overflow the session **terminates with an explicit reason** (`host_runtime.rs`). "Never drop events" is impossible with bounded queues and an indefinitely blocked worker; the correct guarantee is **never silently drop — bounded, with an explicit terminal failure the application can observe**.
- **Error recovery:** a failed commit never publishes; a failed command returns an error for that command only; a worker crash closes the session; a blocked worker serves no new content by design.

## Extension contract

Keep the compiled, statically-composed extension model and hyphenated namespaces. Change three things: (1) props become immutable per revision, so `set_prop` becomes a diff over `(props, changed_keys)` from `Arc::ptr_eq`/version comparison; (2) generate typed prop structs and TS types from one schema per element, removing the "type-checks, serializes, drops" failure mode; (3) make instance/effect ownership explicit — `render` is pure over `(props, constraints, instance state, now)`, entities are released in `destroy`, and effect producers own cancellation and reduced-motion. Add the missing composed-build fingerprint (`gpuix` commit, `gpui` commit, extension API version) enforced identically by host and worker; its absence is the largest maintainability hazard in this story.

## Cost model and memory, without categorical claims

Structural sharing removes duplicated **props/content** for unchanged subtrees. It adds, and I no longer hide: retained old roots until dropped, path-copied nodes per commit, a worker parent index, wide `children` arrays rebuilt on structural change, per-node callbacks/handles, and JS-side handles. Win-window whether `children` arrays stay small depends on the application using React windowing (`<virtual-list>` with `windowStart`/`itemCount`); a non-virtualized 10k-row map has wide arrays and makes path copying expensive.

The AGENTS 626 ms `applyBatch` figure is a **historical diagnosis of the single-threaded path** (the fix was queueing raw objects instead of pre-stringifying). It is not a measurement of the current worker+UI cost, and I withdraw using it as evidence.

The A-vs-C comparison is therefore not "JSON vs handles." Option A can receive a worker-validated, typed/binary op buffer and avoid UI JSON parsing entirely; its remaining cost is applying the mutation to a mutable UI tree (map inserts, dirty walks, style interning) plus revision bookkeeping. Option C's UI cost is an `Arc` swap plus instance reconciliation/diffing. Both must still build GPUI elements. Whether C wins depends on which of those UI-thread costs dominates, and on path-copy overhead — a measurement, not an axiom.

## Risks, decisive prototypes, and what remains untested

**Strongest objection (unchanged in force):** Option A plus a JS host-node map may be simpler than a persistent tree, and the UI-thread mutation cost may be small relative to element build. If so, C is not worth the machinery.

**Falsifiable prototypes:**
1. UI-thread commit cost for a 10k-turn chat: mutable apply (typed batch) vs `Arc` swap + build.
2. Memory/RSS: two mutable trees vs one persistent tree + parent index, under one-row-per-frame scroll and full remount, **including a non-virtualized wide-children case**.
3. `onLayout`-vs-deferred-present: does the post-paint callback suffice for real measurement UX, and does a GPUI draw/present split cause hit-test/pixel disagreement under input during the wait?
4. Id-lifetime race: scripted A-create/A-focus/A-remove across epochs; assert no stale focus applied and no handler leak.
5. Controlled-input/IME: edit1-then-edit2 with delayed echo, normalization, and a real composition sequence.
6. Static sharing: worker and host observe the same slot in Bun and Node `worker_threads`; abort the `Arc` handoff otherwise.
7. Overload: commit every 1 ms while painting at 16 ms, with a blocked worker; assert explicit overflow, bounded memory, and no silent loss.

**Remains untested here:** all of the above (no builds or apps were run), real IME behavior, per-backend GPU resource threading, GPUI's ability to defer presentation safely without hit-test drift, and whether Node (as opposed to Bun) shares native statics across worker threads.

## Conclusion

**Recommendation:** one immutable, structurally-shared worker-owned shadow tree handed to the UI thread by `Arc` identity, with a UI-thread instance registry; no mutation batch on the UI thread; ordered, base-validated commands; bounded queues with explicit terminal overflow; separate `render_version`/`search_version`; and `onLayout`-style post-paint measurement as the supported path, with a GPUI draw/present split only as a proposed change if synchronous correction becomes a hard requirement.

**Most important correction:** the earlier `measure(epoch)` synchronous-layout-effect promise is withdrawn. GPUI cannot currently separate draw from present, and `draw` mutates frame, input, and focus state; the honest default is post-paint `onLayout`, with the semantic gap stated rather than papered over.

**Model identity:** DeepSeek V4.1 Flash. **Sources inspected:** GPUiX `packages/react/src/{application.ts,reconciler/{host-config,batch-renderer,event-registry,renderer}.ts}`, `packages/native/src/{retained_tree.rs,renderer.rs,renderer/host_runtime.rs,extension.rs,custom_elements/mod.rs,style.rs}`, README architecture/extension/search sections; GPUI `element.rs`, `arena.rs`, `window.rs` (`draw`, `draw_roots`, `present`, `present_if_needed`, `request_layout`, `compute_layout`, element arena), `app.rs` frame-effect/presentation path, `taffy.rs`, `text_system.rs`; React Native New-Architecture docs (render-pipeline, threading-model, fabric-renderer, landing-page).
