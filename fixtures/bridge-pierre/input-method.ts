import assert from "node:assert/strict"
import { cp, rm } from "node:fs/promises"
import { join, resolve } from "node:path"

// Generic helper check. It loads core only; the Pierre probe checks its external editor.
if (!process.argv[2]) throw Error("Usage: bun fixtures/bridge-pierre/input-method.ts /path/to/libgpuix_native.dylib")
process.env.GPUIX_BACKGROUND = "1"
const path = join(import.meta.dir, "core-ime.node")
await cp(resolve(process.argv[2]), path)
try {
  const renderer = new (require(path).TestGpuixRenderer)(400, 120)
  assert.throws(() => renderer.simulateInputMethod("x", true), /No focused input handler/)
  renderer.applyBatch(JSON.stringify([
    ["createElement", 1, "input"], ["setStyle", 1, { width: 300, height: 40 }],
    ["setCustomProp", 1, "value", "abc"], ["setEventListener", 1, "change", true], ["setRoot", 1],
  ]))
  renderer.flush()
  renderer.focusElement(1)
  renderer.flush()
  renderer.simulateKeystrokes("cmd-right")
  renderer.flush()
  let result = JSON.parse(renderer.simulateInputMethod("🙂", true))
  assert.deepEqual(result, { selected: [5, 5], marked: [3, 5] })
  assert.ok(renderer.getPaintedText().includes("abc🙂"))
  result = JSON.parse(renderer.simulateInputMethod("日本", true, 0, 1))
  assert.deepEqual(result, { selected: [3, 4], marked: [3, 5] })
  result = JSON.parse(renderer.simulateInputMethod("你", false))
  assert.deepEqual(result, { selected: [4, 4], marked: null })
  assert.ok(renderer.getPaintedText().includes("abc你"))
  // A second operation proves the platform handler was restored after the first.
  result = JSON.parse(renderer.simulateInputMethod("!", false))
  assert.deepEqual(result.selected, [5, 5])
  assert.ok(renderer.getPaintedText().includes("abc你!"))
  assert.ok(renderer.drainEvents().some((event: { eventType: string; value: string }) => event.eventType === "change" && event.value === "abc你!"))
  renderer.simulateKeystrokes("cmd-z")
  renderer.flush()
  assert.ok(renderer.getPaintedText().includes("abc你"))
  renderer.applyBatch(JSON.stringify([["setCustomProp", 1, "readOnly", true]]))
  renderer.flush()
  assert.equal(JSON.parse(renderer.simulateInputMethod("ignored", true)).marked, null)
  assert.equal(JSON.parse(renderer.simulateInputMethod("ignored", false)).marked, null)
  assert.ok(renderer.getPaintedText().includes("abc你"))
  console.log("PASS native input-method helper: missing focus, UTF-16 preedit selection, commit, handler restoration, events, undo, and read-only input")
} finally { await rm(path, { force: true }) }
