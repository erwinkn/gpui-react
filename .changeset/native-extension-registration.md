---
"@gpuix/native": minor
---

Expose a Rust extension API for native components compiled in external crates. Register a fixed set before renderer startup, reject incompatible or conflicting registrations, and share GPUI, text, style, event, accessibility, and bounds services through one public module.

Add `@gpuix/native/runtime` to select one application composition before React imports it. Desktop and browser entries use the selected binding without initializing their default runtime. Reject incompatible or replacement bindings.
