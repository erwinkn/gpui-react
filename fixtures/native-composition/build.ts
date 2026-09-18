import { copyFileSync, mkdirSync } from "node:fs"
import { resolve } from "node:path"
import { fileURLToPath } from "node:url"

const directory = fileURLToPath(new URL(".", import.meta.url))
const target = resolve(process.env.CARGO_TARGET_DIR ?? resolve(directory, "target"))
const wasm = process.argv.includes("--wasm")
async function run(command: string[]) {
  const child = Bun.spawn(command, {
    cwd: directory,
    env: { ...process.env, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "3" },
    stdout: "inherit", stderr: "inherit",
  })
  if (await child.exited !== 0) throw new Error(`${command[0]} failed`)
}
if (wasm) {
  await run(["cargo", "+nightly", "build", "--release", "--no-default-features", "--target", "wasm32-unknown-unknown"])
  mkdirSync(resolve(directory, "wasm"), { recursive: true })
  await run(["wasm-bindgen", resolve(target, "wasm32-unknown-unknown/release/gpuix_extension_example.wasm"),
    "--target", "web", "--out-dir", resolve(directory, "wasm"), "--out-name", "example"])
} else {
  if (process.platform !== "darwin") throw new Error("The native texture fixture currently tests macOS")
  await run(["cargo", "build", "--release", "--locked"])
  copyFileSync(resolve(target, "release/libgpuix_extension_example.dylib"), resolve(directory, "example.node"))
}
