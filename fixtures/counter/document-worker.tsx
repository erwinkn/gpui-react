import assert from "node:assert/strict"
import { createRef } from "react"
import { attachApplication } from "@gpui-react/core/application"
import { Document, Text, type DocumentRef, type DocumentEvent } from "@gpui-react/kit"

const bindings = require("./counter.node")
const root = attachApplication(bindings)
const document = createRef<DocumentRef>()
const events: DocumentEvent[] = []
const name = "😀"
root.renderSync(<Document ref={document} search={{ query: "token", activeIndex: 0 }} onEvent={event => events.push(event)} style={{ color: "white", width: 300, lineHeight: 24 }}>
  <Text textKey="greeting">Hello {name}! token</Text>
  <Text textKey="content" text="another token" />
  <Text textKey="chrome" selectable={false}>chrome token</Text>
</Document>)
await root.flush()
const initial = await document.current!.query(null)
assert.ok(initial.frame, "the first native frame must register document content")
assert.deepEqual(initial.text.map(text => text.text), ["Hello 😀! token", "another token", "chrome token"])
assert.equal(initial.matchCount, 3)
assert.equal(initial.query?.query, "token")
assert.ok(events.some(event => event.type === "search" && event.count === 3 && event.query?.query === "token" && !!event.frame))
await document.current!.command({ type: "select", start: { key: "greeting", offset: 6 }, end: { key: "greeting", offset: 8 }, expectedContentRevision: initial.contentRevision })
assert.equal((await document.current!.query(null)).selection, "😀")
await document.current!.command({ type: "selectAll" })
assert.equal((await document.current!.query(null)).selection, "Hello 😀! token\nanother token")
assert.ok(events.some(event => event.type === "selection" && event.hasSelection))
await document.current!.command({ type: "clear" })
assert.equal((await document.current!.query(null)).selection, null)
console.log("React document: one native interpolated text, search, versioned UTF-16 selection, and events passed")
await root.unmount()
