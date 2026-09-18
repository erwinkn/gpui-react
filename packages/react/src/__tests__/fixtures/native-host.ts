import { runApplication } from "../../application.js"
process.env.GPUIX_BACKGROUND = "1"
const count = Number(process.env.GPUIX_HOST_REPEATS ?? 1)
for (let index = 0; index < count; index++) {
  await runApplication(
    new URL(
      process.env.GPUIX_HOST_TEST === "missing"
        ? "./missing-worker.ts"
        : "./native-host-worker.tsx",
      import.meta.url,
    ),
    {
      title: "GPUiX application runtime test",
      width: 400,
      height: 280,
      focus: false,
    },
  )
  console.log(`Native application cycle ${index + 1} complete`)
}
if (process.env.GPUIX_AFTER_HOST_WAIT) {
  console.log("Native host ended; waiting outside the application")
  await new Promise((resolve) => setTimeout(resolve, 10000))
}
