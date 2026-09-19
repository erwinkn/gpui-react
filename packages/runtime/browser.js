function unsupported() {
  throw new Error("@gpui-react/runtime is a native desktop runtime. The bridge currently has no browser driver.")
}
export const bridgeRuntimeVersion = unsupported, NativeHost = unsupported, NativeClient = unsupported
export default { bridgeRuntimeVersion, NativeHost, NativeClient }
unsupported()
