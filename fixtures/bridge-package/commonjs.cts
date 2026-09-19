import bindings = require("@gpuix/bridge-runtime")
import type { Bindings } from "@gpuix/bridge/application" with { "resolution-mode": "import" }
const typed: Bindings = bindings
if (typed.bridgeRuntimeVersion() !== 1) throw Error("Native protocol mismatch")
