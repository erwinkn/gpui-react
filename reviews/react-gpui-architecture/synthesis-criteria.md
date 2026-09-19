# Review comparison criteria

These criteria were recorded before reading the four reports. They do not select a winner by model reputation or by majority vote.

1. Does the proposed ownership model preserve native scroll/input/IME/motion under a blocked application runtime?
2. Does it improve actual work ownership, or only rename the current duplicate tree?
3. Does it distinguish native document revisions from UI-owned interaction/resource state?
4. Does its off-thread layout claim account for actual GPUI Window/App-dependent elements, speculative virtual rows, text measurement, and custom Rust components?
5. Does it define fresh measurement and React layout-effect semantics honestly?
6. Can it reconcile a prepared revision with newer native input, resize, fonts, and scrolling without lost updates or stale geometry?
7. Does visual coalescing preserve imperative commands, event ordering, and component lifetimes?
8. Is the claimed UI-thread publication bound credible for large tree changes, destruction, accessibility updates, and native custom components?
9. Are memory growth, unacknowledged revisions, reclamation, and burst behavior explicitly bounded?
10. Are API and transport choices separated from thread scheduling, and are runtime portability costs explicit?
11. Does it support independent native editors/effects without putting consumer dependencies in core?
12. Does it identify a decisive prototype and measurable failure threshold, rather than claiming an unmeasured speedup?

Synthesis should separate:
- independent agreement;
- substantive architectural disagreement;
- source-backed constraints;
- speculative implementation claims;
- a recommended next experiment, not an unrequested rewrite.
