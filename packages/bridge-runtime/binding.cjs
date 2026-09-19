if (process.platform !== "darwin" || process.arch !== "arm64") {
  throw new Error(`@gpuix/bridge-runtime provides a tested macOS arm64 runtime only; no binary is included for ${process.platform}-${process.arch}.`)
}

// Keep the require literal so Bun can embed the same native asset for both entries.
module.exports = require("./gpui-react-runtime.darwin-arm64.node")
