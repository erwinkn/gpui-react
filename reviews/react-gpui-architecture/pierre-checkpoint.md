# Pierre shared viewport checkpoint

The ordinary GPUI viewport and both optional adapters are implemented in the
Pierre checkout. GPUiX contains the external test fixture only. The default
Pierre native/WASM composition and its JS entry points remain unchanged.

The tested framework pin is
`3dd67a230be63030e78332e874f9cec206a1f7c1`, with GPUI
`feda54e61a9469cf484c387c341382d3172cecb6`. Cargo resolves one GPUI package for
the shared view and both adapters. The pure viewport has no renderer dependency.
The new native composition has no `gpuix-native` dependency when built alone.

The Rust extraction removes the adapter's second full typed `Spec`. The view
implements normal GPUI `Render`, `Focusable`, and `EventEmitter` interfaces.
Optional bridge traits live beside the type; the new composition registers them.
Source and view updates use the existing payloads. The legacy adapter preserves
its `change.value` JSON envelope. The new adapter sends the event object directly.

Tests found and fixed scroll propagation loss and stale IME row backups during
source changes. Twelve Rust tests pass, plus the offscreen GPU example for
platform input, IME, focus, nested scrolling, and split-diff pixels. Both native
compositions and the legacy WASM composition build in release mode. The generated
WASM module preserves the existing browser exports and shared-memory setup.

The [integration probe](../../fixtures/bridge-pierre/README.md) passes with source
workers and relocated Bun executables. An optional external test component forces
native draws for hidden windows and verifies later document paint and annotation
resize. It is absent from the default composition. These checks do not certify
display frame rate or replace the complete Pierre playground interaction suites.

The external document model still owns committed edits and undo. This extraction
does not make committed typing independent of that model. Native IME preview,
selection, scroll, geometry, and paint remain in the viewport. For a large initial
source, use an update command from a mount layout effect; keeping it in
`initialSpec` can resend it on unrelated prop changes.

The lockfile changes 26 framework/submodule package source refs from `08ffd4b`
to `3dd67a2`. It adds `gpui-react`, `gpui-react-controls`, `gpui-react-host`,
`pierre-react-runtime`, and `signal-hook` 0.3.18. It removes no package and
upgrades no existing registry package. The source pin and adapter dependency
edges account for the remaining changes.

The scoped patch and file hashes are in the thread's
[handoff folder](/Users/erwin/.bb-machines/erwin.getbb.app/thread-storage/thr_b6hiac9fv5/handoffs/pierre-bridge).
They cover root Cargo files, the viewport crate, the optional composition,
and the legacy runtime's feature selection. No Pierre TypeScript, npm/release
pins, website, scripts, or deployment files were edited or staged.
See the [decision audit](./implementation-decisions.md#pierre-shared-viewport-checkpoint)
for choices, test corrections, and limits.
