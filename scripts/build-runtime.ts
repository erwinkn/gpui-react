import { copyFileSync } from "node:fs"
import { resolve, join } from "node:path"

if (process.platform !== "darwin" || process.arch !== "arm64") {
  throw Error("The default runtime package currently builds on macOS arm64 only")
}
const root = resolve(import.meta.dir, "..")
const target = resolve(process.env.CARGO_TARGET_DIR ?? join(root, "target/bridge-runtime"))
const build = Bun.spawn(["cargo", "build", "--release", "--locked", "--manifest-path", "crates/gpui-react-runtime/Cargo.toml"], {
  cwd: root, env: { ...process.env, CARGO_TARGET_DIR: target, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "3" },
  stdout: "inherit", stderr: "inherit",
})
if (await build.exited !== 0) throw Error("Default native runtime build failed")
const destination = join(root, "packages/runtime/gpui-react-runtime.darwin-arm64.node")
copyFileSync(join(target, "release/libgpui_react_runtime.dylib"), destination)
console.log(destination)
