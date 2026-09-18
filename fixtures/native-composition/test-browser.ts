import assert from "node:assert/strict"
import { execFileSync } from "node:child_process"
import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { resolve } from "node:path"
import { PNG } from "pngjs"
import { waitFor } from "./wait-for"

const base = process.argv[2] ?? "http://127.0.0.1:4187"
const session = `gpuix-composition-${process.pid}`
const output = resolve(import.meta.dir, "../../packages/react/screenshots/extension-composition")
mkdirSync(output, { recursive: true })
function browser(args: string[], input?: string) {
  const response = JSON.parse(execFileSync("agent-browser", ["--session", session, "--json", ...args], {
    input, encoding: "utf8", timeout: 30000,
  }))
  assert(response.success, response.error)
  return response.data
}
const evaluate = (code: string) => browser(["eval", "--stdin"], code).result
const proofs = []
try {
  for (const backend of ["webgpu", "webgl"]) {
    browser(["open", `${base}/?backend=${backend}`])
    browser(["wait", "--fn", "Boolean(globalThis.extensionProbe && globalThis.gpuix)"])
    const geometry = await waitFor(() => evaluate(`(async()=>({
      entry: (await globalThis.gpuix.getByTestId('texture').all())[0],
      canvas: {width: document.querySelector('canvas').width, rect: document.querySelector('canvas').getBoundingClientRect().toJSON()},
      viewportWidth: innerWidth,
      info: globalThis.extensionProbe.info,
      defaultRequests: performance.getEntriesByType('resource').filter(entry=>entry.name.includes('/unexpected-default/')).length
    }))()`), value => value.entry?.bounds?.width === 64)
    assert.equal(geometry.defaultRequests, 0)
    assert.equal(geometry.entry.bounds.width, 64)
    assert.deepEqual(geometry.info.extensions.map((item: {id: string}) => item.id), ["gpuix.example"])
    const logs = browser(["console"]).messages
    assert(JSON.stringify(logs).includes(backend === "webgpu" ? "selected=BrowserWebGpu" : "selected=Gl"))
    function screenshot(name: string) {
      const path = resolve(output, `${backend}-${name}.png`)
      browser(["screenshot", path])
      return PNG.sync.read(readFileSync(path))
    }
    function pixel(image: PNG, x: number, y: number, expected: number[]) {
      const factor = image.width / geometry.viewportWidth
      const canvasFactor = geometry.canvas.rect.width / geometry.canvas.width
      const px = Math.floor((geometry.canvas.rect.x + (x + 0.5) * canvasFactor) * factor)
      const py = Math.floor((geometry.canvas.rect.y + (y + 0.5) * canvasFactor) * factor)
      const offset = (py * image.width + px) * 4
      const actual = [...image.data.subarray(offset, offset + 4)]
      actual.forEach((channel, i) => assert(Math.abs(channel - expected[i]) <= 1, `${backend} pixel ${x},${y}: ${actual}, expected ${expected}`))
    }
    let image = screenshot("initial")
    pixel(image, 24, 40, [128, 0, 239, 255])
    pixel(image, 64, 40, [0, 0, 255, 255])
    pixel(image, 8, 8, [0, 0, 255, 255])
    pixel(image, 96, 24, [0, 255, 0, 255])
    assert.equal(evaluate("(async()=>{await globalThis.gpuix.getByTestId('texture').click();return globalThis.extensionProbe.clicks()})()"), 1)
    evaluate("globalThis.extensionProbe.resize()")
    await waitFor(() => evaluate("(async()=>(await globalThis.gpuix.getByTestId('texture').all())[0]?.bounds?.width)()"), width => width === 32)
    image = screenshot("resized")
    pixel(image, 24, 24, [32, 0, 223, 255])
    pixel(image, 48, 40, [0, 0, 255, 255])
    evaluate("globalThis.extensionProbe.remove()")
    await waitFor(() => evaluate("globalThis.gpuix.getByTestId('texture').count()"), count => count === 0)
    image = screenshot("removed")
    pixel(image, 24, 24, [0, 0, 255, 255])
    pixel(image, 96, 24, [0, 0, 255, 255])
    assert.deepEqual(browser(["errors"]).errors, [])
    assert(!browser(["console"]).messages.some((message: {type: string}) => message.type === "error"))
    proofs.push({ backend, geometry, logs, passed: true })
  }
  writeFileSync(resolve(output, "browser-results.json"), JSON.stringify(proofs, null, 2) + "\n")
  console.log("WebGPU and WebGL composition tests passed")
} catch (error) {
  writeFileSync(resolve(output, "browser-failure.json"), JSON.stringify({
    error: String(error), logs: browser(["console"]), errors: browser(["errors"]),
  }, null, 2) + "\n")
  throw error
} finally {
  browser(["close"])
}
