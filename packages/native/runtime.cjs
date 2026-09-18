// This entry imports no native or WebAssembly implementation. Applications use
// it before importing React so one selected composition owns the UI runtime.
const KEY = Symbol.for("gpuix.native.runtime.v1");

function validate(bindings) {
  if (!bindings || typeof bindings.GpuixRenderer !== "function") {
    throw new Error("GPUiX bindings must export GpuixRenderer");
  }
  if (typeof bindings.nativeRuntimeInfo !== "function") {
    throw new Error(
      "GPUiX bindings have no runtime contract; rebuild the composition with the matching GPUiX fork",
    );
  }
  const info = JSON.parse(bindings.nativeRuntimeInfo());
  if (info.apiVersion !== 1 || info.extensionApiVersion !== 1) {
    throw new Error(
      "GPUiX native runtime contract mismatch; use a compatible React and native build",
    );
  }
  return info;
}

function configureNativeBindings(bindings) {
  const installed = globalThis[KEY];
  if (installed) {
    if (installed.bindings !== bindings) {
      throw new Error(
        "GPUiX already has a native binding. Configure one composition before importing @gpuix/react",
      );
    }
    return installed.info;
  }
  const info = validate(bindings);
  globalThis[KEY] = { bindings, info };
  return info;
}

function getNativeBindings(loadDefault) {
  const installed = globalThis[KEY];
  if (installed) return installed.bindings;
  if (!loadDefault) return undefined;
  const bindings = loadDefault();
  configureNativeBindings(bindings);
  return bindings;
}

module.exports = { configureNativeBindings, getNativeBindings };
