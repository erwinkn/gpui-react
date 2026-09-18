/** Check browser entry points from installed archives, including default WASM. */
import assert from "node:assert/strict"
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join, resolve, sep } from "node:path"
import { PNG } from "pngjs"

const artifacts = resolve(process.argv[2] ?? "dist/runtime")
const manifest = JSON.parse(readFileSync(join(artifacts, "runtime-manifest.json"), "utf8"))
const directory = mkdtempSync(join(tmpdir(), "gpuix-browser-install-"))
const session = `gpuix-package-${process.pid}`
const output = resolve(import.meta.dir, "../../packages/react/screenshots/extension-composition")
mkdirSync(output, { recursive: true })
async function run(command: string[], cwd = directory) {
  const child = Bun.spawn(command, { cwd, stdout: "pipe", stderr: "pipe" })
  const [stdout, stderr, status] = await Promise.all([
    new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited,
  ])
  assert.equal(status, 0, `${command[0]} failed: ${stderr}\n${stdout}`)
  return stdout
}
async function browser(...args: string[]) {
  const response = JSON.parse(await run(["agent-browser", "--session", session, "--json", ...args]))
  assert(response.success, response.error)
  return response.data
}
let server: ReturnType<typeof Bun.serve> | undefined
try {
  const dependencies = Object.fromEntries(manifest.packages.map((item: {name: string; filename: string}) => [item.name, `file:${join(artifacts, item.filename)}`]))
  writeFileSync(join(directory, "package.json"), JSON.stringify({
    private: true, type: "module", dependencies: { ...dependencies, react: "19.2.4" },
    overrides: { "@gpuix/native": "$@gpuix/native" },
  }))
  await run(["npm", "install", "--ignore-scripts", "--no-audit", "--no-fund"])
  for (const selected of [false, true]) {
    const name = selected ? "selected" : "default"
    writeFileSync(join(directory, `${name}.ts`), `
      ${selected ? `import * as bindings from ${JSON.stringify(resolve(import.meta.dir, "wasm/example.js"))};
        import wasmUrl from ${JSON.stringify(resolve(import.meta.dir, "wasm/example_bg.wasm"))} with {type:'file'};
        import {configureNativeBindings} from '@gpuix/native/runtime';` : ""}
      if(new URL(location.href).searchParams.get('backend')==='webgl') Object.defineProperty(navigator,'gpu',{value:undefined,configurable:true});
      ${selected ? "await bindings.default({module_or_path:wasmUrl}); bindings.registerProbe(); configureNativeBindings(bindings);" : ""}
      const {createElement:h}=await import('react');
      const {render}=await import('@gpuix/react');
      const native=await import('@gpuix/native');
      ${selected ? "if(!Object.is(native.GpuixRenderer,bindings.GpuixRenderer)) throw new Error('Wrong renderer');" : ""}
      render(h(${JSON.stringify(selected ? "example-gpu-texture" : "div")}, {
        testId:'package-probe', ${selected ? "color:[0,1,0,1]," : ""}
        style:{width:64,height:64,backgroundColor:${JSON.stringify(selected ? "#0000ff" : "#00ff00")}}
      }),{title:'GPUiX package test',focus:false});
      globalThis.packageProbe=JSON.parse(native.nativeRuntimeInfo());
    `)
  }
  const web = join(directory, "web")
  const bundle = await Bun.build({
    entrypoints: [join(directory, "default.ts"), join(directory, "selected.ts")],
    outdir: web, target: "browser", splitting: true, format: "esm",
    naming: { entry: "[name].js", chunk: "[name]-[hash].[ext]", asset: "[name]-[hash].[ext]" },
  })
  if (!bundle.success) throw new AggregateError(bundle.logs, "Installed package bundle failed")
  const headers = { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" }
  server = Bun.serve({ hostname: "127.0.0.1", port: 0, async fetch(request) {
    const url = new URL(request.url)
    if (url.pathname === "/") {
      const entry = url.searchParams.get("entry") === "selected" ? "selected" : "default"
      return new Response(`<!doctype html><html><head><style>html,body{margin:0}</style></head><body><script type="module" src="/${entry}.js"></script></body></html>`, { headers: { ...headers, "Content-Type": "text/html" } })
    }
    const file = resolve(web, `.${url.pathname}`)
    if (!file.startsWith(web + sep)) return new Response("Not found", { status: 404 })
    const asset = Bun.file(file)
    return await asset.exists() ? new Response(asset, { headers }) : new Response("Not found", { status: 404 })
  } })
  for (const entry of ["default", "selected"]) {
    for (const backend of ["webgpu", "webgl"]) {
      await browser("open", `${server.url}?entry=${entry}&backend=${backend}`)
      await browser("wait", "--fn", "Boolean(globalThis.packageProbe && globalThis.gpuix)")
      const { result } = await browser("eval", `(async()=>({info:globalThis.packageProbe, bounds:(await globalThis.gpuix.getByTestId('package-probe').waitFor()).bounds, viewport:innerWidth, canvas:{width:document.querySelector('canvas').width,rect:document.querySelector('canvas').getBoundingClientRect().toJSON()}, resources:performance.getEntriesByType('resource').map(x=>x.name)}))()`)
      assert.equal(result.info.extensions.length, entry === "selected" ? 1 : 0)
      assert.equal(result.bounds.width, 64)
      if (entry === "selected") assert(!result.resources.some((url: string) => url.includes("gpuix-web")), "Selected binding fetched default WASM")
      assert(JSON.stringify((await browser("console")).messages).includes(backend === "webgpu" ? "selected=BrowserWebGpu" : "selected=Gl"))
      const screenshot = join(output, `package-${entry}-${backend}.png`)
      await browser("screenshot", screenshot)
      const image = PNG.sync.read(readFileSync(screenshot))
      const factor = image.width / result.viewport
      const canvasFactor = result.canvas.rect.width / result.canvas.width
      const x = Math.floor((result.canvas.rect.x + 32 * canvasFactor) * factor)
      const y = Math.floor((result.canvas.rect.y + 32 * canvasFactor) * factor)
      assert.deepEqual([...image.data.subarray((y * image.width + x) * 4, (y * image.width + x) * 4 + 4)], [0,255,0,255])
      assert.deepEqual((await browser("errors")).errors, [])
      assert(!(await browser("console")).messages.some((message: {type: string}) => message.type === "error"))
    }
  }
  console.log("Installed browser archives passed default and selected WebGPU/WebGL tests")
} finally {
  await browser("close")
  server?.stop(true)
  rmSync(directory, { recursive: true, force: true })
}
