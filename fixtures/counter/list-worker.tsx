import assert from "node:assert/strict"
import { createRef, useLayoutEffect } from "react"
import { attachApplication } from "@gpui-react/core/application"
import { Container, List, Text, type ListRef, type TextRef, type ContainerRef } from "@gpui-react/controls"

const root = attachApplication(require("./counter.node"))
const list = createRef<ListRef>()
const first = createRef<TextRef>()
let anchor: Promise<void> | undefined
function Windowed({ start }: { start: number }) {
  useLayoutEffect(() => { anchor = list.current!.command({ type: "scrollTo", index: start + 2, offset: 3 }) }, [start])
  return <Container style={{ width: 320, height: 180 }}>
    <List ref={list} itemCount={100_000} windowStart={start} estimatedItemHeight={20} style={{ width: 320, height: 100 }}>
      {Array.from({ length: 60 }, (_, local) => <Text key={start + local} ref={local === 2 ? first : undefined} measure={local === 2} text={`row ${start + local}`} style={{ height: 20, lineHeight: 20, fontSize: 14 }} />)}
    </List>
  </Container>
}
root.renderSync(<Windowed start={49_998} />)
await root.flush()
await anchor
const identity = list.current!.id
const state = await list.current!.query(null)
assert.deepEqual(state.anchor, { index: 50_000, offset: 3 })
assert.deepEqual(state.supplied, { start: 49_998, end: 50_058 })
const painted = await first.current!.query(null)
assert.equal(painted.text, "row 50000")
assert.equal(painted.painted?.bounds.y, -3, "first native frame must use the layout-effect anchor")
root.renderSync(<Windowed start={59_998} />)
await root.flush()
await anchor
assert.equal(list.current!.id, identity)
assert.deepEqual((await list.current!.query(null)).anchor, { index: 60_000, offset: 3 })
console.log("React list: 100,000 logical rows, 60 supplied rows, first-frame anchor, and atomic window replacement passed")

const container = createRef<ContainerRef>()
const a = createRef<TextRef>()
const b = createRef<TextRef>()
const rows = (reversed: boolean) => <Container ref={container}>{
  (reversed ? ["b", "a"] : ["a", "b"]).map(key => <Text key={key} ref={key === "a" ? a : b} text={key} />)
}</Container>
root.renderSync(rows(false)); await root.flush()
const aId = a.current!.id, bId = b.current!.id
root.renderSync(rows(true)); await root.flush()
assert.equal(a.current!.id, aId); assert.equal(b.current!.id, bId)
assert.equal((await container.current!.query(null)).childCount, 2)
assert.equal((await a.current!.query(null)).text, "a")
assert.equal((await b.current!.query(null)).text, "b")
console.log("React children: keyed moves retain native identity")
await root.unmount()
