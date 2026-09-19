import { runApplication } from "@gpuix/bridge/application"
const bindings = require("./pierre.node")
await runApplication(bindings, new URL("./worker.tsx", import.meta.url), {
  title: "Pierre bridge check", width: 640, height: 320, show: false,
})
console.log("Pierre native worker completed")
