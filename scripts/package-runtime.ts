/** Pack tested fork artifacts for GitHub Releases. This never publishes to npm. */
import { createHash } from "node:crypto"
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { resolve, join } from "node:path"
import { execFileSync } from "node:child_process"
import { createRequire } from "node:module"

const root = resolve(import.meta.dir, "..")
const version = process.argv[2]
if (!version || !/^\d+\.\d+\.\d+-[a-z0-9.]+$/.test(version)) throw new Error("Pass an explicit fork prerelease version")
if (process.platform !== "darwin" || process.arch !== "arm64") throw new Error("This release packs the tested darwin-arm64 default binary")
const output = resolve(process.argv[3] ?? join(root, "dist/runtime"))
const base = `https://github.com/erwinkn/gpuix/releases/download/runtime-v${version}`
const stage = mkdtempSync(join(tmpdir(), "gpuix-runtime-pack-"))
const nativeFiles = ["index.js", "index.d.ts", "loader.cjs", "runtime.cjs", "runtime.d.ts", "browser.mjs", "LICENSE", "wasm", "gpuix-native.darwin-arm64.node"]
const reactFiles = ["dist", "jsx-runtime.js", "jsx-runtime.d.ts", "jsx-dev-runtime.js", "jsx-dev-runtime.d.ts", "LICENSE"]
const sha256 = (file: string) => createHash("sha256").update(readFileSync(file)).digest("hex")
for (const file of ["wasm/gpuix-web.js", "wasm/gpuix-web_bg.wasm", "wasm/gpuix-web.d.ts"]) {
  if (!existsSync(join(root, "packages/native", file))) throw new Error(`Missing browser artifact: ${file}`)
}
mkdirSync(output, { recursive: true })
try {
  const packages = []
  for (const [name, files] of [["native", nativeFiles], ["react", reactFiles]] as const) {
    const source = join(root, "packages", name)
    const destination = join(stage, name)
    mkdirSync(destination)
    for (const file of files) {
      if (!existsSync(join(source, file))) throw new Error(`Missing built artifact: ${name}/${file}`)
      cpSync(join(source, file), join(destination, file), { recursive: true })
    }
    const manifest = JSON.parse(readFileSync(join(source, "package.json"), "utf8"))
    manifest.version = version
    manifest.private = true
    manifest.files = files
    manifest.repository = { type: "git", url: "https://github.com/erwinkn/gpuix" }
    delete manifest.publishConfig
    delete manifest.scripts
    delete manifest.devDependencies
    if (name === "native") manifest.napi.targets = ["aarch64-apple-darwin"]
    if (name === "react") manifest.dependencies["@gpuix/native"] = `${base}/gpuix-native-${version}.tgz`
    writeFileSync(join(destination, "package.json"), JSON.stringify(manifest, null, 2) + "\n")
    const packed = JSON.parse(execFileSync("npm", ["pack", "--ignore-scripts", "--json", "--pack-destination", output], {
      cwd: destination, encoding: "utf8",
    }))
    if (packed.length !== 1) throw new Error("Expected one package archive")
    const filename = packed[0].filename
    packages.push({ name: manifest.name, filename, url: `${base}/${filename}`, sha256: sha256(join(output, filename)) })
  }
  const native = createRequire(import.meta.url)(join(stage, "native/loader.cjs"))
  const contract = JSON.parse(native.nativeRuntimeInfo())
  if (contract.apiVersion !== 1 || contract.extensionApiVersion !== 1 || contract.extensions.length !== 0) {
    throw new Error("Expected the plain core runtime contract without consumer extensions")
  }
  const commit = (cwd: string) => execFileSync("git", ["rev-parse", "HEAD"], { cwd, encoding: "utf8" }).trim()
  const manifest = {
    version, source: commit(root), gpui: commit(join(root, "zed")),
    sourceDirty: execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim().length !== 0,
    defaultNative: { platform: "darwin", arch: "arm64" }, contract,
    packages,
  }
  writeFileSync(join(output, "runtime-manifest.json"), JSON.stringify(manifest, null, 2) + "\n")
  console.log(JSON.stringify(manifest, null, 2))
} finally {
  rmSync(stage, { recursive: true, force: true })
}
