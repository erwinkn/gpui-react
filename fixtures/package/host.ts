import bindings from "@gpui-react/runtime"
import { runApplication } from "@gpui-react/core/application"

await runApplication(bindings, new URL("./worker.tsx", import.meta.url), {
  title: "Installed bridge check", width: 400, height: 320, show: false,
})
console.log("Installed native application completed")
