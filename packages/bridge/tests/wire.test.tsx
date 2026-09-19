import { createElement as h, createRef } from "react"
import { afterEach, describe, expect, it, vi } from "vitest"
import { createRoot, decodeWire, nativeComponent, type KindSchema, type NativeEvent, type NativeRef, type Transaction, type TransactionReply, type Transport } from "../src/index.js"

const caps = { events: true, commands: true, queries: true, children: true, view: false }
const schema: KindSchema[] = [
  { name: "box", capabilities: caps, fields: null },
  { name: "text", capabilities: caps, fields: [{ name: "text", type: "str", required: false }, { name: "style", type: "style", required: false }] },
  { name: "label", capabilities: caps, fields: [
    { name: "flag", type: "bool", required: false },
    { name: "count", type: "u32", required: true },
    { name: "delta", type: "i32", required: false },
    { name: "ratio", type: "f32", required: false },
    { name: "wide", type: "f64", required: false },
    { name: "title", type: "str", required: false },
    { name: "style", type: "style", required: false },
    { name: "tags", type: "value", required: false },
  ] },
]
/** Decodes every payload at once: a binary view is only valid until send settles. */
class Recording implements Transport {
  transactions: Transaction[] = []
  bytes: number[] = []
  constructor(readonly schema: KindSchema[]) {}
  subscribe(_: (event: NativeEvent) => void) { return () => {} }
  async send(encoded: string | Uint8Array): Promise<TransactionReply> {
    const transaction = typeof encoded === "string" ? JSON.parse(encoded) as Transaction : decodeWire(encoded, this.schema)
    this.bytes.push(typeof encoded === "string" ? Buffer.byteLength(encoded) : encoded.byteLength)
    this.transactions.push(transaction)
    return { sequence: transaction.sequence, retired: [], results: transaction.operations.flatMap(op =>
      op.op === "command" || op.op === "query" ? [{ request: op.request, value: op.op === "query" ? 42 : null }] : []) }
  }
  close() {}
}
const Label = nativeComponent<{ flag?: boolean; count: number; delta?: number; ratio?: number; wide?: number; title?: string; style?: object; tags?: unknown; extra?: unknown }, unknown, string, string, number>("label")
const Text = nativeComponent<{ text: string; style?: object }>("text")
const roots = (wire: "json" | "binary", checks?: boolean) => {
  const transport = new Recording(schema)
  return { transport, root: createRoot(transport, { schema, wire, wireChecks: checks, onError: () => {} }) }
}
afterEach(() => vi.restoreAllMocks())

describe("The binary wire", () => {
  it("carries the same operations as JSON for creates, updates, styles, moves, removals, and calls", async () => {
    const style = { width: 10, color: "red" }
    const ref = createRef<NativeRef<string, string, number>>()
    const scene = (n: number) => h("box", { style, key: "b" }, ...Array.from({ length: n }, (_, i) =>
      h(Label, { key: i, flag: i % 2 === 0, count: i, delta: -i, ratio: 1.5, wide: 1e-9, title: `row ${i}`, style, tags: ["a", { b: i }], ref: i === 0 ? ref : undefined })),
      h(Text, { key: "t", text: "hello", style: { width: 1 } }))
    const results: Record<string, Transaction[]> = {}
    const sizes: Record<string, number[]> = {}
    for (const wire of ["json", "binary"] as const) {
      const { transport, root } = roots(wire)
      root.renderSync(scene(3))
      await root.flush()
      await ref.current!.command("go")
      expect(await ref.current!.query("?")).toBe(42)
      root.renderSync(scene(2))
      await root.flush()
      root.renderSync(h("box", { style, key: "b" }, h(Label, { key: 1, count: 1 }), h(Label, { key: 0, count: 0 })))
      await root.flush()
      await root.unmount()
      results[wire] = transport.transactions
      sizes[wire] = transport.bytes
    }
    expect(results.binary).toEqual(results.json)
    expect(results.json!.length).toBeGreaterThanOrEqual(5)
    expect(sizes.binary!.reduce((a, b) => a + b)).toBeLessThan(sizes.json!.reduce((a, b) => a + b))
    const first = results.binary![0]!.operations
    expect(first.filter(op => op.op === "style")).toHaveLength(2)
    expect(first.find(op => op.op === "create" && op.component === "label")).toMatchObject({ props: { flag: true, count: 0, delta: 0, ratio: 1.5, wide: 1e-9, title: "row 0", style: 0, tags: ["a", { b: 0 }] }, parent: 0 })
  })

  it("skips an update whose native fields did not change", async () => {
    const { transport, root } = roots("binary")
    const onEvent = () => {}
    const render = (title: string, tags: unknown) => root.renderSync(h(Label, { count: 1, title, tags, style: { width: 5 }, onEvent }))
    render("a", [1])
    await root.flush()
    render("a", [1])
    await root.flush()
    expect(transport.transactions).toHaveLength(1)
    render("b", [1])
    await root.flush()
    expect(transport.transactions[1]!.operations).toEqual([{ op: "props", id: 0, component: "label", props: { count: 1, title: "b", tags: [1], style: 0 } }])
    await root.unmount()
  })

  it("fails the root on a missing required prop or a wrong type, naming the field", async () => {
    // React reports commit errors through the root, so the failure surfaces at flush.
    for (const [props, message] of [[{ title: "x" }, "label is missing required props: count"], [{ count: "1" }, "label.count must be an unsigned 32-bit integer"], [{ count: 1.5 }, "label.count must be an unsigned 32-bit integer"], [{ count: 1, ratio: Infinity }, "label.ratio must be a finite number"], [{ count: 1, title: 3 }, "label.title must be a string"]] as const) {
      const { transport, root } = roots("binary", true)
      root.renderSync(h(Label, props as any))
      await expect(root.flush()).rejects.toThrow(message)
      expect(transport.transactions).toHaveLength(0)
      root.dispose()
    }
  })

  it("rounds and clamps integers when checks are off", async () => {
    const { transport, root } = roots("binary", false)
    root.renderSync(h(Label, { count: 1.9, delta: -2.5 }))
    await root.flush()
    expect(transport.transactions[0]!.operations[0]).toMatchObject({ props: { count: 1, delta: -2 } })
    await root.unmount()
  })

  it("warns once per component and prop about a prop native does not declare", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {})
    const { transport, root } = roots("binary")
    root.renderSync(h("box", null, h(Label, { key: 1, count: 1, extra: 1 }), h(Label, { key: 2, count: 2, extra: 2 })))
    await root.flush()
    expect(warn).toHaveBeenCalledTimes(1)
    expect(warn.mock.calls[0]![0]).toContain('label has no prop "extra"')
    expect(transport.transactions[0]!.operations.find(op => op.op === "create" && op.component === "label")).toMatchObject({ props: { count: 1 } })
    await root.unmount()
  })

  it("keeps each sealed payload intact until its send settles, across buffer reuse", async () => {
    // Send N settles after the next commit has been written, so the buffer
    // pool must hand the next transaction a different allocation.
    const seen: Transaction[] = []
    const transport: Transport = {
      subscribe: () => () => {},
      send: (encoded: string | Uint8Array) => new Promise(resolve => setTimeout(() => {
        const transaction = decodeWire(encoded as Uint8Array, schema)
        seen.push(transaction)
        resolve({ sequence: transaction.sequence, retired: [], results: [] })
      }, 2)),
      close() {},
    }
    const root = createRoot(transport, { schema, wire: "binary" })
    for (const title of ["first", "second", "third", "fourth"]) {
      root.renderSync(h(Label, { count: 1, title }))
      root.render(h(Label, { count: 1, title: title + " again" }))
      await new Promise(resolve => setTimeout(resolve, 0))
    }
    await root.flush()
    expect(seen.map(t => t.operations.map(op => op.op === "create" || op.op === "props" ? op.props.title : op.op))).toEqual([
      ["first"], ["first again"], ["second"], ["second again"], ["third"], ["third again"], ["fourth"], ["fourth again"],
    ])
    await root.unmount()
  })

  it("rejects the binary wire without a schema and an unknown component with one", async () => {
    expect(() => createRoot(new Recording(schema), { wire: "binary" })).toThrow("needs the native component schema")
    const { root } = roots("json")
    root.renderSync(h("nope", null))
    await expect(root.flush()).rejects.toThrow("Unknown native component nope")
    root.dispose()
  })
})
