import assert from "node:assert/strict"
import { createRef, useLayoutEffect } from "react"
import { nativeComponent, type NativeRef } from "@gpuix/bridge"
import { attachApplication } from "@gpuix/bridge/application"

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
await root.unmount()
