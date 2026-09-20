# The standard kit for gpui-react

This crate contains the five standard controls for the
[`gpui-react`](../gpui-react/README.md) engine: `Container`, `Text`,
`VirtualList`, `Document`, and `Input`, together with their `Style`, the
painted `geometry` types, `document_text`, and the visual examples. It is
opinionated by design: camelCase props, numbers as pixels, CSS colors, and a
curated event surface. The engine has none of these opinions.

`register_kit(&mut registry)` declares `Style` as the session's shared value
type (`registry.shared::<Style>()`) and registers `document`, `list`,
`container`, `text`, and `input` with their capabilities. The default
composition in [`gpui-react-runtime`](../gpui-react-runtime/README.md) calls
it before the application's own registrations; a composition that builds its
own `Registry` calls it the same way, and one that wants none of the standard
controls does not depend on this crate.

```rust,ignore
let mut registry = gpui_react::Registry::default();
gpui_react_kit::register_kit(&mut registry)?;
registry.register(Component::<Counter>::new("counter").events())?;
```

Props that carry a style are typed `Shared<Style>` (`SharedStyle` is that
alias); equal styles are defined once on the wire and shared by id. A native
component outside the kit that wants the kit's styling declares
`pub style: SharedStyle` in its props and reads it through `Deref`. Text that
should take part in `Document` selection and search renders through
`document_text(key, text)`.

The native behavior of every control, the test scenarios, and the Document
text protocol are documented in [CONTROLS.md](./CONTROLS.md). The JavaScript
side is [`@gpui-react/kit`](../../packages/kit/README.md). The input editor's
third-party notice is in [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md).

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo test -p gpui-react-kit --release
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo run -p gpui-react-kit --example document_visual
```

Every example opens its window with `focus: false`, stays in the background,
and saves a PNG under `/tmp`.
