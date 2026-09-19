// How much of the worker heap after mount is the bridge's own host
// records versus React's fiber tree. Bun's heap snapshot names every object
// "Object", so attribution is by construction: build the same number of
// host-shaped records in isolation and measure their heap delta.
import { heapStats } from "bun:jsc"
const ROWS = Number(process.argv[2] ?? 5000)
function heap(): number { Bun.gc(true); return process.memoryUsage().heapUsed }
function counts() { Bun.gc(true); const c = heapStats().objectTypeCounts; return { Object: c.Object ?? 0, Array: c.Array ?? 0, string: c.string ?? 0, total: heapStats().objectCount } }
// Shape of `Host` in packages/bridge/src/index.ts: 8 fields, one `initial` array.
type Host = { id: number; component: string; props: object; root: object; initial: Host[]; mounted: boolean; subscription: number | null; public: null }
const root = {}
const propsShared = Array.from({ length: ROWS + 3 }, () => ({ text: "x", style: 1 })) // stands in for React's props objects (owned by React, not us)
let keep: Host[] = []
const before = heap(), beforeCounts = counts()
for (let i = 0; i < ROWS + 3; i++) keep.push({ id: i, component: "text", props: propsShared[i]!, root, initial: [], mounted: true, subscription: null, public: null })
const after = heap(), afterCounts = counts()
const idMap = new Map<number, Host>(); for (const h of keep) idMap.set(h.id, h)
const withMap = heap()
console.log(JSON.stringify({ rows: ROWS, hostRecords: keep.length, hostRecordBytes: after - before, perRecord: +((after - before) / keep.length).toFixed(1), idMapBytes: withMap - after, objectsAdded: afterCounts.Object - beforeCounts.Object, arraysAdded: afterCounts.Array - beforeCounts.Array }))
keep = []
