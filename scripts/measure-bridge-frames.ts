/** Sequential native comparisons. Build once, then run each scene in a fresh process. */
import { execFileSync } from "node:child_process"
import assert from "node:assert/strict"
import { createHash } from "node:crypto"
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { join, resolve } from "node:path"

const root = resolve(import.meta.dir, "..")
const output = resolve(process.argv[2] ?? "/tmp/gpuix-frame-cost")
const target = resolve(process.env.CARGO_TARGET_DIR ?? join(root, "target/bridge-performance"))
mkdirSync(output, { recursive: true })
const run = (command: string, args: string[], cwd = root) => execFileSync(command, args, { cwd, encoding: "utf8" }).trim()
for (const allocations of [false, true]) {
  const args = ["build", "--release", "--locked", "--manifest-path", "fixtures/bridge-performance/Cargo.toml"]
  if (allocations) args.push("--features", "allocation-counts")
  const build = Bun.spawn(["cargo", ...args], { cwd: root, env: { ...process.env, CARGO_TARGET_DIR: target, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "3" }, stdout: "inherit", stderr: "inherit" })
  if (await build.exited !== 0) throw Error("Frame comparison build failed")
  copyFileSync(join(target, "release/gpui-react-frame-cost"), join(output, allocations ? "heap" : "timing"))
}
const results: object[] = []
const modes = ["raw", "controls", "bridge", "legacy"]
const images = join(output, "scenes")
const sceneHashes: Record<string, string> = {}
for (const scene of ["flow", "list"]) {
  for (const mode of modes) {
    execFileSync(join(output, "timing"), [mode, scene, "100"], { cwd: output, env: { ...process.env, FRAME_BENCH_IMAGES: images }, timeout: 30_000, stdio: ["ignore", "pipe", "pipe"] })
    const hash = createHash("sha256").update(readFileSync(join(images, `${mode}-${scene}-100.png`))).digest("hex")
    if (sceneHashes[scene]) assert.equal(hash, sceneHashes[scene], `${mode} ${scene} image differs from the baseline`)
    sceneHashes[scene] = hash
  }
}
console.log("PASS matching nonempty native scene images and explicit draw guards")
for (const allocations of [false, true]) {
  const executable = join(output, allocations ? "heap" : "timing")
  for (const scene of ["flow", "list"]) {
    for (const rows of [100, 1000, 5000]) {
      // Rotate mode order across repeats to reduce a fixed thermal/order bias.
      for (let repeat = 0; repeat < 3; repeat++) {
        for (let index = 0; index < modes.length; index++) {
          const mode = modes[(index + repeat) % modes.length]!
          const child = Bun.spawn([executable, mode, scene, String(rows)], { cwd: output, env: { ...process.env, GPUIX_BACKGROUND: "1", FRAME_BENCH_IMAGES: "" }, stdout: "pipe", stderr: "pipe" })
          const timer = setTimeout(() => child.kill("SIGKILL"), 120_000)
          const [stdout, stderr, code] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited])
          clearTimeout(timer)
          if (code !== 0) throw Error(`${mode} ${scene} ${rows}: ${stderr}\n${stdout}`)
          const data = JSON.parse(stdout.trim())
          results.push({ ...data, repeat })
          writeFileSync(join(output, "samples.json"), JSON.stringify(results, null, 2) + "\n")
          console.log(`${allocations ? "heap" : "time"} ${scene} ${rows} ${mode}: draw p50 ${data.updatedDraw.p50Us.toFixed(1)} us`)
        }
      }
    }
  }
}
const metadata = {
  source: run("git", ["rev-parse", "HEAD"]), gpui: run("git", ["rev-parse", "HEAD"], join(root, "zed")),
  sourceDirty: run("git", ["status", "--porcelain"]) !== "", rust: run("rustc", ["--version"]),
  cpu: run("sysctl", ["-n", "machdep.cpu.brand_string"]), memory: run("sysctl", ["-n", "hw.memsize"]),
  macos: run("sw_vers", ["-productVersion"]), measuredAt: new Date().toISOString(), sceneHashes,
}
writeFileSync(join(output, "results.json"), JSON.stringify({ metadata, results }, null, 2) + "\n")
console.log(join(output, "results.json"))
