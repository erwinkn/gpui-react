/** Browser entry. A configured composition bypasses the default WASM module. */
import runtime from "./runtime.cjs";

let bindings = runtime.getNativeBindings();
if (!bindings) {
  const implementation = await import("./wasm/gpuix-web.js");
  const { default: wasmUrl } = await import("./wasm/gpuix-web_bg.wasm", {
    with: { type: "file" },
  });
  await implementation.default({ module_or_path: wasmUrl });
  runtime.configureNativeBindings(implementation);
  bindings = implementation;
}

export const GpuixRenderer = bindings.GpuixRenderer;
export const nativeRuntimeInfo = bindings.nativeRuntimeInfo;
