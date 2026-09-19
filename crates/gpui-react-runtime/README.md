# Native application runtime

This crate supplies the macOS AppKit loop and the N-API worker channel for
`gpui-react`, together with the default composition. Its `registry()` always
registers the five built-in controls (`container`, `text`, `input`, `list`,
`document`) through `gpui_react::register_builtins`, then every component added
through `register_components`. Custom applications register only their own
components.

The crate builds both an `rlib` for composition crates and the `cdylib`
packaged as [`@gpui-react/runtime`](../../packages/runtime/README.md).

```sh
cargo build --release --locked --manifest-path crates/gpui-react-runtime/Cargo.toml
```

Applications with custom native components build their own `cdylib` that
depends on `gpui-react` and `gpui-react-runtime`, registers its components in
one `#[napi_derive::module_init]`, and points `host.ts` at its own `.node` file.
Follow the [counter composition](../../fixtures/counter/README.md). Load that
one composition in both launcher and worker; the built-in controls are always
present, so the composition adds only its own kinds.