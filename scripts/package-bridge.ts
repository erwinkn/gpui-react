/** Build and pack the new bridge packages for a fork release. Never publishes. */
import { createHash } from "node:crypto"
import { execFileSync } from "node:child_process"
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { createRequire } from "node:module"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"

const root = resolve(import.meta.dir, "..")
const version = process.argv[2]
if (!version || !/^\d+\.\d+\.\d+-[a-z0-9.]+$/.test(version)) throw Error("Pass an explicit prerelease version")
const output = resolve(process.argv[3] ?? join(root, "dist/bridge"))
const dirty = execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim() !== ""
if (dirty && !process.argv.includes("--allow-dirty")) throw Error("Commit the source before packing; --allow-dirty is for local development tests only")
for (const args of [["scripts/build-bridge-runtime.ts"], ["run", "--cwd", "packages/bridge", "build"], ["run", "--cwd", "packages/bridge-controls", "build"]]) {
  const child = Bun.spawn(["bun", ...args], { cwd: root, stdout: "inherit", stderr: "inherit" })
  if (await child.exited !== 0) throw Error(`Build failed: ${args.join(" ")}`)
}
const binding = createRequire(import.meta.url)(join(root, "packages/bridge-runtime/binding.cjs"))
if (binding.bridgeRuntimeVersion() !== 1) throw Error("Unexpected native protocol")
const exports = Object.keys(binding).sort()
if (exports.join(",") !== "NativeClient,NativeHost,bridgeRuntimeVersion") throw Error(`Unexpected native exports: ${exports}`)
const base = `https://github.com/erwinkn/gpuix/releases/download/bridge-v${version}`
const stage = mkdtempSync(join(tmpdir(), "gpuix-bridge-pack-"))
const sha256 = (path: string) => createHash("sha256").update(readFileSync(path)).digest("hex")
mkdirSync(output, { recursive: true })
try {
  const packages = []
  for (const name of ["bridge", "bridge-controls", "bridge-runtime"]) {
    const source = join(root, "packages", name)
    const destination = join(stage, name)
    mkdirSync(destination)
    const manifest = JSON.parse(readFileSync(join(source, "package.json"), "utf8"))
    for (const file of manifest.files) cpSync(join(source, file), join(destination, file), { recursive: true })
    manifest.version = version
    manifest.private = true
    manifest.repository = { type: "git", url: "https://github.com/erwinkn/gpuix" }
    if (manifest.peerDependencies?.["@gpuix/bridge"]) manifest.peerDependencies["@gpuix/bridge"] = version
    delete manifest.devDependencies
    delete manifest.scripts
    writeFileSync(join(destination, "package.json"), JSON.stringify(manifest, null, 2) + "\n")
    const packed = JSON.parse(execFileSync("npm", ["pack", "--ignore-scripts", "--json", "--pack-destination", output], { cwd: destination, encoding: "utf8" }))
    if (packed.length !== 1) throw Error("Expected one archive")
    const filename = packed[0].filename
    packages.push({ name: manifest.name, filename, url: `${base}/${filename}`, sha256: sha256(join(output, filename)) })
  }
  const commit = (cwd: string) => execFileSync("git", ["rev-parse", "HEAD"], { cwd, encoding: "utf8" }).trim()
  const native = join(root, "packages/bridge-runtime/gpui-react-runtime.darwin-arm64.node")
  const manifest = {
    version, source: commit(root), gpui: commit(join(root, "zed")), sourceDirty: dirty,
    platform: process.platform, arch: process.arch, bun: Bun.version,
    native: { protocol: binding.bridgeRuntimeVersion(), exports, sha256: sha256(native) }, packages,
  }
  writeFileSync(join(output, "bridge-manifest.json"), JSON.stringify(manifest, null, 2) + "\n")
  console.log(JSON.stringify(manifest, null, 2))
} finally { rmSync(stage, { recursive: true, force: true }) }
