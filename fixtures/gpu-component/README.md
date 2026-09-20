# External GPU component

This fixture is an ordinary GPUI `TextureView` with a small React binding. It
depends on `gpui-react` and the `gpui-react-kit` standard controls. The application composition
registers it in the same runtime as its other components.

The producer obtains the window's Metal device and command queue. It creates
an RGBA16Float texture, clears it through a GPU render command, and submits that
command on the shared queue. Its canvas then calls `Window::paint_gpu_texture`.
There is no CPU pixel upload or wait in the producer. Screenshot tests perform
readback separately.

Colors are premultiplied and use the render target's color space. RGB can exceed
alpha. For example, `[2, 0, 0, 0.25]` retains its bright red contribution after
parent opacity and blending. GPUI applies inherited opacity, clipping, corner
coverage, and normal scene ordering. See the [texture contract](../../docs/gpu-textures.md).

Native code can use the view directly:

```rust
let effect = cx.new(|cx| TextureView::new(TextureProps {
    initial_color: [2., 0., 0., 0.25],
    ..Default::default()
}, cx));
// Give it dimensions through props or its normal GPUI layout parent.
div().child(effect)
```

`new`, `update_props`, `apply_command`, and `snapshot` are normal native methods.
The optional bridge capabilities delegate to those methods. `register` adds
`example-texture` to a `gpui_react::Registry`. The matching React wrapper is in
`react.ts`:

```tsx
import { createRef } from "react"
import { Texture, type TextureRef } from "@gpui-react/gpu-example"

const texture = createRef<TextureRef>()
root.render(<Texture ref={texture} initialColor={[2, 0, 0, 0.25]}
  radius={8} style={{ width: 64, height: 48, opacity: 0.25 }} />)

// After mounting:
await texture.current!.command({ type: "transition", to: [0, 1, 0, 1], durationMs: 300 })
await texture.current!.command({ type: "cancel" })
```

The fixture package is private. It is a worked component example, not a released
standard control or a general shader language.

| Prop | Behavior |
| --- | --- |
| `initialColor` | Construction-only RGBA value, default transparent black. Values must fit finite RGBA16Float; alpha must be in 0..1. Later prop updates preserve native color and motion. |
| `style` | The standard controls' native style fields. Supply dimensions for an empty-label effect. |
| `radius` | Logical-pixel corner radius, default zero. GPUI clamps corners to the painted size. |
| `label` | Optional native text over the texture. It uses the shared document text helper. |
| `reducedMotion` | Default false. Enabling it finishes the active transition. GPUI's application-wide reduced-motion setting also takes effect on the next native draw. |
| `onEvent`, `ref` | Standard bridge event callback and asynchronous ref. The component has no React child slots. |

`transition` takes `to` and a nonnegative integer `durationMs`. A zero duration
sets the color immediately. A running transition starts again from the current
native color. `cancel` freezes that color. Reduced motion sets the target
immediately and requests no further animation frames. Animation uses GPUI's
clock and native frame requests. It does not require a JavaScript frame loop.

Events are `{type: 'click', generation}` and `{type: 'error', message}`.
Repeated identical GPU errors do not emit repeatedly. An error stops the
transition. This fixture does not implement device recovery.

`query(null)` returns current `color`, `animating`, `error`, and `painted`.
Painted data contains `bounds`, physical texture `size`, `color`, resource
`generation`, and the bridge `frame` tag. It can be older than current state.
The query does not force a draw.

The view reuses a texture when size and color are unchanged. New content gets a
new resource. GPUI scene records hold shared handles, so resize and removal do
not overwrite or release a resource still referenced by a recorded frame.
Label-only updates do not allocate a new texture. A production effect with a
large animated output may need a resource pool with explicit frame retirement;
this small fixture does not claim that allocation policy is optimal for video.

Run the Metal pixel and resource checks from the repository root:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 cargo run --manifest-path fixtures/gpu-component/Cargo.toml --release --example visual
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 cargo test --manifest-path fixtures/gpu-component/Cargo.toml --release
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 cargo clippy --manifest-path fixtures/gpu-component/Cargo.toml --release --all-targets -- -D warnings
```

The example keeps its window offscreen. It checks exact pixels for HDR blending,
parent opacity, clipping, corners, multiple textures, resize, and removal. It
also checks clicks, shared text, resource retention and release, native
transitions, retargeting, cancellation, both reduced-motion settings, and idle
frame requests. Its image is `/tmp/bridge-gpu-initial.png`.

The [application composition](../counter/README.md) includes this crate
and wrapper. Its source and relocated executable tests check the worker path.
With `interaction-tests`, a native driver verifies that the producer creates
new resources and changes GPU pixels while the React worker is synchronously
blocked. These are macOS/Metal/Bun checks. They do not establish browser support,
physical presentation timing, or an end-to-end performance budget.
