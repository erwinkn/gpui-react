# React on GPUI architecture review

Start with the [accepted architecture and scenario matrix](scenario-matrix.md). It records the user's decision to use one retained native host model, ordinary GPUI layout, and a small React-to-native translation layer. It maps 18 scenario groups and 15 recorded Cherry issues to that design, with proposed acceptance tests.

Events, updates, and live native queries cross an asynchronous boundary. Components that need current-frame layout run that logic natively and expose a React wrapper. This is an accepted trade-off, not a pending architecture question.

The [concrete examples](examples.md) and [earlier comparison](synthesis.md) record the alternatives considered before that decision. Their recommendations are historical, not the current direction. No new architecture has been implemented or benchmarked in this directory.

These reports examine a possible redesign. They do not document implemented APIs or measured performance gains. The four reviewers inspected GPUiX at 08ffd4bfd70c638be6a8025548ba4234a5ff1edd and GPUI at bea32f070b9fe5286081f1ea8730ad3eca890ba2.

| Report | Final recommendation |
| --- | --- |
| [Fable 5.1 Extra High](fable.md) | One mutable UI-owned tree, ordered typed transactions, no worker Rust tree |
| [DeepSeek V4.1 Flash](deepseek.md) | Shared immutable native descriptions, UI-owned interaction state |
| [GPT-6 Astra Extra High](astra.md) | Shared immutable descriptions; UI layout by default; examine pure layout and adoption |
| [Gemini 3.8 Flash](gemini.md) | Shared immutable descriptions, worker preparation, UI-owned layout and interaction |

The reports include correction passes. The synthesis separates source evidence from untested proposals and identifies remaining weaknesses. Read it before treating a review's API sketch as a viable implementation.

Supporting material:

- [Common review brief](brief.md)
- [Comparison criteria](synthesis-criteria.md)
- [Parent source checks and corrections](parent-source-checks.md)

The original reports were saved in BB thread storage. This directory is a worktree copy so the reports are visible in the repository file tree. No benchmark implementation is included.
