import bindings = require("@gpui-react/runtime")
import type { Bindings } from "@gpui-react/core/application" with { "resolution-mode": "import" }
const typed: Bindings = bindings
if (typed.bridgeRuntimeVersion() !== 2) throw Error("Native protocol mismatch")
