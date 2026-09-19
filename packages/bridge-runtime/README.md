# Default bridge runtime

This package provides the native host and standard controls for `@gpuix/bridge`.
The tested target is macOS arm64 with Bun. The window starts inactive. The
runtime contains no Cherry, Pierre, GPU effect, or test-only components.

Install matching versions of `@gpuix/bridge`, `@gpuix/bridge-controls`, and
`@gpuix/bridge-runtime`, plus React 19.2. Use the release archives listed in the
framework release manifest until these packages have an npm release.

```ts
// host.ts
import bindings from "@gpuix/bridge-runtime"
import { runApplication } from "@gpuix/bridge/application"
await runApplication(bindings, new URL("./worker.tsx", import.meta.url), {
  title: "My app", width: 800, height: 600,
})
```

```tsx
// worker.tsx
import bindings from "@gpuix/bridge-runtime"
import { attachApplication } from "@gpuix/bridge/application"
import { Container, Input, Text } from "@gpuix/bridge-controls"

const root = attachApplication(bindings)
root.render(<Container style={{ padding: 16, gap: 12 }}>
  <Text text="Native GPUI" />
  <Input initialValue="Hello" label="Message" />
</Container>)
```

Run `bun host.ts`. For a standalone executable, include both entries:

```sh
bun build --compile host.ts worker.tsx --outfile my-app
```

The loader uses one literal native asset path, including in a compiled program.
The default export and named exports expose the same `Bindings` interface.
CommonJS `require('@gpuix/bridge-runtime')` returns those bindings directly.
The bridge verifies the native protocol before it starts an application.

To add a native component, build a composition that registers it with
`gpui-react-host`. Pass that composition to the same launcher and worker helpers
in place of this package. Do not load two compositions into one application.
See the [composition guide](https://github.com/erwinkn/gpuix/blob/bridge/minimal-react-gpui/crates/gpui-react-host/README.md).

The browser export fails explicitly. The existing `@gpuix/react` browser path
is separate and remains available. No new browser driver is included here.
