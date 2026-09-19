import assert from "node:assert/strict"
import { createRef } from "react"
import { attachApplication } from "@gpuix/bridge/application"
import { Document, type DocumentRef } from "@gpuix/bridge-controls"
import { Texture, type TextureRef } from "@gpuix/bridge-gpu-example"

const bindings = require("./counter.node")
const root = attachApplication(bindings)
const texture = createRef<TextureRef>()
const document = createRef<DocumentRef>()
const errors: string[] = []
function App({ width = 64, initial = 2 }: { width?: number; initial?: number }) {
  return <Document ref={document} style={{ width: 200, height: 100, background: "blue" }}>
    <Texture ref={texture} style={{ width, height: 48, opacity: 0.25, color: "white" }}
      initialColor={[initial, 0, 0, 0.25]} radius={8} label="external GPU view"
      onEvent={event => { if (event.type === "error") errors.push(event.message) }} />
  </Document>
}
root.renderSync(<App />)
await root.flush()
const initial = await texture.current!.query(null)
assert.equal(initial.error, null)
assert.equal(initial.painted?.bounds.width, 64)
assert.deepEqual(initial.painted?.color, [2, 0, 0, 0.25])
assert.ok(initial.painted?.frame)
assert.equal(initial.painted!.size[0], 64 * initial.painted!.frame!.scaleFactor)
assert.ok((await document.current!.query(null)).text.some(text => text.text === "external GPU view"))
await texture.current!.command({ type: "transition", to: [0, 1, 0, 1], durationMs: 1000 })
await new Promise(resolve => setTimeout(resolve, 80))
const moving = await texture.current!.query(null)
assert.ok(moving.animating && moving.color[1] > 0 && moving.color[1] < 1)
await texture.current!.command({ type: "cancel" })
const stopped = await texture.current!.query(null)
assert.equal(stopped.animating, false)
root.renderSync(<App width={32} initial={20} />)
await root.flush()
assert.deepEqual((await texture.current!.query(null)).color, stopped.color, "props preserve the native effect state")
assert.deepEqual(errors, [])
await root.unmount()
console.log("React GPU component: float texture, native state, document text, transition, and cancellation passed")
