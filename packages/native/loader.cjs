const { getNativeBindings } = require("./runtime.cjs");
const bindings = getNativeBindings(() => require("./index.js"));
module.exports = { ...bindings };
// Explicit names let Node's ESM loader discover the core CommonJS exports.
module.exports.GpuixRenderer = bindings.GpuixRenderer;
module.exports.TestGpuixRenderer = bindings.TestGpuixRenderer;
module.exports.NativeHost = bindings.NativeHost;
module.exports.NativeClient = bindings.NativeClient;
module.exports.hasTestGpuixRenderer = bindings.hasTestGpuixRenderer;
module.exports.nativeRuntimeInfo = bindings.nativeRuntimeInfo;
module.exports.checkUpdate = bindings.checkUpdate;
module.exports.InstallUpdateTask = bindings.InstallUpdateTask;
module.exports.CheckUpdateTask = bindings.CheckUpdateTask;
module.exports.AvailableUpdate = bindings.AvailableUpdate;
