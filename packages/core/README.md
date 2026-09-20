# Asynchronous React to GPUI bridge

This is the React reconciler for GPUI. It includes an explicit macOS/Bun
native host connection and can wrap ordinary GPUI views from a compiled
composition. It knows no control names: the typed wrappers for the five
standard controls are in [`@gpui-react/kit`](../kit/README.md), and their full
prop, event, command, and style contract is in
[`packages/kit/CONTROLS.md`](../kit/CONTROLS.md). The default
`@gpui-react/runtime` package composes the engine with the kit. Release
publication and broader performance checks remain in progress.

```tsx
import { createRoot, nativeComponent } from '@gpui-react/core'

const Counter = nativeComponent<
  { step: number },
  { value: number }
>('counter')

const root = createRoot(transport)
root.render(<Counter step={2} onEvent={event => console.log(event.value)} />)
```

`nativeComponent<Props, Event, Command, Query, Reply>(name)` creates an ordinary
React wrapper. Props must be JSON data. `children`, `ref`, and `onEvent` have
bridge semantics. A ref has a stable `id`, asynchronous `command(value)`, and
asynchronous `query(value)`. It has no synchronous native state getter. Commands
and query types can be supplied through the additional generic parameters.
The wrapper returns `ReactElement`; its native props do not inherit DOM
attribute or CSS type constraints.

`createRoot(transport, options?)` binds a session to one root. `textKind`
(default `"text"`) names the native kind that renders a string or number
child as `{ text }` props; with a `schema` present and no such kind, the first
string child fails the root with an error that names the option. `render` schedules
React work. `renderSync` flushes React only; it does not wait for native work.
`flush` waits for the already collected native transactions, not a future React
render or native presentation. `unmount` commits removal, awaits application,
then closes the session. A closed transport cannot be reused with reset IDs.
`dispose` runs React cleanup after native failure/shutdown without sending more
native operations. It rejects outstanding requests and closes the transport.

The transport implements `send(encodedTransaction)`, `subscribe(receiver, onError?)`, and
`close(reason)`. It preserves transaction order and delivers earlier events
before the acknowledgement that retires their subscriptions. Query and command
results carry request IDs. Missing results or invalid acknowledgement order
fail the root. An individual command error rejects its promise.
It does not undo native state changes that occurred before the error.
Transport failure also reaches the root when no request is pending.

The reconciler retains only speculative child descriptions until their commit.
After mounting, native code owns child topology. Native removal reports retired
subscriptions, so the worker needs no native tree to discover removed callbacks.
Host IDs are allocated at commit, in native creation order. Abandoned render
descriptions allocate no native IDs or component instances.
Callback changes do not resend unchanged native props, and structurally equal
props are not resent even when React created a new object for them, so an
inline style literal costs nothing after its first commit. Props follow React's
immutable-update convention; changing an object in place is unsupported.
Every prop, command, and query operation names its component so the native
worker can decode it to typed data before it reaches the UI thread. A `style`
object is sent once as a `style` definition and referenced by id afterwards;
the root keeps up to 4,096 definitions and drops the least recently used. Host
ids are reused once the transaction that removed a node is acknowledged; a ref
whose node has unmounted rejects further commands and queries.

Mutations and synchronous layout-effect commands are sealed together in a
microtask. Native events use subscription IDs, so callback versions remain
available until native retirement. Each encoded transaction is retained until
native acknowledgement. There is one in-flight send per root.

Callback selection follows native effect order, not React render time or display
presentation. An event emitted before a native subscription change keeps its
old callback. An event emitted after that change uses the new callback. GPUI can
still hit-test the last drawn frame while native component state is newer.
The binding preserves that normal GPUI behavior. It does not promise that a
callback or live state matches the pixels from the previous draw. After unmount,
a listener left in an older frame cannot deliver an event to a replacement host
ID. Components should include the relevant native identity and state in their
event payloads when handlers need them. Queries and painted snapshots keep their
separate version rules.

The default limits are 256 pending transactions and 4 MiB of encoded data.
`maxPending` and `maxBytes` override them. Saturation fails the root explicitly;
silently dropping a React commit would desynchronize the renderer. `onError`
receives root failures and event-handler exceptions. The default logs errors.

```sh
bun run --cwd packages/core build
bun run --cwd packages/core test
```

The tests cover commit/effect grouping, asynchronous refs, abandoned Suspense
work, keyed movement, text removal, callback versions, session reuse, queue
failure, invalid values, and missing native replies. Full native component,
platform, and installed-package checks are documented in the repository README.

## Native application entries

Select one compiled composition explicitly in both entry files:

For the standard controls, use `import bindings from '@gpui-react/runtime'`
in both files and import the wrappers from `@gpui-react/kit`. Install matching
versions of `@gpui-react/core`, `@gpui-react/kit`, and `@gpui-react/runtime`. A custom
composition uses the literal native path shown below instead. The default
runtime has no browser driver. It supports macOS arm64 with Bun.

```ts
// host.ts
import { runApplication } from '@gpui-react/core/application'
const bindings = require('./app-runtime.node')
await runApplication(bindings, new URL('./worker.tsx', import.meta.url), {
  title: 'My app', width: 800, height: 600,
})
```

```tsx
// worker.tsx
import { attachApplication } from '@gpui-react/core/application'
const bindings = require('./app-runtime.node')
const root = attachApplication(bindings)
root.render(<App />)
```

The launcher enters AppKit's native loop. It never pumps that loop from a JS
timer. Native events and acknowledgements cross a bounded queue; the worker
holds no Rust tree. The window starts hidden and is shown inactive after the
first applied transaction and native draw. `show: false` keeps it hidden. This
API does not activate the app. Closing the native window, unmounting the root,
or worker failure ends the session. Worker attachment has a 10-second deadline;
shutdown allows two seconds for worker cleanup before termination. A responsive
worker runs React effect and process exit cleanup. A blocked worker cannot run
that cleanup; native resources still close. SIGINT and SIGTERM request graceful
native shutdown and let `runApplication` return. After the host ends, default
process signal behavior applies. Per-signal JS listeners are not forwarded.

Use static binary paths and include both entries in a Bun compilation. The
[counter fixture](../../fixtures/counter/README.md) contains source and
relocated executable checks. This host currently supports macOS with Bun;
cross-platform host and browser drivers remain separate work.

Native measurement replies can include the exported `FrameInfo` type. Its
`root` string identifies the native host within its application session; `frame` identifies a draw within it;
`commit` identifies the latest native transaction incorporated into that draw.
The other fields are `viewportWidth`, `viewportHeight`, and `scaleFactor`.
A matching component prop revision alone does not establish fresh geometry:
parent styles and viewport changes can also affect layout. Frame tags describe
GPUI paint work and do not acknowledge physical display presentation.
