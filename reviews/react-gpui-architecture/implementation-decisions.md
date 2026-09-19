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
