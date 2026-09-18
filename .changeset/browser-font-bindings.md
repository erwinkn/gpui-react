---
"@gpuix/native": patch
---

Expose private font registration and text measurement to the WebAssembly renderer. These methods use the application's text system before the graphics window is ready, so the first layout uses the supplied fonts. Keep native-only N-API annotations out of the browser syntax module.
