import assert from "node:assert/strict"
import { mkdtemp, cp, rm, readdir } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import { spawn } from "node:child_process"

async function run(command: string, args: string[], cwd?: string, mode?: string): Promise<string> {
  return new Promise((resolveResult, reject) => {
    const child = spawn(command, args, { cwd, env: { ...process.env, GPUIX_BACKGROUND: "1", BRIDGE_COUNTER_MODE: mode ?? "" }, stdio: ["ignore", "pipe", "pipe"] })
    let output = ""
    child.stdout.on("data", chunk => { output += chunk })
    child.stderr.on("data", chunk => { output += chunk })
    const timer = setTimeout(() => { child.kill("SIGKILL"); reject(Error(`Timed out: ${command}\n${output}`)) }, 30_000)
    child.on("error", error => { clearTimeout(timer); reject(error) })
    child.on("exit", code => { clearTimeout(timer); code === 0 ? resolveResult(output) : reject(Error(`Exit ${code}: ${command}\n${output}`)) })
  })
}

const fixture = resolve(import.meta.dir)
const checkout = resolve(fixture, "../..")
const originalFiles = new Set(await readdir(checkout))
const temporary = await mkdtemp(join(tmpdir(), "gpui-react-package-"))
try {
  const source = await run("bun", [join(fixture, "host.ts")])
  assert.match(source, /Native counter worker completed/)
  console.log("PASS source worker", source.trim())
  const repeat = await run("bun", [join(fixture, "host.ts")], undefined, "repeat")
  assert.equal(repeat.match(/Native counter state after worker stall/g)?.length, 3)
  console.log("PASS three sequential native sessions")
  for (const [mode, message] of [["startup-error", /Expected worker startup failure/], ["missing", /missing-worker/], ["invalid-props", /Native transaction failed/]] as const) {
    await assert.rejects(run("bun", [join(fixture, "host.ts")], undefined, mode), message)
    console.log(`PASS ${mode} cleanup`)
  }
  const binary = join(temporary, "build", "counter")
  await run("bun", ["build", "--compile", join(fixture, "host.ts"), join(fixture, "worker.tsx"), "--outfile", binary], temporary)
  assert.deepEqual((await readdir(checkout)).filter(name => name.endsWith(".bun-build") && !originalFiles.has(name)), [], "Compilation left generated files in the checkout")
  const moved = join(temporary, "relocated", "counter")
  await cp(binary, moved, { recursive: true })
  await rm(join(temporary, "build"), { recursive: true, force: true })
  const packaged = await run(moved, [], tmpdir())
  assert.match(packaged, /Native counter worker completed/)
  console.log("PASS relocated executable", packaged.trim())
} finally {
  await rm(temporary, { recursive: true, force: true })
}
