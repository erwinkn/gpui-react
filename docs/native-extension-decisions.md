# Native extension decisions

This checkpoint adds the public Rust contract. Application components will move
to consumer crates in the next integration step.

| Decision | Alternative | Confidence | Failure case |
| --- | --- | --- | --- |
| Expose the existing GPUI view context through an opaque `NativeView` type. | Introduce a separate entity wrapper and context abstraction. | Medium | A future renderer owner change requires a new extension API version. |
| Link extension crates into one native library. | Load native plugins dynamically. | High | A user needs to install new native code without rebuilding the runtime. Dynamic ABI support is not included. |
| Freeze registrations after the first renderer. | Mutate the registry while windows are active. | High | A late-loading native extension receives an explicit error and must restart with a rebuilt composition. |
| Re-export GPUI and shared services. | Let each extension select a GPUI revision or implement its own text and bounds helpers. | High | A consumer ignores the re-export and selects incompatible framework types; compilation fails. |
| Allow identical registration after startup. | Reject all repeat registration. | High | Host and worker imports must share one catalog. Different implementations still fail. |
| Reserve lowercase hyphenated names for extensions. | Allow replacement of built-in host names. | High | An extension must rename a conflicting element. Overrides belong in React/application composition. |
| Configure the binding before React imports, in each JavaScript context. | Embed an app-specific native dependency in each component library. | High | A host imports React too early; the loader selects its default and rejects the later replacement. The error includes the required import order. |

Validation at this checkpoint: an external composition release build retained
GPUiX's N-API and worker-host exports. A native offscreen test passed text paint,
exact bounds, click delivery, text updates without geometry change, and removal.
The rendered image was inspected. Three Rust registration tests and four loader
tests passed. The loader tests include Node ESM named imports, default-load
avoidance, incompatible bindings, and replacement rejection. Full application
and browser composition validation follows extraction; this checkpoint does not
claim those later checks.

The full React run had 455 passes and two failures. The known nested-scroll
callback regression remains in the prior GPUI patch. The new loader also exposed
a Bun compiled-output error with `Object.assign(module.exports, bindings)`.
Using an explicit CommonJS export assignment corrected that failure. The loader
checks and relocated worker test then passed, five checks in that focused run.
The React TypeScript build passed. The final composition metadata was exercised
with the real native library and configured loader.

I stand behind this extension-contract checkpoint. I do not consider the full
fork ready for release while the scroll regression and component extraction are
unfinished. Browser composition and the reduced GPUI patch remain validation
work for the integration, not claims of this checkpoint.

The user authorized autonomous implementation and checkpoint commits. This audit
records the decisions for review without blocking that work. Validation results
are recorded with the tested checkpoint.
