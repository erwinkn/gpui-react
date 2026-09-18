import assert from "node:assert/strict"
import { createRequire } from "node:module"
import { mkdirSync, readFileSync } from "node:fs"
import { resolve } from "node:path"
import { PNG } from "pngjs"
import { configureNativeBindings } from "../../packages/native/runtime.cjs"

process.env.GPUIX_BACKGROUND = "1"
const load = createRequire(import.meta.url)
const library = resolve(process.argv[2] ?? "fixtures/native-composition/example.node")
const bindings = load(library)
const info = configureNativeBindings(bindings)
assert.deepEqual(info.extensions.map((item: { id: string }) => item.id), ["gpuix.example"])
const native = load("../../packages/native/loader.cjs")
assert.equal(native.GpuixRenderer, bindings.GpuixRenderer)
assert.equal(load.cache[resolve("packages/native/index.js")], undefined)
const renderer = new native.TestGpuixRenderer(128, 96)
const commit = (ops: unknown[]) => {
  renderer.applyBatch(JSON.stringify(ops))
  renderer.flush()
}
const output = resolve("packages/react/screenshots/extension-composition")
mkdirSync(output, { recursive: true })
function screenshot(name: string) {
  const path = resolve(output, `${name}.png`)
  renderer.captureScreenshot(path)
  return PNG.sync.read(readFileSync(path))
}
function pixel(image: PNG, x: number, y: number, expected: number[]) {
  const scale = image.width / 128
  assert.equal(image.height, 96 * scale)
  const offset = (Math.floor((y + 0.5) * scale) * image.width + Math.floor((x + 0.5) * scale)) * 4
  const actual = [...image.data.subarray(offset, offset + 4)]
  actual.forEach((channel, index) => assert(Math.abs(channel - expected[index]) <= 1,
    `pixel ${x},${y}: ${actual}, expected ${expected}`))
}
commit([
  ["createElement", 1, "div"],
  ["setStyle", 1, { width: 128, height: 96, backgroundColor: "#0000ff" }],
  ["createElement", 2, "div"],
  ["setStyle", 2, { position: "absolute", left: 8, top: 8, width: 48, height: 48, overflow: "hidden", opacity: 0.25 }],
  ["createElement", 3, "example-gpu-texture"],
  ["setStyle", 3, { width: 64, height: 48 }],
  ["setCustomProp", 3, "color", [2, 0, 0, 0.25]],
  ["setCustomProp", 3, "radius", 8],
  ["setEventListener", 3, "click", true],
  ["appendChild", 2, 3],
  ["appendChild", 1, 2],
  ["createElement", 4, "example-gpu-texture"],
  ["setStyle", 4, { position: "absolute", left: 80, top: 8, width: 32, height: 32 }],
  ["setCustomProp", 4, "color", [0, 1, 0, 1]],
  ["appendChild", 1, 4],
  ["setRoot", 1],
])
const bounds = renderer.getElementBounds(3)
assert.deepEqual(bounds, { x: 8, y: 8, width: 64, height: 48 })
let image = screenshot("initial")
pixel(image, 24, 40, [128, 0, 239, 255])
pixel(image, 64, 40, [0, 0, 255, 255])
pixel(image, 8, 8, [0, 0, 255, 255])
pixel(image, 96, 24, [0, 255, 0, 255])
renderer.simulateClick(24, 40)
assert(renderer.drainEvents().some((event: { elementId: number; eventType: string }) => event.elementId === 3 && event.eventType === "click"))
commit([["setCustomProp", 3, "label", "External text"]])
assert(renderer.getPaintedText().includes("External text"))
assert.deepEqual(renderer.getElementBounds(3), bounds)
commit([
  ["setCustomProp", 3, "label", ""],
  ["setCustomProp", 3, "color", [0.5, 0, 0, 0.5]],
  ["setStyle", 3, { width: 32, height: 32 }],
])
image = screenshot("resized")
pixel(image, 24, 24, [32, 0, 223, 255])
pixel(image, 48, 40, [0, 0, 255, 255])
commit([["destroyElement", 3], ["destroyElement", 4]])
image = screenshot("removed")
pixel(image, 24, 24, [0, 0, 255, 255])
pixel(image, 96, 24, [0, 0, 255, 255])
assert.equal(renderer.getElementBounds(3), null)
console.log(JSON.stringify({ ok: true, runtime: info, evidence: output }))
