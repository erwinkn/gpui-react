# Default native composition

This crate combines `gpui-react-host` and `gpui-react-controls` in one native
library. It registers `container`, `text`, `input`, `list`, and `document`.
It contains no consumer components or test driver. The JavaScript package is
[`@gpuix/bridge-runtime`](../../packages/bridge-runtime/README.md).

```sh
cargo build --release --locked --manifest-path crates/gpui-react-runtime/Cargo.toml
```

Applications with custom native components build a composition that registers
those components and the controls they use. Follow the
[counter composition](../../fixtures/bridge-counter/README.md). Load that one
composition in both launcher and worker; do not also load the default runtime.
