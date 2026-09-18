---
"@gpuix/native": patch
"@gpuix/react": patch
---

Wake the macOS embedded event pump from the display signal. Keep a bounded timer fallback and yield before pumping from a N-API callback. Add opt-in frame timings and request missing virtual rows after a programmatic jump. Requires the matching GPUI production/test scheduling fix.

Bound each macOS AppKit event drain and avoid pumping the same run loop twice per host tick. Add native AppKit queue testing and isolated, nonactivating frame captures.
