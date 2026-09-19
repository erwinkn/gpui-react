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


## IME test API follow-up

The broader Pierre suites found a missing generic test helper from its previous
runtime. Published framework commit `af4ee6d7682d922334eb92291b7ec92bc5251ddf`
restores `TestGpuixRenderer.simulateInputMethod` with its original contract.
Both Pierre Rust adapters now use that common pin. GPUI is unchanged.

The built-in input probe covers preedit, UTF-16 selection, commit, restoration,
undo, and read-only input. The rebuilt legacy Pierre probe also passes its IME
checks. The new composition's source/relocated probes and the legacy WASM build
plus wasm-bindgen pass at the updated pin. Updated artifacts and checksums are
in the same handoff folder under names ending in `af4ee6d`.

## Counted-click test API and complete native suites

Published framework `5f5de7b61a57a44b4e13466068d5cc056de3a9a2` restores the
optional fifth click-count argument on both mouse-down and mouse-up events.
Both compositions build at this pin, with one GPUI crate and unchanged GPUI
ref. The direct legacy probe and the new source/relocated probes pass. The
legacy WASM build and wasm-bindgen pass. Current artifacts and SHA256 hashes
are in the handoff folder under names ending in `5f5de7b`.

The fixture's word-selection assertion now uses the native deletions side.
The additions side sends the click count to the external JS document model;
a fixture without that model cannot expect it to perform word selection.
No editor behavior changed to satisfy this test.

The Pierre owner reports that all seven default native editor/playground suites,
636 unit/parity tests, source/relocated app worker tests, and installed-library
native rendering/typing pass. The owner published the source, including the
shared viewport and optional adapter, at Pierre commit
`fe426a371b1edd20f15237070f2d67c42ec78f33`. All six Rust pins use `5f5de7b`.
The npm pins and default JS path remain unchanged. The owner also reports full
local and preview WebGPU/WebGL success using the `af4ee6d` WASM artifact. CI is
rebuilding at `5f5de7b`; those browser results are not yet claimed for that pin.

The Pierre owner subsequently verified the `5f5de7b` WASM SHA256 and passed both
complete local browser suites on actual WebGPU and WebGL. Native and browser
artifacts now match the same source pin. Clean CI builds native/WASM and passes
the four editor suites. A playground assertion differed on the CI display's
viewport and pixel scale; the owner is fixing test coordinate conversion and
explicit viewport assertions. No framework resize behavior or consumer Rust
pin is being changed for that test setup issue.
