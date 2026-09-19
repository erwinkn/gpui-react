# Native application host

This crate supplies the macOS AppKit loop and N-API worker channel for
`gpui-react`. It has no dependency on the previous `gpuix-native` renderer.
The worker retains only a session handle and queued typed transactions. It does
not retain a Rust description tree or live GPUI objects.

A composition crate re-exports this crate's N-API methods and calls
`register_components` from its module initializer. The function it registers
fills a new `gpui-react::Registry` for each native host. Its ordinary GPUI
component crates use the GPUI re-export so all types share one framework build.
The [counter composition](../../fixtures/bridge-counter/README.md) is the
first complete example.

The exported methods are `bridgeRuntimeVersion`, `NativeHost` with `id`, `run`
and `close`, and `NativeClient` with `send`, `receive` and `close`. Use the
JavaScript helpers in `@gpuix/bridge/application` for normal application entry.
They require an explicit composition object in both host and worker. No default
GPUiX binary is loaded.

The native input queue allows 256 transactions and 4 MiB. Output allows 4096
messages and 4 MiB. Overflow is explicit. The UI processes up to 32 transactions
or four milliseconds before yielding; one atomic transaction can exceed that
budget. JSON is decoded once on worker admission. Typed component props and
topology are validated on UI before application. Native events are delivered
before acknowledgements that retire their callback subscriptions.

Windows remain inactive. The current test evidence covers source and compiled
Bun workers, relocation, repeated host startup, missing/failed workers, malformed
component props, and native executor progress during a blocked worker. Signal
handling and complete distribution are still required before release. The
counter fixture's optional `interaction-tests` build also checks native typing,
selection, undo, IME, scroll, hover, caret and animated pixels during a blocked
worker. Its source and relocated executable use the same production channel.
Test-only native dispatch and image capture stay in the fixture.

```sh
cargo test --manifest-path crates/gpui-react-host/Cargo.toml --release
cargo clippy --manifest-path crates/gpui-react-host/Cargo.toml --release --all-targets -- -D warnings
```
