import { runApplication } from "@gpui-react/core/application"

await runApplication(require("./counter.node"), new URL("./demo-worker.tsx", import.meta.url), {
  title: "New React to GPUI bridge", width: 1000, height: 760,
})
