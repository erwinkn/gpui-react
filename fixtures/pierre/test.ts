import assert from "node:assert/strict"
import { cp, mkdtemp, rm } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import { spawn } from "node:child_process"

// The component and binary remain in Pierre. This probe does not build or edit it.
const library = process.argv[2]
if (!library) throw Error("Usage: bun fixtures/pierre/test.ts /path/to/libpierre_react_runtime.dylib")
const fixture = import.meta.dir
async function run(command: string, args: string[], cwd = fixture): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, env: { ...process.env, PIERRE_FRAME_PROBE: process.argv.includes("--frames") ? "1" : "0" }, stdio: ["ignore", "pipe", "pipe"] })
    let output = ""
    child.stdout.on("data", chunk => { output += chunk })
    child.stderr.on("data", chunk => { output += chunk })
    const timer = setTimeout(() => { child.kill("SIGKILL"); reject(Error(`Timeout: ${command}\n${output}`)) }, 30_000)
    child.on("error", error => { clearTimeout(timer); reject(error) })
    child.on("exit", code => { clearTimeout(timer); code === 0 ? resolve(output) : reject(Error(`Exit ${code}: ${command}\n${output}`)) })
  })
}
const temp = await mkdtemp(join(tmpdir(), "pierre-bridge-package-"))
try {
  await cp(resolve(library), join(fixture, "pierre.node"))
  const source = await run("bun", ["host.ts"])
  assert.match(source, /PASS Pierre bridge:/)
  console.log("PASS source", source.trim())
  const build = join(temp, "build", "pierre")
  await run("bun", ["build", "--compile", join(fixture, "host.ts"), join(fixture, "worker.tsx"), "--outfile", build], temp)
  const moved = join(temp, "relocated", "pierre")
  await cp(build, moved, { recursive: true })
  await rm(join(temp, "build"), { recursive: true, force: true })
  const relocated = await run(moved, [], tmpdir())
  assert.match(relocated, /PASS Pierre bridge:/)
  console.log("PASS relocated", relocated.trim())
} finally {
  await rm(temp, { recursive: true, force: true })
  await rm(join(fixture, "pierre.node"), { force: true })
}
