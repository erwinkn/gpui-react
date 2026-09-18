import { configureNativeBindings } from "../../packages/native/runtime.cjs"
import * as bindings from "./wasm/example.js"
import wasmUrl from "./wasm/example_bg.wasm" with { type: "file" }

if (new URL(location.href).searchParams.get("backend") === "webgl") {
  // Exercise the real automatic fallback when WebGPU is unavailable.
  Object.defineProperty(navigator, "gpu", { value: undefined, configurable: true })
}
await bindings.default({ module_or_path: wasmUrl })
bindings.registerProbe()
const info = configureNativeBindings(bindings)
const { createElement: h } = await import("react")
const { render } = await import("@gpuix/react")
const { GpuixRenderer } = await import("@gpuix/native")
if (!Object.is(GpuixRenderer, bindings.GpuixRenderer)) throw new Error("The configured composition was not selected")
let clicks = 0
function scene(removed = false, resized = false) {
  return h("div", { style: { width: 128, height: 96, backgroundColor: "#0000ff" } },
    removed ? null : [
      h("div", { key: "clip", style: { position: "absolute", left: 8, top: 8, width: 48, height: 48, overflow: "hidden", opacity: 0.25 } },
        h("example-gpu-texture", { testId: "texture", color: resized ? [0.5, 0, 0, 0.5] : [2, 0, 0, 0.25], radius: 8,
          onClick: () => { clicks++ }, style: { width: resized ? 32 : 64, height: resized ? 32 : 48 } })),
      h("example-gpu-texture", { key: "green", testId: "green", color: [0, 1, 0, 1],
        style: { position: "absolute", left: 80, top: 8, width: 32, height: 32 } }),
    ])
}
const root = render(scene(), { title: "GPUiX extension test", focus: false })
Object.assign(globalThis, { extensionProbe: { info, clicks: () => clicks, resize: () => root.render(scene(false, true)), remove: () => root.render(scene(true)) } })
