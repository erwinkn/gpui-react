/** Install release archives in isolation and test real default/selected loaders. */
import assert from "node:assert/strict"
import { createHash } from "node:crypto"
import { execFileSync } from "node:child_process"
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"

const artifacts = resolve(process.argv[2] ?? "dist/runtime")
const manifest = JSON.parse(readFileSync(join(artifacts, "runtime-manifest.json"), "utf8"))
for (const item of manifest.packages) {
  assert.equal(createHash("sha256").update(readFileSync(join(artifacts, item.filename))).digest("hex"), item.sha256)
}
const directory = mkdtempSync(join(tmpdir(), "gpuix-package-install-"))
const composition = resolve("fixtures/native-composition/example.node")
const archives = Object.fromEntries(manifest.packages.map((item: {name: string; filename: string}) => [item.name, `file:${join(artifacts, item.filename)}`]))
try {
  writeFileSync(join(directory, "package.json"), JSON.stringify({
    private: true, type: "module", dependencies: { ...archives, react: "19.2.4" },
    overrides: { "@gpuix/native": "$@gpuix/native" },
  }))
  execFileSync("npm", ["install", "--ignore-scripts", "--no-audit", "--no-fund"], {
    cwd: directory, encoding: "utf8", timeout: 120000,
  })
  for (const command of ["node", "bun"]) {
    for (const selected of [false, true]) {
      const script = `
        import assert from 'node:assert/strict';
        import {createRequire} from 'node:module';
        const load=createRequire(process.cwd()+'/probe.cjs');
        const selected=${selected};
        let bindings;
        if(selected) {
          bindings=load(${JSON.stringify(composition)});
          const {configureNativeBindings}=await import('@gpuix/native/runtime');
          configureNativeBindings(bindings);
        }
        const native=await import('@gpuix/native');
        const {createElement}=await import('react');
        const {createTestRoot}=await import('@gpuix/react/testing');
        const info=JSON.parse(native.nativeRuntimeInfo());
        assert.equal(info.apiVersion,1);
        assert.equal(info.extensions.length,selected?1:0);
        if(selected) {
          assert.equal(native.GpuixRenderer,bindings.GpuixRenderer);
          const nativePath=load.resolve('@gpuix/native/package.json');
          assert.equal(load.cache[nativePath.replace('package.json','index.js')],undefined);
        }
        const root=createTestRoot();
        root.render(selected
          ? createElement('example-gpu-texture',{label:'Package composition',color:[0,1,0,1],style:{width:120,height:60}})
          : createElement('text',null,'Package core'));
        assert(root.renderer.getPaintedText().includes(selected?'Package composition':'Package core'));
        console.log('package loader passed');
      `
      const result = execFileSync(command, ["--input-type=module", "--eval", script], {
        cwd: directory, encoding: "utf8", env: { ...process.env, GPUIX_BACKGROUND: "1" }, timeout: 15000,
      })
      assert(result.includes("package loader passed"))
    }
  }
  console.log("Installed archives passed Node and Bun default/selected composition tests")
} finally {
  rmSync(directory, { recursive: true, force: true })
}
