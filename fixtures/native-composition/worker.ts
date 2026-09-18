import assert from "node:assert/strict"
import { configureNativeBindings } from "../../packages/native/runtime.cjs"
configureNativeBindings(require("./example.node"))
const { attachApplication } = await import("@gpuix/react/application")
const renderer = attachApplication()
const { createElement: h } = await import("react")
const { render } = await import("@gpuix/react")
let id = 0
render(h("example-gpu-texture", {
  ref: (node: { id: number } | null) => { id = node?.id ?? 0 },
  label: "Worker texture", color: [0, 0.5, 0, 0.5], radius: 8,
  style: { width: 120, height: 80, color: "#ffffff" },
}))
await renderer.whenIdle()
const tree = JSON.parse(await renderer.query<string>("getAutomationTree"))
assert.equal(tree.type, "example-gpu-texture")
assert.equal(tree.id, id)
assert.equal(tree.bounds.width, 120)
assert.equal(tree.bounds.height, 80)
await renderer.shutdown()
