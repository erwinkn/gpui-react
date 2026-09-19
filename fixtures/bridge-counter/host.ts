import { runApplication } from "@gpuix/bridge/application"
const bindings = require("./counter.node")
const mode = process.env.BRIDGE_COUNTER_MODE ?? ""
const entries: Record<string, string> = {
  missing: "./missing-worker.tsx", list: "./list-worker.tsx", document: "./document-worker.tsx",
  interaction: "./interaction-worker.tsx", gpu: "./gpu-worker.tsx",
}
for (let attempt = 0; attempt < (process.env.BRIDGE_COUNTER_MODE === "repeat" ? 3 : 1); attempt++) {
  await runApplication(bindings, new URL(entries[mode] ?? "./worker.tsx", import.meta.url), { title: "React GPUI bridge check", width: 320, height: 180, show: process.env.BRIDGE_COUNTER_MODE !== "interaction" })
}
console.log("Native counter worker completed")
