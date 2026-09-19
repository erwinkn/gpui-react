function unsupported() {
  throw new Error("@gpuix/bridge-runtime is a native desktop runtime. The new bridge has no browser driver yet; retain the existing @gpuix/react browser integration.")
}
export const bridgeRuntimeVersion = unsupported, NativeHost = unsupported, NativeClient = unsupported
export default { bridgeRuntimeVersion, NativeHost, NativeClient }
unsupported()
