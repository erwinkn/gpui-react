// Alternate build graphs in one target directory. A reusable host must not
// overwrite a different feature graph's unqualified native library artifact.
import { resolve } from "node:path"

const repo = resolve(import.meta.dir, "..")
const env = {
  ...process.env,
  CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR ?? resolve(repo, "target/bridge-build-check"),
  CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "3",
}
const builds = [
  ["fixtures/counter/Cargo.toml", "--features", "interaction-tests"],
  ["crates/gpui-react-runtime/Cargo.toml"],
  ["fixtures/counter/Cargo.toml", "--features", "interaction-tests"],
]
for (const [manifest, ...args] of builds) {
  const child = Bun.spawn(["cargo", "build", "--release", "--manifest-path", manifest!, ...args], {
    cwd: repo, env, stdout: "inherit", stderr: "inherit",
  })
  if (await child.exited !== 0) throw Error(`Native build failed: ${manifest}`)
}
console.log("PASS application and standalone host builds share one target directory")
