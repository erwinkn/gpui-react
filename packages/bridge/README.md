# Asynchronous React to GPUI bridge

This is the new React integration, developed alongside the existing GPUiX
packages. It includes an explicit macOS/Bun native host connection and can wrap
ordinary GPUI views from a compiled composition. The complete native component
library and release distribution remain in progress.

```tsx
import { createRoot, nativeComponent } from '@gpuix/bridge'

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

`createRoot(transport, options?)` binds a session to one root. `render` schedules
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
Callback changes do not resend unchanged native props. Props follow React's
immutable-update convention; changing an object in place is unsupported.

Mutations and synchronous layout-effect commands are sealed together in a
microtask. Native events use subscription IDs, so callback versions remain
available until native retirement. Each encoded transaction is retained until
native acknowledgement. There is one in-flight send per root.

The default limits are 256 pending transactions and 4 MiB of encoded data.
`maxPending` and `maxBytes` override them. Saturation fails the root explicitly;
silently dropping a React commit would desynchronize the renderer. `onError`
receives root failures and event-handler exceptions. The default logs errors.

```sh
bun run --cwd packages/bridge build
bun run --cwd packages/bridge test
```

The tests cover commit/effect grouping, asynchronous refs, abandoned Suspense
work, keyed movement, text removal, callback versions, session reuse, queue
failure, invalid values, and missing native replies. Full native component,
platform, and installed-package validation remains required before release.

## Native application entries

Select one compiled composition explicitly in both entry files:

```ts
// host.ts
import { runApplication } from '@gpuix/bridge/application'
const bindings = require('./app-runtime.node')
await runApplication(bindings, new URL('./worker.tsx', import.meta.url), {
  title: 'My app', width: 800, height: 600,
})
```

```tsx
// worker.tsx
import { attachApplication } from '@gpuix/bridge/application'
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
shutdown allows two seconds for worker cleanup before termination. These limits
match the tested earlier host, but broader lifecycle coverage remains pending.

Use static binary paths and include both entries in a Bun compilation. The
[counter fixture](../../fixtures/bridge-counter/README.md) contains source and
relocated executable checks. This host currently supports macOS with Bun;
cross-platform host and browser drivers remain separate work.

Native measurement replies can include the exported `FrameInfo` type. Its
`root` string identifies the native host within its application session; `frame` identifies a draw within it;
`commit` identifies the latest native transaction incorporated into that draw.
The other fields are `viewportWidth`, `viewportHeight`, and `scaleFactor`.
A matching component prop revision alone does not establish fresh geometry:
parent styles and viewport changes can also affect layout. Frame tags describe
GPUI paint work and do not acknowledge physical display presentation.
