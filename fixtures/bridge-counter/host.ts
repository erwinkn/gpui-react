import { runApplication } from "@gpuix/bridge/application"
const bindings = require("./counter.node")
for (let attempt = 0; attempt < (process.env.BRIDGE_COUNTER_MODE === "repeat" ? 3 : 1); attempt++) {
  await runApplication(bindings, new URL(process.env.BRIDGE_COUNTER_MODE === "missing" ? "./missing-worker.tsx" : process.env.BRIDGE_COUNTER_MODE === "list" ? "./list-worker.tsx" : process.env.BRIDGE_COUNTER_MODE === "interaction" ? "./interaction-worker.tsx" : "./worker.tsx", import.meta.url), { title: "React GPUI bridge check", width: 320, height: 180, show: process.env.BRIDGE_COUNTER_MODE !== "interaction" })
}
console.log("Native counter worker completed")
