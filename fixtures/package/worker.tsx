import assert from "node:assert/strict"
import { createRef, useLayoutEffect } from "react"
import bindings from "@gpui-react/runtime"
import { nativeComponent } from "@gpui-react/core"
import { attachApplication } from "@gpui-react/core/application"
import {
  Container, Document, Input, List, Text,
  type ContainerRef, type DocumentRef, type DocumentEvent,
  type InputRef, type InputEvent, type ListRef, type TextRef,
} from "@gpui-react/core"

const root = attachApplication(bindings)
if (process.env.BRIDGE_PACKAGE_MODE === "unknown") {
  const TestOnly = nativeComponent<object>("counter")
  root.renderSync(<TestOnly />)
  await root.flush() // A fixture component must not exist in the default runtime.
  throw Error("Default runtime accepted a test-only component")
}

const container = createRef<ContainerRef>()
const document = createRef<DocumentRef>()
const input = createRef<InputRef>()
const list = createRef<ListRef>()
const row = createRef<TextRef>()
const inputEvents: InputEvent[] = []
const documentEvents: DocumentEvent[] = []
let focused: Promise<void> | undefined, anchored: Promise<void> | undefined
function App({ initialValue }: { initialValue: string }) {
  useLayoutEffect(() => {
    focused = input.current!.command({ type: "focus" })
    anchored = list.current!.command({ type: "scrollTo", index: 50_000, offset: 3 })
  }, [])
  return <Container ref={container} measure style={{ width: 400, height: 320, color: "white" }}>
    <Document ref={document} search={{ query: "token" }} onEvent={event => documentEvents.push(event)} style={{ width: 400, height: 50 }}>
      <Text textKey="greeting">Hello {"😀"}! token</Text>
      <Text textKey="second">another token</Text>
    </Document>
    <Input ref={input} initialValue={initialValue} label="Installed input" onEvent={event => inputEvents.push(event)} style={{ width: 400, height: 40 }} />
    <List ref={list} itemCount={100_000} windowStart={49_998} estimatedItemHeight={20} style={{ width: 400, height: 100 }}>
      {Array.from({ length: 60 }, (_, local) => <Text key={local} ref={local === 2 ? row : undefined} measure={local === 2} style={{ height: 20, lineHeight: 20 }} text={`row ${49_998 + local}`} />)}
    </List>
  </Container>
}

root.renderSync(<App initialValue="native" />)
await root.flush()
await Promise.all([focused, anchored])
const [outer, text, buffer, rows, first] = await Promise.all([
  container.current!.query(null), document.current!.query(null), input.current!.query(null),
  list.current!.query(null), row.current!.query(null),
])
assert.equal(outer.childCount, 3)
assert.ok(outer.painted?.frame, "initial native draw must finish before the first query")
assert.equal(buffer.value, "native")
assert.ok(buffer.painted?.frame)
assert.deepEqual(rows.anchor, { index: 50_000, offset: 3 })
assert.deepEqual(rows.supplied, { start: 49_998, end: 50_058 })
assert.equal(first.text, "row 50000")
assert.equal(first.painted!.bounds.y, rows.painted!.bounds.y - 3, "layout-effect scroll must apply before the first draw")
assert.deepEqual(text.text.map(item => item.text), ["Hello 😀! token", "another token"])
assert.equal(text.matchCount, 2)
assert.ok(documentEvents.some(event => event.type === "search" && event.count === 2 && !!event.frame))
for (const frame of [text.frame, buffer.painted!.frame, rows.painted!.frame, first.painted!.frame]) {
  assert.deepEqual(frame, outer.painted!.frame, "all controls must report the same native draw")
}
await document.current!.command({ type: "select", start: { key: "greeting", offset: 6 }, end: { key: "greeting", offset: 8 }, expectedContentRevision: text.contentRevision })
assert.equal((await document.current!.query(null)).selection, "😀")
await document.current!.command({ type: "selectAll" })
assert.equal((await document.current!.query(null)).selection, "Hello 😀! token\nanother token")
await document.current!.command({ type: "clear" })
assert.equal((await document.current!.query(null)).selection, null)

const id = input.current!.id
await input.current!.command({ type: "replace", value: "changed", expectedRevision: buffer.revision })
await assert.rejects(input.current!.command({ type: "replace", value: "stale", expectedRevision: buffer.revision }), /stale input revision/)
root.renderSync(<App initialValue="stale prop" />)
await root.flush()
assert.equal(input.current!.id, id)
const changed = await input.current!.query(null)
assert.equal(changed.value, "changed")
assert.equal(inputEvents.filter(event => event.type === "change").length, 1)
await input.current!.command({ type: "select", selection: { start: 1, end: 4 }, expectedRevision: changed.revision })
assert.deepEqual((await input.current!.query(null)).selection, { start: 1, end: 4, reversed: false })
await input.current!.command({ type: "blur" })
await root.unmount()
assert.equal(input.current, null)
assert.equal(document.current, null)
console.log("Installed controls passed: native draw, document search/selection, input revisions/events, list anchor, and unmount")
