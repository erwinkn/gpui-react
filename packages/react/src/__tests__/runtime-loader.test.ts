import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const runtime = fileURLToPath(
  new URL("../../../native/runtime.cjs", import.meta.url),
);
const loader = fileURLToPath(
  new URL("../../../native/loader.cjs", import.meta.url),
);
const generated = fileURLToPath(
  new URL("../../../native/index.js", import.meta.url),
);
const run = (body: string) =>
  execFileSync(
    "node",
    [
      "-e",
      `
  const assert = require("node:assert/strict");
  const api = require(${JSON.stringify(runtime)});
  const bindings = {
    GpuixRenderer: class Renderer {},
    nativeRuntimeInfo: () => JSON.stringify({apiVersion: 1, extensionApiVersion: 1, version: "test", extensions: []}),
  };
  ${body}
`,
    ],
    { encoding: "utf8" },
  );

describe("native composition loader", () => {
  it("loads no native library when importing the configuration entry", () => {
    expect(
      run(`assert.equal(api.getNativeBindings(), undefined);
      assert.equal(require.cache[${JSON.stringify(generated)}], undefined);`),
    ).toBe("");
  });

  it("uses one configured composition without loading the default library", () => {
    expect(
      run(`api.configureNativeBindings(bindings);
      assert.equal(require(${JSON.stringify(loader)}).GpuixRenderer, bindings.GpuixRenderer);
      assert.equal(require.cache[${JSON.stringify(generated)}], undefined);
      assert.equal(api.configureNativeBindings(bindings).version, "test");
      assert.throws(() => api.configureNativeBindings({...bindings}), /already has/);`),
    ).toBe("");
  });

  it("exposes configured constructors to Node ESM named imports", () => {
    expect(
      run(`api.configureNativeBindings(bindings);
      import(${JSON.stringify(loader)}).then(module => {
        assert.equal(module.GpuixRenderer, bindings.GpuixRenderer);
        assert.equal(module.nativeRuntimeInfo, bindings.nativeRuntimeInfo);
      });`),
    ).toBe("");
  });

  it("rejects incompatible bindings without poisoning the next registration", () => {
    expect(
      run(`assert.throws(() => api.configureNativeBindings({}), /GpuixRenderer/);
      assert.throws(() => api.configureNativeBindings({GpuixRenderer: class {}}), /no runtime contract/);
      assert.throws(() => api.configureNativeBindings({...bindings, nativeRuntimeInfo: () => '{"apiVersion":9}'}), /mismatch/);
      assert.equal(api.getNativeBindings(), undefined);
      api.configureNativeBindings(bindings);`),
    ).toBe("");
  });
});
