# React wrappers for the standard gpui-react controls

`@gpui-react/kit` exports the typed React wrappers, refs, events, and the
`Style` type for the five standard controls: `Container`, `Text`, `List`,
`Document`, and `Input`. They are ordinary `nativeComponent` wrappers over
[`@gpui-react/core`](../core/README.md), which is a peer dependency together
with React 19. The native side is the
[`gpui-react-kit`](../../crates/gpui-react-kit/README.md) crate, which the
default [`@gpui-react/runtime`](../runtime/README.md) package registers; a
custom composition registers it through `gpui_react_kit::register_kit`.

```tsx
import bindings from "@gpui-react/runtime"
import { attachApplication } from "@gpui-react/core/application"
import { Container, Input, Text } from "@gpui-react/kit"

const root = attachApplication(bindings)
root.render(<Container style={{ padding: 16, gap: 12 }}>
  <Text text="Native GPUI" />
  <Input initialValue="Hello" label="Message" />
</Container>)
```

`Text` also accepts a string or number child and sends it as its `text` prop.
String children placed directly under any other element render through the
root's `textKind`, which defaults to the kit's `text` control. A `style`
object is sent once as a shared definition and referenced by id afterwards;
the kit's `Style` is the shared type of a session that registers the kit.

The full prop, event, command, query, and style contract is in
[CONTROLS.md](./CONTROLS.md).

```sh
bun run --cwd packages/kit build
bun run --cwd packages/kit test
```
