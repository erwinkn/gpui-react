import assert from "node:assert/strict"
import { writeFileSync } from "node:fs"
import { NativeClient, NativeHost } from "@gpuix/native"
import { workerData } from "node:worker_threads"
import { useLayoutEffect, useRef, useState } from "react"
import { attachApplication } from "../../application.js"
import { render } from "../../reconciler/renderer.js"

if (process.env.GPUIX_HOST_TEST === "not-ready") {
  await new Promise((resolve) => setTimeout(resolve, 60000))
}
const renderer = attachApplication()
if (process.env.GPUIX_CLEANUP_FILE)
  process.on("exit", () =>
    writeFileSync(process.env.GPUIX_CLEANUP_FILE!, "worker cleanup ran"),
  )
async function verify() {
  const mode = process.env.GPUIX_HOST_TEST ?? "normal"
  if (mode === "crash") throw Error("Intentional application worker failure")
  if (mode === "unload") process.exit(7)
  assert.throws(
    () => new NativeClient(workerData.gpuixSession),
    /already has an application runtime/,
  )
  assert.throws(() => new NativeClient(0), /Unknown or expired/)
  assert.throws(
    () => new NativeClient(workerData.gpuixSession - 1),
    /Unknown or expired/,
  )
  assert.throws(() => new NativeHost({ focus: false }), /macOS main thread/)
  if (mode === "commits") {
    renderer.applyBatch(
      JSON.stringify([
        ["createElement", 1, "text"],
        ["setText", 1, "Initial"],
        ["setRoot", 1],
      ]),
    )
    await renderer.whenIdle()
    for (let index = 0; index < 256; index++) {
      renderer.applyBatch(
        JSON.stringify([["setText", 1, `Revision ${index}`]]),
      )
    }
    const last = JSON.stringify([["setText", 1, "Revision 256"]])
    assert.throws(() => renderer.applyBatch(last), /transport is full/)
    await renderer.whenIdle()
    assert.equal(
      JSON.parse(await renderer.query<string>("getAutomationTree")).text,
      "Revision 255",
    )
    assert.equal(JSON.parse(renderer.getAutomationTree()).text, "Revision 255")
    renderer.applyBatch(last)
    await renderer.whenIdle()
    assert.equal(
      JSON.parse(await renderer.query<string>("getAutomationTree")).text,
      "Revision 256",
    )
    await renderer.shutdown()
    return
  }
  if (mode === "first-frame") {
    await renderer.query("startHostProbe", 1000, 0)
    function InitialAnchor() {
      const list = useRef<any>(null)
      useLayoutEffect(() => renderer.scrollToItem(list.current.id, 99), [])
      return (
        <virtual-list
          ref={list}
          estimatedItemHeight={20}
          style={{ width: 300, height: 100 }}
        >
          {Array.from({ length: 100 }, (_, index) => (
            <text
              key={index}
              style={{ height: 20, flexShrink: 0 }}
            >{`Ready row ${index}`}</text>
          ))}
        </virtual-list>
      )
    }
    render(<InitialAnchor />)
    await renderer.whenIdle()
    const probe = JSON.parse(await renderer.query<string>("takeHostProbe"))
    const first = probe.find((record: any) => record.kind === "first-frame")
    assert(first, "The initial native frame must be recorded")
    assert(
      first.data.text.includes("Ready row 99"),
      "The first native frame must include the layout-effect anchor",
    )
    assert(!first.data.text.includes("Ready row 0"))
    if (process.env.GPUIX_HOST_OUTPUT)
      writeFileSync(
        process.env.GPUIX_HOST_OUTPUT,
        JSON.stringify({ probe }, null, 2),
      )
    await renderer.shutdown()
    return
  }
  let changes: string[] = []
  function Fixture() {
    const [value, setValue] = useState("")
    return (
      <div
        testId="root"
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          flexDirection: "column",
          gap: 10,
        }}
      >
        <div
          testId="animated"
          motion={{
            initial: { width: 30 },
            animate: { width: 300 },
            transition: { duration: 3, ease: "linear" },
          }}
          style={{ width: 30, height: 40, backgroundColor: "#ee9944" }}
        />
        <input
          testId="input"
          value={value}
          onChange={(event) => {
            changes.push(event.value ?? "")
            setValue(event.value ?? "")
          }}
          style={{ width: 200, height: 40 }}
        />
        <div
          testId="scroll"
          style={{ width: 200, height: 80, overflowY: "scroll" }}
        >
          {Array.from({ length: 20 }, (_, index) => (
            <text key={index} style={{ height: 25, flexShrink: 0 }}>
              Row {index}
            </text>
          ))}
        </div>
      </div>
    )
  }
  render(<Fixture />)
  if (mode === "automation")
    await new Promise((resolve) => setTimeout(resolve, 2000))
  if (mode === "blocked-shutdown") {
    await renderer.query("closeWindowAfterForTest", 50)
    const start = performance.now()
    while (performance.now() - start < 10000) {}
    throw Error("A blocked worker was not terminated")
  }
  await renderer.whenIdle()
  const tree = JSON.parse(await renderer.query<string>("getAutomationTree"))
  const find = (node: any, id: string): any =>
    node?.testId === id
      ? node
      : node?.children?.map((child: any) => find(child, id)).find(Boolean)
  const input = find(tree, "input").id
  const animated = find(tree, "animated").id
  renderer.focusElement(input)
  await renderer.query("getAutomationTree")
  await renderer.query("simulateKeystrokes", "a b c")
  await new Promise((resolve) => setTimeout(resolve, 30))
  assert.equal(changes.at(-1), "abc")
  await renderer.query("resizeWindowForTest", 520, 340)
  await renderer.query("getAutomationTree")
  assert.deepEqual(renderer.getWindowSize(), { width: 520, height: 340 })
  const widths = renderer.measureTextWidths("Helvetica", 14, 400, ["ii", "WW"])
  assert(widths[0]! > 0 && widths[1]! > widths[0]!)
  // Fill the bounded application queue in one synchronous task. A rejected
  // request must not prevent the accepted prefix from completing in order.
  const burst = Array.from({ length: 256 }, (_, index) =>
    renderer.query("setWindowTitle", `Burst ${index}`),
  )
  await assert.rejects(renderer.query("barrier"), /transport is full/)
  await renderer.whenIdle()
  await Promise.all(burst)
  await renderer.query("setWindowTitle", "GPUiX application runtime test")
  await assert.rejects(
    renderer.query("unsupported-operation"),
    /Unknown native command/,
  )
  await renderer.query("startHostProbe", 2000, animated)
  await renderer.query("dispatchKeysAfterForTest", 100, "d")
  await renderer.query("markHostProbe", "block-start")
  const started = performance.now()
  while (performance.now() - started < 600) {}
  await renderer.query("markHostProbe", "block-end")
  const probe = JSON.parse(await renderer.query<string>("takeHostProbe"))
  const start = probe.find((v: any) => v.data === "block-start").atMs
  const end = probe.find((v: any) => v.data === "block-end").atMs
  const frames = probe.filter(
    (v: any) => v.kind === "frame" && v.atMs > start + 30 && v.atMs < end - 30,
  )
  assert(frames.length > 10)
  assert(
    frames.some((frame: any) => frame.data.text.includes("abcd")),
    "Native input must paint during the worker block",
  )
  assert.equal(changes.at(-1), "abcd")
  if (process.env.GPUIX_HOST_OUTPUT)
    writeFileSync(
      process.env.GPUIX_HOST_OUTPUT,
      JSON.stringify(
        { probe, changes, widths, frames: frames.length },
        null,
        2,
      ),
    )
  if (mode === "close") await renderer.query("closeWindow")
  else if (mode === "menu")
    await Promise.all([
      renderer.query("terminateAppForTest"),
      renderer.query("terminateAppForTest"),
    ])
  else await renderer.shutdown()
}
void verify().catch((error) => renderer.reportStartupError(error))
