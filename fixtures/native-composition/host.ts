import { configureNativeBindings } from "../../packages/native/runtime.cjs"
process.env.GPUIX_BACKGROUND = "1"
configureNativeBindings(require("./example.node"))
const { runApplication } = await import("@gpuix/react/application")
await runApplication(new URL("./worker.ts", import.meta.url), {
  title: "GPUiX external texture test", width: 128, height: 96, focus: false,
})
console.log("External texture worker completed")
