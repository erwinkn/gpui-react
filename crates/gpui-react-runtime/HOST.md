# Native application host

This crate supplies the macOS AppKit loop and N-API worker channel for
`gpui-react`.
The worker retains only a session handle and queued typed transactions. It does
not retain a Rust description tree or live GPUI objects.

The host is a Rust library inside `gpui-react-runtime`, which also builds the
packaged `cdylib`. A custom application composition emits its own `cdylib` and
runs `napi_build::setup()` in its build script. This avoids an unqualified
layout overwriting another feature graph in a shared Cargo target directory.
Run `bun scripts/check-builds.ts` from the repository
root to alternate the counter composition and the default runtime builds in one
directory. Set `CARGO_TARGET_DIR` to reuse an existing release target.

A composition crate re-exports this crate's N-API methods and calls
`register_components` from its module initializer. The function it registers
fills a new `gpui-react::Registry` for each native host. Its ordinary GPUI
component crates use the GPUI re-export so all types share one framework build.
The [counter composition](../../fixtures/counter/README.md) is the
first complete example.

The exported methods are `bridgeRuntimeVersion`, `NativeHost` with `id`, `run`
and `close`, and `NativeClient` with `send`, `receive` and `close`. Use the
JavaScript helpers in `@gpui-react/core/application` for normal application entry.
They require an explicit composition object in both host and worker.

The default [`gpui-react-runtime`](../gpui-react-runtime/README.md) composition
registers the standard controls and is packaged as `@gpui-react/runtime`.
Applications can pass that package to both JS helpers. Custom native views
still require an application composition in its place.

The native input queue allows 256 transactions and 4 MiB. Output allows 4096
messages and 4 MiB. Overflow is explicit. The UI processes up to 32 transactions
or four milliseconds before yielding; one atomic transaction can exceed that
budget. JSON is decoded once on worker admission. Typed component props and
topology are validated on UI before application. Native events are delivered
before acknowledgements that retire their callback subscriptions.

Windows remain inactive. The current test evidence covers source and compiled
Bun workers, relocation, repeated host startup, missing/failed workers, malformed
component props, and native executor progress during a blocked worker. Signal
and shutdown tests also cover the cases below. Complete distribution remains
required before release. The
counter fixture's optional `interaction-tests` build also checks native typing,
selection, undo, IME, scroll, hover, caret and animated pixels during a blocked
worker. Its source and relocated executable use the same production channel.
Test-only native dispatch and image capture stay in the fixture.

`SIGINT` and `SIGTERM` request native shutdown while `NativeHost::run` owns the
main thread. A native signal reader wakes the host without a JavaScript callback.
The launcher gives a responsive worker time to run React effect and exit cleanup.
After two seconds it terminates a blocked worker, which cannot run that cleanup.
Native `unmounting` and owned resource destruction still run. Signals use their
default process behavior after the host ends; per-signal JS handlers are not
forwarded by this API. A signal that requests graceful shutdown lets
`runApplication` return normally.

The host registers GPUI's `Window::on_close` callback. It clears the mounted
native components while the window and app remain available, on both direct
window removal and application shutdown. Waiting until the native loop returns
is too late because GPUI has already destroyed those windows. Worker failure
and event-queue overflow use the same cleanup path. Overflow remains an explicit
failure, rather than a graceful result or silent loss of events.

```sh
cargo test --manifest-path crates/gpui-react-runtime/Cargo.toml --release
cargo clippy --manifest-path crates/gpui-react-runtime/Cargo.toml --release --all-targets -- -D warnings
```
