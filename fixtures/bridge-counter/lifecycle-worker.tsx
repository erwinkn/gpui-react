import { appendFileSync, writeFileSync } from "node:fs"
import { createRef, useEffect, useLayoutEffect } from "react"
import { attachApplication } from "@gpui-react/core/application"
import { nativeComponent, type NativeRef } from "@gpui-react/core"

const bindings = require("./counter.node")
const root = attachApplication(bindings)
const mode = process.env.BRIDGE_LIFECYCLE_MODE!
const path = process.env.BRIDGE_LIFECYCLE_REACT_FILE!
process.on("exit", () => appendFileSync(path, "exit\n"))
const Probe = nativeComponent<{ record_path: string }, number, "close" | "overflow">("lifecycle-probe")
const probe = createRef<NativeRef<"close" | "overflow">>()
function App() {
  useLayoutEffect(() => () => appendFileSync(path, "layout\n"), [])
  useEffect(() => () => appendFileSync(path, "passive\n"), [])
  return <Probe ref={probe} record_path={process.env.BRIDGE_LIFECYCLE_NATIVE_FILE!} onEvent={() => {}} />
}
root.renderSync(<App />)
await root.flush()
// Let mount effects complete before testing shutdown.
await new Promise(resolve => setTimeout(resolve, 0))
if (mode === "normal" || mode === "after-host") {
  await root.unmount()
} else if (mode === "crash") {
  throw Error("Expected failure after native mount")
} else if (mode === "exit") {
  process.exit(7)
} else {
  if (mode === "blocked-close") await probe.current!.command("close")
  if (mode === "overflow") await probe.current!.command("overflow")
  if (mode.startsWith("blocked-") || mode === "overflow") bindings.beginWorkerStall()
  console.log("Lifecycle worker ready")
  writeFileSync(process.env.BRIDGE_LIFECYCLE_READY_FILE!, "worker")
  if (mode.startsWith("blocked-") || mode === "overflow") {
    const deadline = performance.now() + 20_000
    while (performance.now() < deadline) {}
    throw Error("Blocked worker was not terminated")
  }
}
