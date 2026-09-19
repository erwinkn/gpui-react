import bindings from "@gpuix/bridge-runtime"
import { runApplication } from "@gpuix/bridge/application"

await runApplication(bindings, new URL("./worker.tsx", import.meta.url), {
  title: "Installed bridge check", width: 400, height: 320, show: false,
})
console.log("Installed native application completed")
