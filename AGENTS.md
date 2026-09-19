# AGENTS.md - GPUI React Bridge Guide

**Read [README.md](./README.md) first** to understand the architecture, crates, packages, controls, and benchmarks.

Unless you are **remorses** or **monotykamary**, do not open a pull request. Open an issue.

## README is the public API contract

Document every user-facing feature, element, prop, event, renderer option,
public method, and behavior change in `README.md` in the same change.

## GPUI is the real API

**Read the GPUI documentation and the GPUI source code before you write native code.**
`zed/crates/gpui` is checked out in this repository. `gpui::ListState`, `gpui::div`,
`gpui::Window`, and related types are the real API. The bridge translates a
React tree into calls on them.

Do not invent behavior on top of GPUI. When a bridge element needs functionality
that GPUI does not provide, follow this sequence:

1. Find the GPUI API that already does it. Search `zed/crates/gpui` for the symbol.
2. Search `zed-industries/zed` issues and pull requests.
3. Fix the issue in the `erwinkn/zed` fork as an ordinary GPUI change, and update the submodule pointer.
4. Only then, write bridge code.

**Never paper over GPUI in native crates.** A workaround that re-applies state after
GPUI calculated it, patches a value that GPUI owns, or circumvents a GPUI invariant
will fail on the next submodule update. When such a change is necessary, write
a comment that explains what GPUI does, why the bridge requires different behavior,
and which GPUI call ensures safety.

Prefer the smallest translation. Keep the system simple with few moving parts.

## Architecture and Core Invariants

### One Host Entity, One Tree

- GPUI retains entity state, not an element tree.
- One `Host` entity per window root holds the committed React tree as plain data.
- The host rebuilds ephemeral GPUI elements from that tree on every frame.
- There is no second native tree and no worker-side Rust element tree.

### Worker-Side Decoding

- The UI thread parses no JSON.
- A worker-side `Decoder` parses transaction text into strongly typed operations.
- Operations map component names to registry indices without intermediate value allocations.
- Shared styles are sent once by id. Style operations never cross to the UI thread.

### Native State Ownership

- Components with native state (text input, list scroll positions, GPU textures) are ordinary GPUI `Render` entities.
- Ephemeral containers and text leaves are host-owned data.
- Text input owns live text, caret position, selection, IME composition, and undo history.
- React props provide initial values. Application replacements require the expected revision.

### Frame Metadata and Geometry

- Call `current_frame(window, cx)` during the paint phase to read `FrameInfo` (draw number, transaction commit, viewport size, scale factor).
- Record bounds and element measurements during **paint**, never during prepaint or speculative layout.
- Cached paint replay skips callbacks; do not rely on layout-phase measurements.

### Event and Subscription Ordering

- Subscription IDs are unique and never reused.
- Native events follow GPUI effect order.
- An event emitted before a subscription change keeps the old callback.
- Removing a node does not deliver pending events to a replacement node that reuses the host ID.

## Submodule Management (`zed/`)

**Always keep `zed/` checked out on the local `bridge/framework-runtime` branch. Never leave the submodule in detached HEAD state.**

- Do not modify or commit inside `zed/` within this checkout.
- Work on GPUI changes in an external Zed worktree.
- Push changes to the `bridge/framework-runtime` branch of `erwinkn/zed`.
- Fast-forward the submodule in this repository to the reachable remote commit.

## Development Rules

### Process Execution and Display

- Every window opens with `focus: false`, in the host and in the benchmark. Windows stay in the background and do not take focus or keyboard input from the user.
- Keep this rule for every new window: do not activate the application from tests, examples, or benchmarks.

### Standard Build Settings

Use the common target directory and job limits for every Cargo command:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 cargo <command>
```

### Verification Commands

#### Rust Crates

Test core crates:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo test --manifest-path crates/gpui-react/Cargo.toml
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo test --manifest-path crates/gpui-react-controls/Cargo.toml
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo test --manifest-path crates/gpui-react-host/Cargo.toml
```

Check the runtime and fixtures:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo check --manifest-path crates/gpui-react-runtime/Cargo.toml
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo check --manifest-path fixtures/bridge-performance/Cargo.toml
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo check --manifest-path fixtures/bridge-performance/Cargo.toml --features allocation-counts
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo check --manifest-path fixtures/bridge-gpu-component/Cargo.toml
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo check --manifest-path fixtures/bridge-counter/Cargo.toml
```

Run Clippy:

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --all-targets --all-features --manifest-path crates/gpui-react/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --all-targets --all-features --manifest-path crates/gpui-react-macros/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --all-targets --all-features --manifest-path crates/gpui-react-controls/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --all-targets --all-features --manifest-path crates/gpui-react-host/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --all-targets --all-features --manifest-path crates/gpui-react-runtime/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --all-targets --all-features --manifest-path fixtures/bridge-performance/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --all-targets --all-features --manifest-path fixtures/bridge-gpu-component/Cargo.toml -- -D warnings
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo clippy --all-targets --all-features --manifest-path fixtures/bridge-counter/Cargo.toml -- -D warnings
```

#### JavaScript Packages

```sh
bun install
bun run --cwd packages/bridge build
bun run --cwd packages/bridge test
bun run --cwd packages/bridge-controls build
```

#### End-to-End Counter Fixture

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  cargo build --manifest-path fixtures/bridge-counter/Cargo.toml --release
cp /tmp/gpui-react-target/release/libgpui_react_counter_example.dylib fixtures/bridge-counter/counter.node
bun fixtures/bridge-counter/test.ts
```

#### Frame Cost Measurement

```sh
CARGO_TARGET_DIR=/tmp/gpui-react-target CARGO_BUILD_JOBS=3 \
  bun scripts/measure-bridge-frames.ts /tmp/gpui-react-frame-cost
```
