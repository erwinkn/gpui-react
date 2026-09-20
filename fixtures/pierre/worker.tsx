import assert from "node:assert/strict"
import { Fragment, createRef, useLayoutEffect } from "react"
import { nativeComponent, type NativeRef, type FrameInfo } from "@gpui-react/core"
import { attachApplication } from "@gpui-react/core/application"
import { Container, Text, type ContainerRef, type Style } from "@gpui-react/kit"

type Spec = { text: string; documentVersion: number; session: number; rows: object[] }
type Event = { kind: string; seq: number; documentVersion: number }
type Command = { type: "focus" | "blur" } | { type: "update"; spec?: Spec; view?: object; patch?: object }
type Snapshot = {
  text: string; documentVersion: number; seq: number; anchor: number; head: number;
  bounds: number[]; paintedDocumentVersion: number; paintCount: number; focused: boolean; frame: FrameInfo | null;
}
const Pierre = nativeComponent<{ initialSpec?: Spec; style?: Style; label?: string }, Event, Command, null, Snapshot>("pierre-viewport")
const verifyPaint = process.env.PIERRE_FRAME_PROBE === "1"
const FrameProbe = verifyPaint ? nativeComponent<object>("pierre-frame-probe") : Fragment
const editor = createRef<NativeRef<Command, null, Snapshot>>()
const annotation = createRef<ContainerRef>()
const events: Event[] = []
const bindings = require("./pierre.node")
const root = attachApplication(bindings)
let focus: Promise<void> | undefined
function spec(text: string, version: number): Spec {
  return { text, documentVersion: version, session: 1, rows: [
    { id: "annotation", slot: 0, kind: "annotation" },
    { id: "line", left: { text, start: 0, number: 1, side: "additions" } },
  ] }
}
function App({ height = 30, initialText }: { height?: number; initialText?: string }) {
  useLayoutEffect(() => {
    focus = Promise.all([
      editor.current!.command({ type: "update", spec: spec("hello", 1) }),
      editor.current!.command({ type: "focus" }),
    ]).then(() => undefined)
  }, [])
  return <FrameProbe><Pierre ref={editor} initialSpec={initialText === undefined ? undefined : spec(initialText, 1)} style={{ width: 600, height: 260 }}
    label="Pierre test editor" onEvent={event => events.push(event)}>
    <Container ref={annotation} measure style={{ height, background: "#334455" }}>
      <Text text="Native annotation" />
    </Container>
  </Pierre></FrameProbe>
}
async function painted(version: number): Promise<Snapshot> {
  const deadline = Date.now() + 3000
  for (;;) {
    const snapshot = await editor.current!.query(null)
    if (snapshot.paintedDocumentVersion === version && snapshot.frame) return snapshot
    if (Date.now() >= deadline) throw Error(`No painted Pierre revision ${version}: ${JSON.stringify(snapshot)}`)
    await new Promise(resolve => setTimeout(resolve, 10))
  }
}
root.renderSync(<App />)
await root.flush()
await focus
const first = await painted(1)
assert.equal(first.text, "hello")
assert.ok(first.bounds[2] > 0 && first.bounds[3] > 0)
assert.equal(first.focused, true, "layout-effect command sets native logical focus without activating the window")
assert.ok(events.some(event => event.kind === "layout"))
assert.ok(events.filter(event => event.kind === "layout").every(event => event.documentVersion === 1), "mount source command precedes the first native layout")
assert.equal((await annotation.current!.query(null)).painted!.bounds.height, 30)
await editor.current!.command({ type: "update", patch: {
  base: 1, version: 2, start: 1, deleteCount: 1,
  rows: spec("hello from the external model", 2).rows.slice(1), text: "hello from the external model",
} })
assert.equal((verifyPaint ? await painted(2) : await editor.current!.query(null)).text, "hello from the external model")
const id = editor.current!.id
root.renderSync(<App height={60} initialText="construction-only" />)
await root.flush()
let slot = await annotation.current!.query(null)
for (let i = 0; verifyPaint && slot.painted?.bounds.height !== 60 && i < 100; i++) {
  await new Promise(resolve => setTimeout(resolve, 10))
  slot = await annotation.current!.query(null)
}
if (verifyPaint) assert.equal(slot.painted?.bounds.height, 60)
assert.equal(slot.childCount, 1)
assert.equal(editor.current!.id, id, "React update keeps the ordinary native viewport")
assert.equal((await editor.current!.query(null)).text, "hello from the external model", "initialSpec does not overwrite native source on rerender")
await editor.current!.command({ type: "update", view: {
  session: 1, revision: 1, ack: 0, selections: [{ anchor: 2, head: 5 }],
} })
const selected = await editor.current!.query(null)
assert.deepEqual([selected.anchor, selected.head], [2, 5])
await editor.current!.command({ type: "update", view: {
  session: 1, revision: 1, ack: 0, selections: [{ anchor: 0, head: 0 }],
} })
const stale = await editor.current!.query(null)
assert.deepEqual([stale.anchor, stale.head], [2, 5], "old view acknowledgement cannot reset selection")
await editor.current!.command({ type: "blur" })
await root.flush()
assert.equal((await editor.current!.query(null)).focused, false)
await root.unmount()
if (verifyPaint) console.log("PASS forced native frames after source and annotation updates")
console.log("PASS Pierre bridge: layout-effect focus, native events, patches, annotation children, keyed identity, stale acknowledgements, and teardown")
