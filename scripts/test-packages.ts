/** Install real archives, then run source and fully relocated native workers. */
import assert from "node:assert/strict"
import { createHash } from "node:crypto"
import { spawn } from "node:child_process"
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"

const root = resolve(import.meta.dir, "..")
const artifacts = resolve(process.argv[2] ?? join(root, "dist/bridge"))
const published = process.argv.includes("--published")
const manifest = JSON.parse(readFileSync(join(artifacts, "bridge-manifest.json"), "utf8"))
if (published) assert.equal(manifest.sourceDirty, false, "Published artifacts must use committed source")
const hash = (file: string) => createHash("sha256").update(readFileSync(file)).digest("hex")
assert.deepEqual(manifest.packages.map((item: { name: string }) => item.name).sort(), ["@gpui-react/core", "@gpui-react/kit", "@gpui-react/runtime"])
for (const item of manifest.packages) assert.equal(hash(join(artifacts, item.filename)), item.sha256)

const temporary = mkdtempSync(join(tmpdir(), "gpui-react-install-"))
const directory = join(temporary, "application")
mkdirSync(directory)
async function run(command: string, args: string[], cwd = directory, mode = "", timeout = 30_000): Promise<string> {
  return new Promise((resolveResult, reject) => {
    const child = spawn(command, args, { cwd, env: { ...process.env, BRIDGE_PACKAGE_MODE: mode }, stdio: ["ignore", "pipe", "pipe"] })
    let output = ""
    child.stdout.on("data", chunk => { output += chunk })
    child.stderr.on("data", chunk => { output += chunk })
    const timer = setTimeout(() => { child.kill("SIGKILL"); reject(Error(`Timed out: ${command}\n${output}`)) }, timeout)
    child.on("error", error => { clearTimeout(timer); reject(error) })
    child.on("exit", code => { clearTimeout(timer); code === 0 ? resolveResult(output) : reject(Error(`Exit ${code}: ${command}\n${output}`)) })
  })
}
try {
  const dependencies = Object.fromEntries(manifest.packages.map((item: { name: string; filename: string; url: string }) => [item.name, published ? item.url : `file:${join(artifacts, item.filename)}`]))
  writeFileSync(join(directory, "package.json"), JSON.stringify({
    private: true, type: "module", dependencies: { ...dependencies, react: "19.2.4" },
    devDependencies: { typescript: "5.9.3", "@types/react": "19.2.18", "@types/node": "22.19.7" },
  }))
  await run("npm", ["install", "--ignore-scripts", "--no-audit", "--no-fund"], directory, "", 120_000)
  const lock = JSON.parse(readFileSync(join(directory, "package-lock.json"), "utf8"))
  for (const item of manifest.packages) {
    const installed = JSON.parse(readFileSync(join(directory, "node_modules", item.name, "package.json"), "utf8"))
    assert.equal(installed.version, manifest.version)
    const integrity = "sha512-" + createHash("sha512").update(readFileSync(join(artifacts, item.filename))).digest("base64")
    assert.equal(lock.packages[`node_modules/${item.name}`].integrity, integrity, "Installed archive differs from the checked artifact")
  }
  assert.equal(hash(join(directory, "node_modules/@gpui-react/runtime/gpui-react-runtime.darwin-arm64.node")), manifest.native.sha256)
  console.log(`PASS isolated ${published ? "published" : "local"} archive installation and hashes`)

  const loader = `
    import assert from 'node:assert/strict';
    import {createRequire} from 'node:module';
    import bindings, {NativeHost, NativeClient, bridgeRuntimeVersion} from '@gpui-react/runtime';
    const commonjs=createRequire(import.meta.url)('@gpui-react/runtime');
    assert.equal(commonjs,bindings);
    assert.equal(NativeHost,bindings.NativeHost);
    assert.equal(NativeClient,bindings.NativeClient);
    assert.equal(bridgeRuntimeVersion(),2);
    assert.deepEqual(Object.keys(commonjs).sort(),['NativeClient','NativeHost','bridgeRuntimeVersion']);
    console.log('loader passed');
  `
  writeFileSync(join(directory, "loader.mjs"), loader)
  for (const runtime of ["node", "bun"]) assert.match(await run(runtime, ["loader.mjs"]), /loader passed/)
  console.log("PASS Node/Bun ESM and CommonJS loader identity")
  const runtime = join(directory, "node_modules/@gpui-react/runtime")
  // Run the real guard with a substituted platform, without attempting to load a binary.
  writeFileSync(join(directory, "unsupported.cjs"), `
    const vm=require('node:vm'), fs=require('node:fs'), assert=require('node:assert/strict');
    const source=fs.readFileSync(${JSON.stringify(join(runtime, "binding.cjs"))},'utf8');
    for(const [platform,arch] of [['linux','x64'],['darwin','x64']]) {
      assert.throws(()=>vm.runInNewContext(source,{process:{platform,arch},require(){throw Error('unexpected native load')}}),/no binary is included/);
    }
  `)
  await run("node", ["unsupported.cjs"])
  writeFileSync(join(directory, "browser-entry.js"), `import bindings from '@gpui-react/runtime'; console.log(bindings.bridgeRuntimeVersion())`)
  await run("bun", ["build", "browser-entry.js", "--target=browser", "--outfile", "browser-bundle.js"])
  const browser = readFileSync(join(directory, "browser-bundle.js"), "utf8")
  assert.ok(!browser.includes(".node"), "Browser resolution pulled in the native loader")
  await assert.rejects(run("node", ["browser-bundle.js"]), /The bridge currently has no browser driver/)
  console.log("PASS unsupported-platform and browser errors")

  for (const file of ["host.ts", "worker.tsx", "commonjs.cts"]) cpSync(join(root, "fixtures/package", file), join(directory, file))
  writeFileSync(join(directory, "tsconfig.json"), JSON.stringify({ compilerOptions: {
    target: "ES2022", module: "NodeNext", moduleResolution: "NodeNext", jsx: "react-jsx",
    strict: true, noEmit: true, skipLibCheck: false, types: ["node", "react"],
  }, include: ["*.ts", "*.tsx", "*.cts"] }))
  await run("node", ["node_modules/typescript/bin/tsc"])
  console.log("PASS installed ESM and CommonJS TypeScript declarations")
  assert.match(await run("bun", ["host.ts"]), /Installed controls passed:/)
  await assert.rejects(run("bun", ["host.ts"], directory, "unknown"), /Unknown native component counter/)
  console.log("PASS installed source worker and default component boundary")

  await run("bun", ["build", "--compile", "host.ts", "worker.tsx", "--outfile", "compiled/application"])
  const relocated = join(temporary, "relocated")
  mkdirSync(relocated)
  const executable = join(relocated, "application")
  cpSync(join(directory, "compiled/application"), executable)
  rmSync(directory, { recursive: true, force: true })
  assert.equal(existsSync(directory), false)
  assert.match(await run(executable, [], relocated), /Installed controls passed:/)
  await assert.rejects(run(executable, [], relocated, "unknown"), /Unknown native component counter/)
  console.log("PASS relocated executable without source, node_modules, or build directory")
} finally {
  rmSync(temporary, { recursive: true, force: true })
}
