import { createElement as h, useLayoutEffect } from "react"
import { expect, it } from "vitest"
import { createRoot } from "../src/index.js"
import { NativeTransport, type Client } from "../src/application.js"

class ClientDouble implements Client {
  sent: string[] = []
  closed?: string
  waiter?: { resolve: (messages: string[]) => void; reject: (error: Error) => void }
  send(transaction: string) { this.sent.push(transaction) }
  receive(): Promise<string[]> { return new Promise((resolve, reject) => { this.waiter = { resolve, reject } }) }
  close(reason: string) { this.closed = reason; this.waiter?.reject(Error(reason)) }
  deliver(messages: unknown[]) { const waiter = this.waiter!; this.waiter = undefined; waiter.resolve(messages.map(message => JSON.stringify(message))) }
}

it("delivers native events before the acknowledgement that retires their callback", async () => {
  const client = new ClientDouble()
  const root = createRoot(new NativeTransport(client))
  const values: unknown[] = []
  root.renderSync(h("counter", { onEvent: (value: unknown) => values.push(value) }))
  const flush = root.flush()
  await Promise.resolve()
  const transaction = JSON.parse(client.sent[0])
  const create = transaction.operations.find((op: { op: string }) => op.op === "create")
  client.deliver([
    { event: { target: create.id, subscription: create.subscription, payload: 7 } },
    { reply: { sequence: 1, retired: [create.subscription], results: [] } },
  ])
  await flush
  expect(values).toEqual([7])
  client.deliver([{ event: { target: create.id, subscription: create.subscription, payload: 8 } }])
  await Promise.resolve()
  expect(values).toEqual([7])
  root.dispose()
})

it("reports an idle native disconnect and still runs React cleanup", async () => {
  const client = new ClientDouble()
  const errors: Error[] = []
  const root = createRoot(new NativeTransport(client), { onError: error => errors.push(error) })
  let cleanups = 0
  function App() { useLayoutEffect(() => () => { cleanups++ }, []); return h("counter") }
  root.renderSync(<App />)
  const flushed = root.flush()
  await Promise.resolve()
  client.deliver([{ reply: { sequence: 1, retired: [], results: [] } }])
  await flushed
  client.waiter!.reject(Error("native disconnected"))
  await Promise.resolve()
  expect(errors[0].message).toBe("native disconnected")
  await expect(root.flush()).rejects.toThrow("native disconnected")
  root.dispose()
  expect(cleanups).toBe(1)
  expect(client.sent).toHaveLength(1)
})

it("rejects outstanding transport work when closed", async () => {
  const client = new ClientDouble()
  const transport = new NativeTransport(client)
  transport.subscribe(() => {})
  const result = transport.send("transaction")
  transport.close("closed by test")
  await expect(result).rejects.toThrow("closed by test")
  await expect(transport.send("later")).rejects.toThrow("closed by test")
})
