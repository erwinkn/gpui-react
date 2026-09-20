import { createElement as h } from "react"
import { describe, expect, it } from "vitest"
import { createRoot, type NativeEvent, type Transaction, type TransactionReply, type Transport } from "@gpui-react/core"
import { Container, Document, Input, List, Text, type Style } from "../src/index.js"

class RecordingTransport implements Transport {
  transactions: Transaction[] = []
  subscribe(_receiver: (event: NativeEvent) => void) { return () => {} }
  async send(encoded: string): Promise<TransactionReply> {
    const transaction: Transaction = JSON.parse(encoded)
    this.transactions.push(transaction)
    return { sequence: transaction.sequence, retired: [], results: [] }
  }
  close() {}
}

describe("the standard controls are ordinary native components", () => {
  it("renders the five kinds under their wire names and interns a shared style once", async () => {
    const transport = new RecordingTransport()
    const root = createRoot(transport)
    const style: Style = { padding: 4, background: "#123456" }
    root.renderSync(<Document>
      <Container style={style}><Text style={style}>hello</Text></Container>
      <List itemCount={3} />
      <Input placeholder="type" />
    </Document>)
    await root.flush()
    const operations = transport.transactions[0].operations
    expect(operations.filter(op => op.op === "style")).toHaveLength(1)
    expect(operations.flatMap(op => op.op === "create" ? [op.component] : [])).toEqual(["document", "container", "text", "list", "input"])
    await root.unmount()
  })

  it("Text sends string and number children as one text prop", async () => {
    const transport = new RecordingTransport()
    const root = createRoot(transport)
    root.renderSync(<Text>count {3}</Text>)
    await root.flush()
    const create = transport.transactions[0].operations.find(op => op.op === "create")
    expect(create && create.op === "create" ? create.props : null).toEqual({ text: "count 3" })
    expect(() => Text({ text: "a", children: "b" })).toThrow(/text or string\/number children/)
    await root.unmount()
  })
})
