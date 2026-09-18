---
"@gpuix/native": patch
"@gpuix/react": patch
---

Expose shared GPUI horizontal scroll handles through `scrollGroup`. This keeps table rows, headers, and footers in the same native scroll frame. Restrict vertical-only scroll containers to their declared axis so nested horizontal panes do not also scroll vertically.
