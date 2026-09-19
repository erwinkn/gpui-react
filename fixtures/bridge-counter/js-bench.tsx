// Worker-side cost of the frame-cost scenes: React render and commit, sealing
// to JSON, and the wire size. No native code runs; the transport records.
import { createRoot, type Transport, type NativeEvent, type TransactionReply } from "@gpui-react/core"
import { Container, Document, List, Text } from "@gpui-react/controls"
import { memo } from "react"

const ROWS = Number(process.argv[2] ?? 5000)
const SCENE = process.argv[3] ?? "flow"
const REPEATS = Number(process.argv[4] ?? 5)
const WIDTH = 800, HEIGHT = 600, HEADER = 32, ROW = 20

class Recording implements Transport {
  sent: { at: number; bytes: number; ops: number }[] = []
  plainStringifyMs = 0
  send(transaction: string): Promise<TransactionReply> {
    const at = performance.now()
    const parsed = JSON.parse(transaction)
    if (parsed.operations.length > 100) { const t = performance.now(); JSON.stringify(parsed); this.plainStringifyMs = performance.now() - t }
    this.sent.push({ at, bytes: Buffer.byteLength(transaction), ops: parsed.operations.length })
    return Promise.resolve({ sequence: parsed.sequence, retired: [], results: [] })
  }
  subscribe(_receiver: (event: NativeEvent) => void): () => void { return () => {} }
  close(): void {}
}

const MEMO = process.env.BENCH_MEMO === "1"
const Row = memo(function Row({ ix }: { ix: number }) {
  return <Text text={rowText(ix)} style={{ width: WIDTH, height: ROW, shrink: 0 }} />
})
function rowText(ix: number) { return `Row ${String(ix).padStart(5, "0")}: retained native content for the frame comparison` }
function Scene({ status, rows }: { status: string; rows: number }) {
  const items = Array.from({ length: rows }, (_, ix) => MEMO
    ? <Row key={ix} ix={ix} />
    : <Text key={ix} text={rowText(ix)} style={{ width: WIDTH, height: ROW, shrink: 0 }} />)
  return <Document style={{ width: WIDTH, height: HEIGHT, fontSize: 14, lineHeight: ROW, color: "white", background: "#101010" }}>
    <Text text={status} style={{ width: WIDTH, height: HEADER, shrink: 0 }} />
    {SCENE === "list"
      ? <List estimatedItemHeight={ROW} style={{ width: WIDTH, height: HEIGHT - HEADER, shrink: 0 }}>{items}</List>
      : <Container scroll="y" style={{ width: WIDTH, height: HEIGHT - HEADER, shrink: 0 }}>{items}</Container>}
  </Document>
}

type Phase = { render: number; seal: number; bytes: number; ops: number }
async function commit(root: ReturnType<typeof createRoot>, transport: Recording, node: React.ReactNode): Promise<Phase> {
  const before = transport.sent.length
  const start = performance.now()
  root.renderSync(node)
  const rendered = performance.now()
  await root.flush()
  const sent = transport.sent[before]
  if (!sent || transport.sent.length !== before + 1) throw Error(`expected one transaction, got ${transport.sent.length - before}`)
  return { render: rendered - start, seal: sent.at - rendered, bytes: sent.bytes, ops: sent.ops }
}
function heap(): number { Bun.gc(true); return process.memoryUsage().heapUsed }
function median(values: number[]): number { const s = [...values].sort((a, b) => a - b); return s[s.length >> 1]! }

const mounts: Phase[] = [], updates: Phase[] = [], removals: Phase[] = []
let heapAfterMount = 0, heapBefore = 0, plainStringify: number[] = []
for (let repeat = 0; repeat < REPEATS; repeat++) {
  const transport = new Recording()
  const root = createRoot(transport)
  heapBefore = heap()
  mounts.push(await commit(root, transport, <Scene status="Status 0" rows={ROWS} />))
  plainStringify.push(transport.plainStringifyMs)
  heapAfterMount = heap()
  for (let i = 0; i < 20; i++) updates.push(await commit(root, transport, <Scene status={i % 2 ? "Status 0" : "Status 1"} rows={ROWS} />))
  removals.push(await commit(root, transport, null))
  root.dispose()
}
const summarize = (phases: Phase[]) => ({
  renderMs: +median(phases.map(p => p.render)).toFixed(3),
  sealMs: +median(phases.map(p => p.seal)).toFixed(3),
  bytes: median(phases.map(p => p.bytes)),
  operations: median(phases.map(p => p.ops)),
})
console.log(JSON.stringify({
  scene: SCENE, rows: ROWS, repeats: REPEATS, bun: Bun.version, memoRows: MEMO,
  plainStringifyMs: +median(plainStringify).toFixed(3),
  mount: summarize(mounts), update: summarize(updates), remove: summarize(removals),
  jsHeapAfterMountBytes: heapAfterMount - heapBefore,
}))
