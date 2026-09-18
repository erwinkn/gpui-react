import { resolve, sep } from "node:path"
import { fileURLToPath } from "node:url"

const directory = fileURLToPath(new URL(".", import.meta.url))
const output = resolve(directory, "web-dist")
const result = await Bun.build({
  entrypoints: [resolve(directory, "browser.ts")],
  outdir: output, target: "browser", format: "esm", splitting: true,
  naming: { entry: "browser.js", chunk: "[name]-[hash].[ext]", asset: "[name]-[hash].[ext]" },
  plugins: [{
    name: "reject-default-runtime",
    setup(build) {
      // A configured composition must never initialize the default WASM.
      // Leave those imports external; the test server rejects any request.
      build.onResolve({ filter: /gpuix-web(?:_bg\.wasm|\.js)$/ }, ({ path }) => ({
        path: `/unexpected-default/${path.split("/").at(-1)}`, external: true,
      }))
    },
  }],
})
if (!result.success) throw new AggregateError(result.logs, "Composition bundle failed")
const headers = {
  "Cross-Origin-Opener-Policy": "same-origin",
  "Cross-Origin-Embedder-Policy": "require-corp",
}
const server = Bun.serve({
  hostname: "127.0.0.1", port: Number(process.env.PORT ?? 4187),
  async fetch(request) {
    const pathname = new URL(request.url).pathname
    if (pathname === "/") return new Response(
      '<!doctype html><html><head><meta charset="utf-8"><title>GPUiX composition test</title><style>html,body{margin:0;padding:0}</style></head><body><script type="module" src="/browser.js"></script></body></html>',
      { headers: { ...headers, "Content-Type": "text/html" } },
    )
    if (pathname.startsWith("/unexpected-default/")) {
      console.error("Unexpected default runtime request", pathname)
      return new Response("The selected composition must own the runtime", { status: 500, headers })
    }
    const file = resolve(output, `.${pathname}`)
    if (!file.startsWith(output + sep)) return new Response("Not found", { status: 404, headers })
    const asset = Bun.file(file)
    return await asset.exists() ? new Response(asset, { headers }) : new Response("Not found", { status: 404, headers })
  },
})
console.log(`Composition test server: ${server.url}`)
