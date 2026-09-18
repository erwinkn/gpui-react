import assert from "node:assert/strict"
import { copyFileSync, mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { fileURLToPath } from "node:url"
import { execFileSync } from "node:child_process"

const host = fileURLToPath(new URL("./host.ts", import.meta.url))
const worker = fileURLToPath(new URL("./worker.ts", import.meta.url))
const build = mkdtempSync(join(tmpdir(), "gpuix-extension-build-"))
const relocated = mkdtempSync(join(tmpdir(), "gpuix-extension-relocated-"))
const env = { ...process.env, GPUIX_BACKGROUND: "1" }
try {
  const source = execFileSync("bun", [host], { env, encoding: "utf8", timeout: 15000 })
  assert(source.includes("External texture worker completed"))
  const executable = join(build, "extension-host")
  execFileSync("bun", ["build", "--compile", host, worker, "--outfile", executable], {
    env, cwd: build, encoding: "utf8", timeout: 30000,
  })
  const moved = join(relocated, "extension-host")
  copyFileSync(executable, moved)
  rmSync(build, { recursive: true, force: true })
  const compiled = execFileSync(moved, [], { env, cwd: relocated, encoding: "utf8", timeout: 15000 })
  assert(compiled.includes("External texture worker completed"))
  console.log("External composition passed source and relocated compiled worker tests")
} finally {
  rmSync(build, { recursive: true, force: true })
  rmSync(relocated, { recursive: true, force: true })
}
