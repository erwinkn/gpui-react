---
"@gpuix/native": patch
"@gpuix/react": patch
---

Keep wheel callbacks active when native divs or lists scroll. Consume only the
default scroll action when an inner container moves. Allow the next event to
scroll its parent when the inner container is at its boundary.

Add process-local macOS font smoothing control and test-only access to the
installed platform input handler. Neither change modifies OS preferences.

The scroll failure came from this fork's propagation change. A search of
`zed-industries/zed` issues and pull requests for `prevent_default scroll`
found no matching upstream fix.
