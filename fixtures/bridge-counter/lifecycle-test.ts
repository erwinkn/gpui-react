import assert from "node:assert/strict"
import { spawn } from "node:child_process"
import { existsSync } from "node:fs"
import { mkdtemp, readFile, rm } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"

const temporary = await mkdtemp(join(tmpdir(), "gpui-react-lifecycle-"))
const compiled = process.argv[2]
try {
  for (const mode of ["normal", "signal-term", "signal-int", "blocked-signal", "blocked-close", "overflow", "crash", "exit", "after-host"]) {
    const native = join(temporary, `${mode}-native`)
    const react = join(temporary, `${mode}-react`)
    const ready = join(temporary, `${mode}-ready`)
    const child = spawn(compiled ?? "bun", compiled ? [] : [join(import.meta.dir, "lifecycle-host.ts")], {
      env: { ...process.env, BRIDGE_LIFECYCLE_MODE: mode, BRIDGE_LIFECYCLE_NATIVE_FILE: native, BRIDGE_LIFECYCLE_REACT_FILE: react, BRIDGE_LIFECYCLE_READY_FILE: ready },
      stdio: ["ignore", "pipe", "pipe"],
    })
    let output = "", sent = false
    child.stdout.on("data", chunk => { output += chunk })
    // Worker console output may wait for the main JS thread. File readiness
    // does not depend on that thread while AppKit owns it.
    const readyTimer = setInterval(() => {
      if (!sent && existsSync(ready) && ["signal-term", "signal-int", "blocked-signal", "after-host"].includes(mode)) {
        sent = true
        child.kill(mode === "signal-int" ? "SIGINT" : "SIGTERM")
      }
    }, 10)
    child.stderr.on("data", chunk => { output += chunk })
    let timedOut = false
    const timer = setTimeout(() => { timedOut = true; child.kill("SIGKILL") }, 8_000)
    const result = await new Promise<{ code: number | null; signal: NodeJS.Signals | null }>((resolve, reject) => {
      child.once("error", reject)
      child.once("exit", (code, signal) => resolve({ code, signal }))
    }).finally(() => { clearTimeout(timer); clearInterval(readyTimer) })
    assert.equal(timedOut, false, `${mode}: timed out\n${output}`)
    if (mode === "after-host") assert.deepEqual(result, { code: null, signal: "SIGTERM" }, output)
    else assert.deepEqual(result, { code: ["overflow", "crash", "exit"].includes(mode) ? 1 : 0, signal: null }, `${mode}: ${output}`)
    assert.equal(await readFile(native, "utf8"), "mounted\nunmount\ndrop\n", `${mode}: native resources must close once\n${output}`)
    const cleanup = await readFile(react, "utf8").catch(() => "")
    if (["normal", "signal-term", "signal-int", "after-host"].includes(mode)) {
      assert.equal(cleanup, "layout\npassive\nexit\n", `${mode}: React cleanup missing\n${output}`)
    } else if (mode.startsWith("blocked-") || mode === "overflow") {
      assert.ok(!cleanup.includes("layout") && !cleanup.includes("passive"), "a blocked worker cannot run React cleanup")
    }
    if (mode === "overflow") assert.match(output, /Native event queue overflow/)
    if (mode === "crash") assert.match(output, /Expected failure after native mount/)
    console.log(`PASS ${mode}: native cleanup, worker exit, and process result`)
  }
} finally {
  await rm(temporary, { recursive: true, force: true })
}
