# Asynchronous React to GPUI bridge

This is the new React integration, developed alongside the existing GPUiX
packages. The native host connection is still in progress. The tests currently
exercise the real React reconciler against a recording transport.

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

The transport implements `send(encodedTransaction)`, `subscribe(receiver)`, and
`close(reason)`. It preserves transaction order and delivers earlier events
before the acknowledgement that retires their subscriptions. Query and command
results carry request IDs. Missing results or invalid acknowledgement order
fail the root. An individual command error rejects its promise.

The reconciler retains only speculative child descriptions until their commit.
After mounting, native code owns child topology. Native removal reports retired
subscriptions, so the worker needs no native tree to discover removed callbacks.
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
