// Worker-side cost of the frame-cost scenes: React render and commit, sealing
// to the wire, and the wire size. No native code runs; the transport records.
// Args: rows scene repeats. Env: BRIDGE_WIRE=json|binary (default json),
// BRIDGE_WIRE_CHECKS=0|1 (binary number checks; default on),
// BRIDGE_SCHEMA=<path to the kind table JSON, from `gpui-react-frame-cost schema`>,
// BENCH_MEMO=1, BENCH_HOIST=1 (row style hoisted to module scope), BENCH_UPDATES=<n>,
// BENCH_NO_PARSE=1 (the transport does not decode),
// BENCH_TRACE=1 (heap before and after each mount), BENCH_BALLAST=<MB> (retained
// objects that raise the collector's threshold; sub-millisecond differences between
// wires are otherwise dominated by where collections land).
// Scene "numbers" is synthetic: a list per row with numeric props.
import { createRoot, decodeWire, type KindSchema, type Transport, type NativeEvent, type TransactionReply } from "@gpui-react/core"
import { Container, Document, List, Text } from "@gpui-react/core"
import { readFileSync } from "node:fs"
import { heapStats } from "bun:jsc"
import { memo } from "react"

const ROWS = Number(process.argv[2] ?? 5000)
const SCENE = process.argv[3] ?? "flow"
const REPEATS = Number(process.argv[4] ?? 5)
const WIDTH = 800, HEIGHT = 600, HEADER = 32, ROW = 20
const WIRE = (process.env.BRIDGE_WIRE ?? "json") as "json" | "binary"
const CHECKS = process.env.BRIDGE_WIRE_CHECKS !== "0"
const schema: KindSchema[] | undefined = WIRE !== "json" ? JSON.parse(readFileSync(process.env.BRIDGE_SCHEMA ?? "/tmp/gpui-react-wire/schema.json", "utf8")) : undefined

class Recording implements Transport {
  sent: { at: number; bytes: number; ops: number }[] = []
  plainStringifyMs = 0
  sequence = 0
  send(transaction: string | Uint8Array): Promise<TransactionReply> {
    const at = performance.now()
    // BENCH_NO_PARSE=1: do not decode on the transport, so the bench itself
    // creates no garbage between commits; the operation count is then unknown.
    if (process.env.BENCH_NO_PARSE === "1") {
      this.sent.push({ at, bytes: typeof transaction === "string" ? transaction.length : transaction.byteLength, ops: -1 })
      return Promise.resolve({ sequence: ++this.sequence, retired: [], results: [] })
    }
    const parsed = typeof transaction === "string" ? JSON.parse(transaction) : decodeWire(transaction, schema!)
    if (typeof transaction === "string" && parsed.operations.length > 100) { const t = performance.now(); JSON.stringify(parsed); this.plainStringifyMs = performance.now() - t }
    this.sent.push({ at, bytes: typeof transaction === "string" ? Buffer.byteLength(transaction) : transaction.byteLength, ops: parsed.operations.length })
    return Promise.resolve({ sequence: parsed.sequence, retired: [], results: [] })
  }
  subscribe(_receiver: (event: NativeEvent) => void): () => void { return () => {} }
  close(): void {}
}

const MEMO = process.env.BENCH_MEMO === "1"
// BENCH_HOIST=1: style objects hoisted to module scope, so the identity cache
// hits and no style is keyed by JSON.stringify during the mount.
const HOIST = process.env.BENCH_HOIST === "1"
const ROW_STYLE = { width: WIDTH, height: ROW, shrink: 0 }
const rowStyle = () => HOIST ? ROW_STYLE : { width: WIDTH, height: ROW, shrink: 0 }
const Row = memo(function Row({ ix }: { ix: number }) {
  return <Text text={rowText(ix)} style={rowStyle()} />
})
function rowText(ix: number) { return `Row ${String(ix).padStart(5, "0")}: retained native content for the frame comparison` }
function Scene({ status, rows }: { status: string; rows: number }) {
  const items = Array.from({ length: rows }, (_, ix) => MEMO
    ? <Row key={ix} ix={ix} />
    : <Text key={ix} text={rowText(ix)} style={rowStyle()} />)
  if (SCENE === "numbers") {
    const lists = Array.from({ length: rows }, (_, ix) =>
      <List key={ix} itemCount={ix} windowStart={ix % 7} estimatedItemHeight={ROW + ix % 3} overdraw={ix % 5} followTail={ix % 2 === 0} style={{ width: WIDTH, height: ROW, shrink: 0 }} />)
    return <Container scroll="y" style={{ width: WIDTH, height: HEIGHT, shrink: 0 }}>
      <Text text={status} style={{ width: WIDTH, height: HEADER, shrink: 0 }} />
      {lists}
    </Container>
  }
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
function median(values: number[]): number { const s = [...values].sort((a, b) => a - b); return s[s.length >> 1] ?? 0 }

// BENCH_BALLAST=<MB>: retained live objects, to raise the heap capacity and
// with it the collector's allocation threshold, so GC pacing can be controlled.
const ballast: object[] = []
for (let i = 0; i < Number(process.env.BENCH_BALLAST ?? 0) * 20000; i++) ballast.push({ i, s: "ballast " + i })
if (process.env.BENCH_TRACE === "1") console.error("ballast objects", ballast.length, "env", process.env.BENCH_BALLAST)
const mounts: Phase[] = [], updates: Phase[] = [], removals: Phase[] = []
let heapAfterMount = 0, heapBefore = 0, plainStringify: number[] = []
for (let repeat = 0; repeat < REPEATS; repeat++) {
  const transport = new Recording()
  const root = createRoot(transport, { schema, wire: WIRE, wireChecks: CHECKS })
  heapBefore = heap()
  const statsBefore = process.env.BENCH_TRACE === "1" ? heapStats() : null
  mounts.push(await commit(root, transport, <Scene status="Status 0" rows={ROWS} />))
  if (statsBefore) {
    const after = heapStats()
    console.error(`repeat ${repeat}: before mount heapSize ${(statsBefore.heapSize / 1e6).toFixed(2)} MB capacity ${(statsBefore.heapCapacity / 1e6).toFixed(2)} MB objects ${statsBefore.objectCount}; render ${mounts.at(-1)!.render.toFixed(2)} seal ${mounts.at(-1)!.seal.toFixed(2)}; after heapSize ${(after.heapSize / 1e6).toFixed(2)} MB`)
  }
  plainStringify.push(transport.plainStringifyMs)
  heapAfterMount = heap()
  // BENCH_UPDATES=<n>: update commits per repeat (default 20); 0 isolates the mount for profiling.
  for (let i = 0; i < Number(process.env.BENCH_UPDATES ?? 20); i++) updates.push(await commit(root, transport, <Scene status={i % 2 ? "Status 0" : "Status 1"} rows={ROWS} />))
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
  scene: SCENE, rows: ROWS, repeats: REPEATS, bun: Bun.version, memoRows: MEMO, hoistedStyles: HOIST, wire: WIRE, wireChecks: CHECKS,
  plainStringifyMs: +median(plainStringify).toFixed(3),
  // The first mount in the process: what an application pays before the JIT is warm.
  coldMount: { renderMs: +mounts[0]!.render.toFixed(3), sealMs: +mounts[0]!.seal.toFixed(3) },
  mount: summarize(mounts), update: summarize(updates), remove: summarize(removals),
  jsHeapAfterMountBytes: heapAfterMount - heapBefore,
  ballastObjects: ballast.length,
}))
