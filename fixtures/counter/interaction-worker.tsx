import assert from "node:assert/strict"
import { createRef, useLayoutEffect } from "react"
import { attachApplication } from "@gpui-react/core/application"
import { nativeComponent, type NativeRef } from "@gpui-react/core"
import { Input, List, Text, type InputEvent, type InputRef, type ListEvent } from "@gpui-react/controls"

import { Texture } from "@gpui-react/gpu-example"

const bindings = require("./counter.node")
const root = attachApplication(bindings)
const Driver = nativeComponent<{}, never, null, null, Record<string, unknown>>("interaction-driver")
const driver = createRef<NativeRef<null, null, Record<string, unknown>>>()
const input = createRef<InputRef>()
const events: InputEvent[] = []
const replacementEvents: InputEvent[] = []
const scrolls: ListEvent[] = []
const originalHandler = (event: InputEvent) => events.push(event)
const replacementHandler = (event: InputEvent) => replacementEvents.push(event)
const onScroll = (event: ListEvent) => scrolls.push(event)
function App({ generation }: { generation: number }) {
  useLayoutEffect(() => { void input.current!.command({ type: "focus" }) }, [])
  return <Driver ref={driver}>
    <Input ref={input} initialValue="" caretColor="#00ff00" style={{ color: "#ffffff", fontSize: 16 }} onEvent={generation === 0 ? originalHandler : replacementHandler} />
    <List estimatedItemHeight={20} style={{ height: 100, width: "100%", shrink: 0 }} onEvent={onScroll}>
      {Array.from({ length: 100 }, (_, i) => <Text key={i} text={`row ${i}`} style={{ height: 20, color: "#ffffff" }} />)}
    </List>
    <Texture initialColor={[0.1, 0.2, 0.4, 1]} style={{ width: 64, height: 10, shrink: 0 }} />
  </Driver>
}
root.renderSync(<App generation={0} />)
await root.flush()
const initial = await input.current!.query(null)
await driver.current!.command(null)
const delivered = events.length
const scrollsDelivered = scrolls.length
bindings.beginWorkerStall()
const deadline = performance.now() + 8_000
try {
  // Native reads of the shared test flag do not run the JS event loop. No await,
  // React work, timer, or callback can run until the native script completes.
  while (!bindings.nativeProbeDone() && performance.now() < deadline) {}
  assert.ok(bindings.nativeProbeDone(), "Native interaction driver timed out")
  assert.equal(events.length, delivered)
  assert.equal(scrolls.length, scrollsDelivered)
} finally {
  bindings.finishWorkerStall()
}
// Replace the JS callback before yielding to the queued native events. Their
// original subscription must stay alive until native retirement is acknowledged.
root.renderSync(<App generation={1} />)
const report = await driver.current!.query(null)
assert.ok(report && !report.error, JSON.stringify(report))
assert.equal(report.value, "你cd")
const current = await input.current!.query(null)
assert.equal(current.value, "你cd")
assert.ok(current.revision > initial.revision)
// A response authored before the native edits cannot erase them after the stall.
await assert.rejects(input.current!.command({ type: "replace", value: "stale echo", expectedRevision: initial.revision }), /stale input revision/)
assert.equal((await input.current!.query(null)).value, "你cd")
const changes = events.filter((event): event is Extract<InputEvent, { type: "change" }> => event.type === "change")
assert.ok(changes.some(event => event.snapshot.value === "a"))
assert.ok(changes.some(event => event.snapshot.value === "ab"))
assert.equal(changes.at(-1)!.snapshot.value, "你cd")
assert.equal(replacementEvents.length, 0, "queued edits must use the callback active when native emitted them")
for (let i = 1; i < changes.length; i++) assert.ok(changes[i].snapshot.revision > changes[i - 1].snapshot.revision)
assert.ok(scrolls.slice(scrollsDelivered).some(event => event.type === "scroll"))
console.log("Native interaction during blocked JavaScript passed", JSON.stringify(report))
await root.unmount()
