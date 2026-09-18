# GPU texture composition

Use a native extension to render an effect into a GPU texture. GPUI composes the
texture in normal scene order. Effect shaders and animation state stay in the
extension crate. The core has no effect-specific background payload.

Get the window's device and queue with `Window::gpu_context()`. Downcast the
returned value to the backend type:

| Backend | Context type | Texture type |
| --- | --- | --- |
| Metal | `(metal::Device, metal::CommandQueue)` | `metal::Texture` |
| wgpu | `(Arc<wgpu::Device>, Arc<wgpu::Queue>)` | `wgpu::Texture` |

Use that device to create textures. Submit effect commands on that queue before
GPUI paints the texture. Queue order lets GPUI sample the result without a CPU
wait or a GPU-to-CPU copy. Do not use a separate submission queue.

During an element's paint phase, call:

```rust
window.paint_gpu_texture(bounds, texture, corner_radii)?;
```

`bounds` is `Bounds<Pixels>`. `corner_radii` is `Corners<Pixels>`.
`texture` is `GpuTextureHandle`, an `Arc` that contains the backend texture.
Native handles require `Send + Sync`; browser handles stay on their owning
thread. This API uses the same compiled GPUI and backend crates as the runtime.

The texture must be a single-sample 2D texture with shader-read usage. Accepted
formats are RGBA8 unorm, BGRA8 unorm, and RGBA16 float. The method rejects an
unsupported type, format, or usage before it records the draw. Metal also checks
the device identity. On wgpu, the caller must use the supplied device; wgpu
validates device ownership when the resource is bound.

Store premultiplied colour values in the texture. GPUI multiplies all four
channels by inherited opacity and corner coverage. RGB values in a float
texture may exceed alpha. GPUI does not clamp them to alpha before blending.
Use the render target's colour space. Metal's unorm target uses encoded colour;
a wgpu sRGB target expects linear colour for composition.

GPUI applies the current content mask and clamps corner radii to the painted
bounds. Later scene content can cover the texture. The scene retains a shared
texture handle until the frame is released. A resize can replace the texture;
the previous frame retains its own handle. Removing an effect means that the
next frame does not paint its texture. Animation, cancellation, and reduced
motion remain the extension's responsibility.

`gpu_device_lost()` reports device loss where the backend can detect it. Stop
submitting if it returns `Some(true)`. Recreate resources after recovery. The
browser backend currently requires a page reload after device loss.

The Metal headless tests cover transparent blending, float RGB above alpha,
opacity, clip masks, corner radii, resize, and later scene content. The producer
uses a GPU clear command on the shared queue. Only the screenshot assertion
waits for readback.

The [framework composition fixture](../fixtures/native-composition/README.md)
tests the public extension and loader APIs. Its pixel checks cover opacity, HDR,
clipping, corners, multiple textures, resize, and removal. It also checks clicks,
shared text, source workers, and relocated compiled workers. Browser pixel checks
run in headless Chrome on macOS, with native WebGPU and ANGLE WebGL2.

The upstream [external compositor proposal](https://github.com/zed-industries/zed/pull/60573)
was closed pending API discussion. This fork extends its existing texture
composition path. It retains the general gradient premultiplication correction
and premultiplied Metal quad blending.
