import assert from "node:assert/strict"
import { createRef, useLayoutEffect } from "react"
import { nativeComponent, type NativeRef } from "@gpui-react/core"
import { attachApplication } from "@gpui-react/core/application"

import { Input, type InputRef, type InputEvent } from "@gpui-react/controls"

const bindings = require("./counter.node")
if (process.env.BRIDGE_COUNTER_MODE === "startup-error") throw Error("Expected worker startup failure")
const root = attachApplication(bindings)
type State = { count: number; step: number; renders: number }
const Counter = nativeComponent<{ step: number }, { value: number }, "increment" | "start" | "stop", null, State>("counter")
const ref = createRef<NativeRef<"increment" | "start" | "stop", null, State>>()
const values: number[] = []
const onEvent = (event: { value: number }) => values.push(event.value)
let initial: Promise<void> | undefined

function App({ step }: { step: number }) {
  useLayoutEffect(() => { initial = ref.current!.command("increment") }, [])
  return <Counter ref={ref} step={step} onEvent={onEvent} />
}

root.renderSync(<App step={process.env.BRIDGE_COUNTER_MODE === "invalid-props" ? -1 : 2} />)
await root.flush()
await initial
assert.equal((await ref.current!.query(null)).count, 2)
assert.deepEqual(values, [2])
root.renderSync(<App step={5} />)
await root.flush()
await ref.current!.command("increment")
assert.equal((await ref.current!.query(null)).count, 7)
assert.deepEqual(values, [2, 7])

await ref.current!.command("start")
// Deliberately block application JavaScript. GPUI's executor owns the timer.
const until = performance.now() + 300
while (performance.now() < until) {}
await ref.current!.command("stop")
const state = await ref.current!.query(null)
assert.ok(state.count >= 7 + 5 * 10, `Native timer stalled during worker block: ${JSON.stringify(state)}`)
assert.ok(state.renders > 0, "The native view did not render")
console.log("Native counter state after worker stall:", state)
// A command immediately followed by removal still reports its earlier event.
const count = state.count
const increment = ref.current!.command("increment")
root.renderSync(null)
await root.flush()
await increment
assert.equal(values.at(-1), count + 5)
// The same ordinary input is wrapped through the production worker bridge.
const input = createRef<InputRef>()
const inputEvents: InputEvent[] = []
const onInput = (event: InputEvent) => inputEvents.push(event)
root.renderSync(<Input ref={input} initialValue="native" label="Example input" onEvent={onInput} />)
await root.flush()
const id = input.current!.id
const before = await input.current!.query(null)
assert.equal(before.value, "native")
await input.current!.command({ type: "focus" })
await input.current!.command({ type: "replace", value: "first", expectedRevision: before.revision })
await assert.rejects(input.current!.command({ type: "replace", value: "late", expectedRevision: before.revision }), /stale input revision/)
root.renderSync(<Input ref={input} initialValue="stale prop" placeholder="Updated" onEvent={onInput} />)
await root.flush()
assert.equal(input.current!.id, id)
const current = await input.current!.query(null)
assert.equal(current.value, "first")
assert.equal(inputEvents.filter(event => event.type === "change").length, 1)
await input.current!.command({ type: "select", selection: { start: 1, end: 4 }, expectedRevision: current.revision })
assert.deepEqual((await input.current!.query(null)).selection, { start: 1, end: 4, reversed: false })
await input.current!.command({ type: "blur" })
console.log("React input: native identity, ordered events, stale-command rejection, selection, and prop preservation passed")
await root.unmount()
