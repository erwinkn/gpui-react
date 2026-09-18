---
"@gpuix/native": minor
"@gpuix/react": minor
---

Add explicit application-worker startup on macOS. AppKit owns the main-thread event loop. React sends bounded native transactions from a worker. Native input and animation continue during blocked application JavaScript. Add asynchronous native queries, ordered event delivery, first-scene presentation, and shutdown handling. The existing embedded startup and Windows/Linux paths remain available.

Requires the matching GPUI inactive-window and embedded lifecycle changes. Upstream Zed changes 60574 and 63235 provide application handles for externally driven loops; they do not provide this worker boundary.
