import assert from "node:assert/strict"
import { cp, rm } from "node:fs/promises"
import { resolve, join } from "node:path"

process.env.GPUIX_BACKGROUND = "1"
if (!process.argv[2]) throw Error("Usage: bun fixtures/bridge-pierre/legacy.ts /path/to/libpierre_native_runtime.dylib")
const binary = join(import.meta.dir, "legacy.node")
await cp(resolve(process.argv[2]), binary)
try {
  const bindings = require(binary)
  const renderer = new bindings.TestGpuixRenderer(600, 240)
  const commit = (operations: unknown[]) => { renderer.applyBatch(JSON.stringify(operations)); renderer.flush() }
  const spec = { session: 1, documentVersion: 1, text: "legacy line", rows: [
    { id: "line", left: { text: "legacy line", number: 1, start: 0, side: "additions" } },
  ] }
  commit([
    ["createElement", 1, "pierre-viewport"], ["setStyle", 1, { width: 600, height: 240 }],
    ["setCustomProp", 1, "spec", spec], ["setEventListener", 1, "change", true], ["setEventListener", 1, "keyDown", true], ["setRoot", 1],
  ])
  assert.ok(renderer.getPaintedText().includes("legacy line"))
  assert.deepEqual(renderer.getElementBounds(1), { x: 0, y: 0, width: 600, height: 240 })
  const events = renderer.drainEvents().filter((event: { eventType: string }) => event.eventType === "change")
  assert.ok(events.map((event: { value: string }) => JSON.parse(event.value)).some((event: { kind: string }) => event.kind === "layout"), "legacy change.value keeps its JSON envelope")
  renderer.focusElement(1)
  renderer.simulateKeystrokes("left")
  renderer.flush()
  assert.ok(renderer.drainEvents().filter((e: { eventType: string }) => e.eventType === "change").map((e: { value: string }) => JSON.parse(e.value)).some((e: { kind: string; key: string }) => e.kind === "key" && e.key === "left"))
  const preedit = JSON.parse(renderer.simulateInputMethod("日本", true))
  assert.deepEqual(preedit.marked, [0, 2])
  assert.deepEqual(preedit.selected, [2, 2])
  assert.ok(renderer.getPaintedText().includes("日本legacy line"))
  const selectedPreedit = JSON.parse(renderer.simulateInputMethod("🙂", true, 0, 2))
  assert.deepEqual(selectedPreedit.marked, [0, 2])
  assert.deepEqual(selectedPreedit.selected, [0, 2])
  const committed = JSON.parse(renderer.simulateInputMethod("你", false))
  assert.equal(committed.marked, null)
  assert.ok(renderer.drainEvents().filter((e: { eventType: string }) => e.eventType === "change").map((e: { value: string }) => JSON.parse(e.value)).some((e: { kind: string; text: string }) => e.kind === "insert" && e.text === "你"))
  commit([["setCustomProp", 1, "patch", {
    base: 1, version: 2, start: 0, deleteCount: 1, text: "patched legacy",
    rows: [{ id: "line", left: { text: "patched legacy", number: 1, start: 0, side: "additions" } }],
  }]])
  assert.ok(renderer.getPaintedText().includes("patched legacy"))
  commit([["setCustomProp", 1, "spec", {
    session: 1, documentVersion: 3, text: "patched legacy", readOnly: true,
    rows: [{ id: "line", left: { text: "patched legacy", number: 1, start: 0, side: "additions" } }],
  }]])
  renderer.simulateClick(85, 10, 0, undefined, 2)
  assert.deepEqual(JSON.parse(renderer.simulateInputMethod("ignored", false)).selected, [0, 7])
  renderer.simulateClick(85, 10, 0, undefined, 3)
  assert.deepEqual(JSON.parse(renderer.simulateInputMethod("ignored", false)).selected, [0, 14])
  commit([["destroyElement", 1]])
  assert.equal(renderer.getElementBounds(1), null)
  console.log("PASS legacy Pierre composition: native registration, paint, bounds, event payload, IME, counted clicks, patch, and removal")
} finally { await rm(binary, { force: true }) }
