---
"@gpuix/native": patch
"@gpuix/react": patch
---

Remove Cherry component implementations and registrations from the core runtime.
Applications load them through a compiled native extension. Keep the standard
core elements and export the generated native runtime contract metadata.
