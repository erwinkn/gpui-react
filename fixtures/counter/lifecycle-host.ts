import { runApplication } from "@gpui-react/core/application"
import { writeFileSync } from "node:fs"
const bindings = require("./counter.node")
await runApplication(bindings, new URL("./lifecycle-worker.tsx", import.meta.url), { show: false })
console.log("Lifecycle host ended")
if (process.env.BRIDGE_LIFECYCLE_MODE === "after-host") {
  console.log("Outside host ready")
  writeFileSync(process.env.BRIDGE_LIFECYCLE_READY_FILE!, "outside")
  await new Promise(resolve => setTimeout(resolve, 60_000))
}
