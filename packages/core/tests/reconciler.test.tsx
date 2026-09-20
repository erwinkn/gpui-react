import { createElement as h, createRef, Suspense, useLayoutEffect } from "react"
import { describe, expect, it } from "vitest"
import { createRoot, nativeComponent, type NativeEvent, type NativeRef, type Transaction, type TransactionReply, type Transport } from "../src/index.js"

class RecordingTransport implements Transport {
  transactions: Transaction[] = []
  receiver: (event: NativeEvent) => void = () => {}
  closed: string | null = null
  subscribe(receiver: (event: NativeEvent) => void) { this.receiver = receiver; return () => { this.receiver = () => {} } }
  async send(encoded: string): Promise<TransactionReply> {
    const transaction: Transaction = JSON.parse(encoded)
    this.transactions.push(transaction)
    return { sequence: transaction.sequence, retired: [], results: transaction.operations.flatMap(op =>
      op.op === "command" || op.op === "query" ? [{ request: op.request, value: op.op === "query" ? 42 : null }] : []) }
  }
  close(reason: string) { this.closed = reason }
}

const Counter = nativeComponent<{ step: number }, { value: number }>("counter")

describe("React commits to the asynchronous native boundary", () => {
  it("allocates native IDs in committed creation order, independent of render completion order", async () => {
    const transport = new RecordingTransport()
    const root = createRoot(transport)
    root.renderSync(h("box", null, h("box", null, "nested"), <Counter step={1} />))
    await root.flush()
    const ids = transport.transactions[0].operations.flatMap(op => op.op === "create" ? [op.id] : [])
    expect(ids).toEqual([0, 1, 2, 3])
    await root.unmount()
  })
  it("groups committed nodes and layout-effect commands; refs only query asynchronously", async () => {
    const transport = new RecordingTransport()
    const root = createRoot(transport)
    const ref = createRef<NativeRef>()
    let completion: Promise<void> | undefined
    function App() {
      useLayoutEffect(() => { completion = ref.current!.command("focus") }, [])
      return <Counter ref={ref} step={2} />
    }
    root.renderSync(<App />)
    expect(transport.transactions).toHaveLength(0)
    await root.flush()
    await completion
    expect(transport.transactions).toHaveLength(1)
    expect(transport.transactions[0].operations.map(op => op.op)).toEqual(["create", "command"])
    const answer = ref.current!.query("count")
    expect(answer).toBeInstanceOf(Promise)
    await expect(answer).resolves.toBe(42)
    await root.unmount()
    expect(ref.current).toBeNull()
  })

  it("does not publish native instances from an abandoned Suspense render", async () => {
    const transport = new RecordingTransport()
    const root = createRoot(transport)
    const never = new Promise(() => {})
    function Pending(): never { throw never }
    root.renderSync(<Suspense fallback={h("text", { text: "loading" })}>
      <Counter step={99} /><Pending />
    </Suspense>)
    await root.flush()
    const created = transport.transactions.flatMap(t => t.operations).filter(op => op.op === "create")
    expect(created.map(op => op.component)).toEqual(["text"])
    await root.unmount()
  })

  it("keeps keyed identities through moves and removes host text explicitly", async () => {
    const transport = new RecordingTransport()
    const root = createRoot(transport)
    const list = (keys: string[], text: boolean) => h("box", null, ...keys.map(key => h(Counter, { key, step: 1 })), text ? "tail" : null)
    root.renderSync(list(["a", "b"], true))
    await root.flush()
    const text = transport.transactions[0].operations.find(op => op.op === "create" && op.component === "text")!
    root.renderSync(list(["b", "a"], false))
    await root.flush()
    expect(transport.transactions[1].operations.some(op => op.op === "create")).toBe(false)
    expect(transport.transactions[1].operations.some(op => op.op === "place")).toBe(true)
    expect(transport.transactions[1].operations).toContainEqual({ op: "remove", id: "id" in text ? text.id : 0 })
    await root.unmount()
  })

  it("keeps callback versions until native retirement and checks target identity", async () => {
    const transport = new RecordingTransport()
    const root = createRoot(transport)
    const values: string[] = []
    root.renderSync(<Counter step={1} onEvent={() => values.push("old")} />)
    await root.flush()
    const created = transport.transactions[0].operations.find(op => op.op === "create")!
    if (created.op !== "create") throw Error("missing create")
    root.renderSync(<Counter step={2} onEvent={() => values.push("new")} />)
    await root.flush()
    const listen = transport.transactions[1].operations.find(op => op.op === "listen")!
    if (listen.op !== "listen") throw Error("missing listen")
    transport.receiver({ target: created.id, subscription: created.subscription!, payload: {} })
    transport.receiver({ target: created.id, subscription: listen.subscription!, payload: {} })
    transport.receiver({ target: created.id + 100, subscription: listen.subscription!, payload: {} })
    expect(values).toEqual(["old", "new"])
    await root.unmount()
  })

  it("does not reset IDs by attaching a new root to an old session", async () => {
    const transport = new RecordingTransport()
    const root = createRoot(transport)
    expect(() => createRoot(transport)).toThrow("already has")
    await root.unmount()
    expect(() => createRoot(transport)).toThrow("already has")
  })

  it("does not resend native props when only a JavaScript callback changes", async () => {
    const transport = new RecordingTransport()
    const root = createRoot(transport)
    root.renderSync(<Counter step={2} onEvent={() => {}} />)
    await root.flush()
    root.renderSync(<Counter step={2} onEvent={() => {}} />)
    await root.flush()
    expect(transport.transactions[1].operations.map(op => op.op)).toEqual(["listen"])
    await root.unmount()
  })

  it("rejects flush after overflow even if an earlier native request is stalled", async () => {
    const transport = new RecordingTransport()
    transport.send = () => new Promise(() => {})
    const root = createRoot(transport, { maxPending: 1, onError: () => {} })
    root.renderSync(<Counter step={1} />)
    await new Promise(resolve => setTimeout(resolve, 0))
    root.renderSync(<Counter step={2} />)
    await new Promise(resolve => setTimeout(resolve, 0))
    const result = await Promise.race([
      root.flush().then(() => "resolved", error => error.message),
      new Promise(resolve => setTimeout(() => resolve("stalled"), 30)),
    ])
    expect(result).toContain("queue is full")
  })

  it("stops explicitly on queue overflow instead of silently losing a React commit", async () => {
    const transport = new RecordingTransport()
    let finish: ((value: TransactionReply) => void) | undefined
    transport.send = async encoded => {
      transport.transactions.push(JSON.parse(encoded))
      return new Promise(resolve => { finish = resolve })
    }
    const errors: Error[] = []
    const root = createRoot(transport, { maxPending: 1, onError: error => errors.push(error) })
    root.renderSync(<Counter step={1} />)
    await new Promise(resolve => setTimeout(resolve, 0))
    root.renderSync(<Counter step={2} />)
    await new Promise(resolve => setTimeout(resolve, 0))
    expect(errors[0].message).toContain("queue is full")
    expect(transport.closed).toContain("queue is full")
    finish!({ sequence: 1, retired: [], results: [] })
    await expect(root.flush()).rejects.toThrow("queue is full")
  })

  it("rejects non-data native props before sending them", async () => {
    const transport = new RecordingTransport()
    const errors: Error[] = []
    const root = createRoot(transport, { onError: error => errors.push(error) })
    root.renderSync(h("counter", { step: Number.NaN }))
    await expect(root.flush()).rejects.toThrow("JSON data")
    expect(transport.transactions).toHaveLength(0)
    expect(errors).toHaveLength(1)
  })

  it("rejects a missing command result instead of leaving the caller pending", async () => {
    const transport = new RecordingTransport()
    const errors: Error[] = []
    const root = createRoot(transport, { onError: error => errors.push(error) })
    const ref = createRef<NativeRef>()
    root.renderSync(<Counter step={1} ref={ref} />)
    await root.flush()
    transport.send = async encoded => ({ sequence: JSON.parse(encoded).sequence, retired: [], results: [] })
    await expect(ref.current!.command("increment")).rejects.toThrow("Missing native request result")
    expect(errors).toHaveLength(1)
  })
})

describe("Native component schema", () => {
  it("keys kinds by name in the order native declared them", () => {
    const transport = new RecordingTransport()
    const capabilities = { events: false, commands: false, queries: false, children: false, view: false }
    const root = createRoot(transport, { schema: [
      { name: "document", capabilities, fields: null },
      { name: "text", capabilities, fields: [{ name: "text", type: "str", required: false }] },
    ] })
    expect([...root.kinds!.entries()]).toEqual([["document", 0], ["text", 1]])
    root.dispose()
    expect(createRoot(new RecordingTransport()).kinds).toBeNull()
  })
  it("renders string children with the textKind root option and names it when the schema lacks the kind", async () => {
    const capabilities = { events: false, commands: false, queries: false, children: false, view: false }
    const schema = [{ name: "label", capabilities, fields: [{ name: "text", type: "str" as const, required: false }] }]
    const transport = new RecordingTransport()
    const root = createRoot(transport, { schema, textKind: "label" })
    root.renderSync(h("label", null, "hello"))
    await root.flush()
    expect(transport.transactions[0].operations.flatMap(op => op.op === "create" ? [[op.component, op.props]] : [])).toEqual([["label", {}], ["label", { text: "hello" }]])
    await root.unmount()

    const errors: Error[] = []
    const failing = createRoot(new RecordingTransport(), { schema, onError: error => errors.push(error) })
    expect(failing.textKind).toBe("text")
    failing.renderSync(h("label", null, "hello"))
    await expect(failing.flush()).rejects.toThrow(/native kind "text".*textKind root option/)
    const plain = createRoot(new RecordingTransport())
    plain.renderSync(h("box", null, "hello"))
    await plain.flush()
    await plain.unmount()
  })
})
