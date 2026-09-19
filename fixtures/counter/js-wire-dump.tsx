// Writes the frame-cost mount transaction as the bridge seals it, on both
// wires, for the native fixture and tests: <dir>/mount-<scene>-<rows>.{json,bin}.
// Args: dir rows scene. Env: BRIDGE_SCHEMA (default /tmp/gpui-react-wire/schema.json).
import { createRoot, type KindSchema, type Transport, type NativeEvent, type TransactionReply } from "@gpui-react/core"
import { Container, Document, List, Text } from "@gpui-react/core"
import assert from "node:assert/strict"
import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"

const DIR = process.argv[2] ?? "/tmp/gpui-react-wire"
const ROWS = Number(process.argv[3] ?? 5000)
const SCENE = process.argv[4] ?? "flow"
const WIDTH = 800, HEIGHT = 600, HEADER = 32, ROW = 20
const schema: KindSchema[] = JSON.parse(readFileSync(process.env.BRIDGE_SCHEMA ?? "/tmp/gpui-react-wire/schema.json", "utf8"))

class Capture implements Transport {
  payloads: (string | Uint8Array)[] = []
  send(transaction: string | Uint8Array): Promise<TransactionReply> {
    this.payloads.push(typeof transaction === "string" ? transaction : transaction.slice())
    return Promise.resolve({ sequence: this.payloads.length, retired: [], results: [] })
  }
  subscribe(_receiver: (event: NativeEvent) => void): () => void { return () => {} }
  close(): void {}
}
function rowText(ix: number) { return `Row ${String(ix).padStart(5, "0")}: retained native content for the frame comparison` }
function Scene({ rows }: { rows: number }) {
  const items = Array.from({ length: rows }, (_, ix) => <Text key={ix} text={rowText(ix)} style={{ width: WIDTH, height: ROW, shrink: 0 }} />)
  return <Document style={{ width: WIDTH, height: HEIGHT, fontSize: 14, lineHeight: ROW, color: "white", background: "#101010" }}>
    <Text text="Status 0" style={{ width: WIDTH, height: HEADER, shrink: 0 }} />
    {SCENE === "list"
      ? <List estimatedItemHeight={ROW} style={{ width: WIDTH, height: HEIGHT - HEADER, shrink: 0 }}>{items}</List>
      : <Container scroll="y" style={{ width: WIDTH, height: HEIGHT - HEADER, shrink: 0 }}>{items}</Container>}
  </Document>
}
mkdirSync(DIR, { recursive: true })
for (const wire of ["json", "binary"] as const) {
  const transport = new Capture()
  const root = createRoot(transport, { schema, wire })
  root.renderSync(<Scene rows={ROWS} />)
  await root.flush()
  assert.equal(transport.payloads.length, 1)
  root.dispose()
  writeFileSync(join(DIR, `mount-${SCENE}-${ROWS}.${wire === "json" ? "json" : "bin"}`), transport.payloads[0]!)
}
console.log(join(DIR, `mount-${SCENE}-${ROWS}.{json,bin}`))
