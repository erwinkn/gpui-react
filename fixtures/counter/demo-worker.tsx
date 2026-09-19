import { Fragment, createRef, useEffect, useLayoutEffect, useState } from "react"
import { nativeComponent, type NativeRef } from "@gpui-react/core"
import { attachApplication } from "@gpui-react/core/application"
import { Container, Document, Input, List, Text, type InputEvent } from "@gpui-react/core"

const root = attachApplication(require("./counter.node"))
const Counter = nativeComponent<{ step: number }, never, "start" | "stop", null, { count: number }>("counter")
const counter = createRef<NativeRef<"start" | "stop", null, { count: number }>>()
const Capture = process.env.BRIDGE_DEMO_IMAGE ? nativeComponent<object>("demo-capture") : Fragment
const panel = { background: "#1c2430", padding: 18, gap: 12, radius: 10, width: "100%" } as const
const button = { background: "#31579b", paddingX: 18, paddingY: 12, radius: 8, hover: { background: "#426eb9" } } as const
const rows = Array.from({ length: 500 }, (_, index) => <Container key={index} style={{ height: 32, shrink: 0, paddingX: 12, background: index % 2 ? "#1c2430" : "#222d3b" }}>
  <Text style={{ lineHeight: 32 }} text={`Native list row ${String(index + 1).padStart(3, "0")}`} />
</Container>)

function Heartbeat() {
  const [ticks, setTicks] = useState(0)
  useEffect(() => { const timer = setInterval(() => setTicks(tick => tick + 1), 250); return () => clearInterval(timer) }, [])
  return <Text style={{ color: "#b4c7e0" }} text={`React heartbeat: ${ticks}`} />
}
function Editing() {
  const [received, setReceived] = useState("No edit events received yet")
  function onEdit(event: InputEvent) {
    if (event.type === "change") setReceived(`Last JS event: ${event.snapshot.value}`)
  }
  return <Container style={panel}>
    <Text style={{ fontSize: 19 }} text="Type here while JavaScript is paused" />
    <Input initialValue="Native text keeps working" label="Native demo input" initialMultiline minRows={2} maxRows={2}
      style={{ width: "100%", padding: 10, fontSize: 17, background: "#101722", borderWidth: 1, borderColor: "#506987", radius: 6 }} onEvent={onEdit} />
    <Text style={{ color: "#b4c7e0", fontSize: 13 }} text={received} />
  </Container>
}
function Pause() {
  const [state, setState] = useState("Ready")
  async function pause() {
    setState("JavaScript will pause for 5 seconds. Type, scroll, or click the native counter.")
    // Let this status commit and give the native window a chance to paint it.
    await new Promise(resolve => setTimeout(resolve, 100))
    await root.flush()
    const before = (await counter.current!.query(null)).count
    const deadline = performance.now() + 5000
    while (performance.now() < deadline) {}
    const after = (await counter.current!.query(null)).count
    setState(`JS resumed. The native counter advanced by ${after - before} during the pause.`)
  }
  return <Container style={{ gap: 10 }}>
    <Container style={button} onEvent={event => { if (event.type === "click") void pause() }}>
      <Text text="Pause JavaScript for 5 seconds" />
    </Container>
    <Text style={{ fontSize: 13, color: "#b4c7e0" }} text={state} />
  </Container>
}
function App() {
  useLayoutEffect(() => { void counter.current!.command("start") }, [])
  useEffect(() => {
    if (process.env.BRIDGE_DEMO_AUTOCLOSE !== "1") return
    const timer = setTimeout(() => { void root.unmount() }, 1500)
    return () => clearTimeout(timer)
  }, [])
  return <Container style={{ width: "100%", height: "100%", padding: 24, gap: 16, background: "#111923", color: "#eef4ff", fontSize: 15 }}>
    <Text style={{ fontSize: 28 }} text="React describes. GPUI runs." />
    <Text style={{ color: "#b4c7e0" }} text="Pause the application worker. Native editing, scrolling, hover, and the counter can still respond." />
    <Container style={{ direction: "row", width: "100%", gap: 18 }}>
      <Container style={{ width: 530, gap: 16 }}>
        <Container style={panel}>
          <Text text="Ordinary Rust GPUI component, wrapped for React" />
          <Counter ref={counter} step={1} />
          <Heartbeat />
          <Pause />
        </Container>
        <Editing />
        <Document style={panel} search={{ query: "native" }}>
          <Text textKey="one" text="Select native text across these two lines." />
          <Text textKey="two" text="Search highlights and native selection stay in GPUI." />
        </Document>
      </Container>
      <Container style={{ width: 380, gap: 12 }}>
        <Text style={{ fontSize: 19 }} text="500 rows, native scrolling" />
        <List estimatedItemHeight={32} style={{ width: "100%", height: 530 }}>{rows}</List>
      </Container>
    </Container>
  </Container>
}
root.renderSync(<Capture><App /></Capture>)
await root.flush()
console.log("Bridge demo ready")
