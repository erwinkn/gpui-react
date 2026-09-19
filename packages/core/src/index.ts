import React, { createContext, createElement, type ReactNode, type Ref } from "react"
import ReactReconciler from "react-reconciler"
import { ConcurrentRoot, DefaultEventPriority } from "react-reconciler/constants.js"
import type { KindSchema, NativeEvent, NativeRef, TransactionReply, Transport } from "./protocol.js"
import { BinaryEncoder, JsonEncoder, validate, type Encoded, type Encoder } from "./wire.js"
export type { FrameInfo, KindSchema, NativeEvent, NativeRef, Operation, Transaction, TransactionReply, Transport, WireField, WireType } from "./protocol.js"
export { decodeWire } from "./wire.js"
export * from "./controls.js"

export function nativeComponent<Props extends object, Event = never, Command = unknown, Query = unknown, Reply = unknown>(name: string) {
  if (!/^[a-z0-9-]+$/.test(name)) throw Error("Invalid native component name")
  return function NativeComponent(props: Props & { children?: ReactNode; onEvent?: (event: Event) => void; ref?: Ref<NativeRef<Command, Query, Reply>> }): React.ReactElement {
    return createElement(name, props)
  }
}

interface Options {
  maxPending?: number
  maxBytes?: number
  onError?: (error: Error) => void
  /** The native component table, from `NativeClient.schema()`. Absent for transports without one. */
  schema?: KindSchema[]
  /** Transaction encoding. The binary wire needs the schema. */
  wire?: "json" | "binary"
  /** Binary wire: check integers and finite floats before writing. On by default; the cost is within noise. */
  wireChecks?: boolean
}
type Props = Record<string, any>
type PendingCall = { resolve: (value: any) => void; reject: (error: Error) => void }
interface Host {
  id: number
  component: string
  /** Index into the native kind table, or -1 without a schema. */
  kind: number
  props: Props
  root: BridgeRoot
  initial: Host[]
  mounted: boolean
  subscription: number | null
  public: NativeRef | null
}

const attached = new WeakSet<Transport>()

export class BridgeRoot {
  private nextId = 0
  private freeIds: number[] = []
  private nextStyle = 0
  private freeStyles: number[] = []
  /** Style text → wire id. Bounded; the oldest definition is dropped. */
  private styleIds = new Map<string, number>()
  /** Identity fast path: a memoized component hands back the same object. */
  private styleByObject = new WeakMap<object, number>()
  private liveStyles = new Set<number>()
  private nextSubscription = 0
  private nextRequest = 0
  private sequence = 0
  private scheduled = false
  private pending = 0
  private bytes = 0
  private removed: number[] = []
  private tail: Promise<void> = Promise.resolve()
  private failure: Error | null = null
  private disposed = false
  private callbacks = new Map<number, { target: number; fn: (value: unknown) => void }>()
  private calls = new Map<number, PendingCall>()
  private flushWaiters = new Set<PendingCall>()
  private unsubscribe: () => void
  private container: any
  private maxPending: number
  private maxBytes: number
  private onError: (error: Error) => void
  /** Kind index by component name, when the transport supplied a schema. */
  readonly kinds: ReadonlyMap<string, number> | null
  readonly wire: "json" | "binary"
  /** Commit hooks record through this after `ready()`. */
  readonly encoder: Encoder

  constructor(private transport: Transport, options: Options = {}) {
    if (attached.has(transport)) throw Error("Transport already has a React root")
    this.kinds = options.schema ? new Map(options.schema.map((kind, index) => [kind.name, index])) : null
    this.wire = options.wire ?? "json"
    if (this.wire !== "json" && !options.schema) throw Error("The binary wire needs the native component schema")
    const styleId = (style: object) => this.styleId(style)
    this.encoder = this.wire === "binary" ? new BinaryEncoder(options.schema!, styleId, options.wireChecks ?? true) : new JsonEncoder(styleId)
    this.maxPending = options.maxPending ?? 256
    this.maxBytes = options.maxBytes ?? 4 * 1024 * 1024
    if (!Number.isSafeInteger(this.maxPending) || this.maxPending < 1 || !Number.isSafeInteger(this.maxBytes) || this.maxBytes < 1) throw Error("Invalid queue limits")
    this.onError = options.onError ?? ((error) => console.error(error))
    attached.add(transport)
    this.unsubscribe = transport.subscribe(event => this.receive(event), error => this.fail(error))
    this.container = reconciler.createContainer(this, ConcurrentRoot, null, false, null, "", (error: Error) => this.fail(error), this.onError, this.onError, null)
  }

  render(node: ReactNode): void {
    this.check()
    reconciler.updateContainer(node, this.container, null, null)
  }

  /** Flush React work only. Native processing and presentation remain async. */
  renderSync(node: ReactNode): void {
    this.check()
    reconciler.flushSyncFromReconciler(() => this.render(node))
  }

  async flush(): Promise<void> {
    this.seal()
    this.check()
    await new Promise<void>((resolve, reject) => {
      const waiter = { resolve, reject }
      this.flushWaiters.add(waiter)
      this.tail.then(() => {
        if (!this.flushWaiters.delete(waiter)) return
        try { this.check(); resolve() } catch (error) { reject(error) }
      }, error => {
        this.flushWaiters.delete(waiter)
        reject(error)
      })
    })
  }

  async unmount(): Promise<void> {
    this.renderSync(null)
    await this.flush()
    this.dispose("React root unmounted")
    // A transport is a session; it must not be reused with reset IDs.
  }

  /** Run React cleanup after host failure or shutdown without attempting to
   * update a closed native session. Native resource cleanup belongs to the host. */
  dispose(reason = "React root disposed"): void {
    if (this.disposed) return
    this.disposed = true
    reconciler.flushSyncFromReconciler(() => reconciler.updateContainer(null, this.container, null, null))
    this.encoder.reset()
    this.unsubscribe()
    this.callbacks.clear()
    const error = this.failure ?? Error(reason)
    for (const call of this.calls.values()) call.reject(error)
    this.calls.clear()
    for (const waiter of this.flushWaiters) waiter.reject(error)
    this.flushWaiters.clear()
    this.transport.close(reason)
  }

  private check(): void {
    if (this.failure) throw this.failure
    if (this.disposed) throw Error("React root is unmounted")
  }

  private fail(reason: unknown): void {
    if (this.failure) return
    this.failure = reason instanceof Error ? reason : Error(String(reason))
    this.encoder.reset()
    for (const call of this.calls.values()) call.reject(this.failure)
    this.calls.clear()
    for (const waiter of this.flushWaiters) waiter.reject(this.failure)
    this.flushWaiters.clear()
    this.callbacks.clear()
    this.unsubscribe?.()
    this.transport.close(this.failure.message)
    this.onError(this.failure)
  }

  private receive(event: NativeEvent): void {
    if (this.failure || this.disposed) return
    const callback = this.callbacks.get(event.subscription)
    if (callback?.target !== event.target) return
    try { callback.fn(event.payload) } catch (error) { this.onError(error instanceof Error ? error : Error(String(error))) }
  }

  /** Whether a commit hook may record now. Schedules the seal. */
  ready(): boolean {
    if (this.disposed) return false
    this.check()
    if (!this.scheduled) {
      this.scheduled = true
      // Seal after synchronous layout effects and their native commands.
      queueMicrotask(() => {
        this.scheduled = false
        try { this.seal() } catch (error) { this.fail(error) }
      })
    }
    return true
  }

  private seal(): void {
    if (!this.encoder.count || this.failure) return
    const sequence = ++this.sequence
    const encoded: Encoded = this.encoder.seal(sequence)!
    const requests = this.encoder.requests()
    const removed = this.removed
    this.removed = []
    const bytes = typeof encoded !== "string" ? encoded.byteLength : typeof Buffer === "function" ? Buffer.byteLength(encoded) : new TextEncoder().encode(encoded).byteLength
    if (this.pending >= this.maxPending || this.bytes + bytes > this.maxBytes) {
      this.fail(Error("Native transaction queue is full; root stopped to prevent lost commits"))
      return
    }
    this.pending++
    this.bytes += bytes
    this.tail = this.tail.then(async () => {
      if (this.failure) return
      try {
        const reply = await this.transport.send(encoded)
        if (reply.sequence !== sequence) throw Error("Native transaction acknowledgement is out of order")
        for (const id of reply.retired) this.callbacks.delete(id)
        // Native has released these nodes; their ids can be reused.
        for (const id of removed) this.freeIds.push(id)
        for (const result of reply.results) {
          const call = this.calls.get(result.request)
          if (!call) throw Error("Unknown native request result")
          this.calls.delete(result.request)
          if (result.error !== undefined) call.reject(Error(result.error))
          else call.resolve(result.value)
        }
        for (const request of requests) {
          if (this.calls.has(request)) throw Error("Missing native request result")
        }
      } catch (error) { this.fail(error) }
      finally { this.pending--; this.bytes -= bytes; this.encoder.release(encoded) }
    })
  }

  host(component: string, props: Props): Host {
    let kind = -1
    if (this.kinds) {
      const known = this.kinds.get(component)
      if (known === undefined) throw Error(`Unknown native component ${component}`)
      kind = known
    }
    return { id: 0, component, kind, props, root: this, initial: [], mounted: false, subscription: null, public: null }
  }

  /** The ref object, created on first request so unreferenced nodes pay nothing. */
  publicInstance(host: Host): NativeRef {
    if (host.public) return host.public
    const call = (op: "command" | "query", value: unknown): Promise<any> => {
      try { this.check() } catch (error) { return Promise.reject(error) }
      if (!host.mounted) return Promise.reject(Error("Native host is unmounted"))
      const request = ++this.nextRequest
      return new Promise((resolve, reject) => {
        try { validate(value) } catch (error) { reject(error); return }
        this.calls.set(request, { resolve, reject })
        if (this.ready()) this.encoder.call(op, host.id, host.kind, host.component, request, value)
      })
    }
    host.public = Object.freeze({ get id() { return host.id }, command: (value: unknown) => call("command", value), query: (value: unknown) => call("query", value) })
    return host.public
  }

  allocateId(): number {
    const free = this.freeIds.pop()
    if (free !== undefined) return free
    if (this.nextId === MAX_ID) throw Error("Native host IDs exhausted")
    return this.nextId++
  }

  /** Release a node's id once the transaction that removed it is acknowledged. */
  released(host: Host): void {
    host.mounted = false
    this.removed.push(host.id)
  }

  /** Style objects are defined once on the wire and referenced by id. */
  styleId(style: object): number {
    const byObject = this.styleByObject.get(style)
    if (byObject !== undefined && this.liveStyles.has(byObject)) return byObject
    // Native serialization is the cheap key. Two literals with different key
    // order get two ids, which is harmless and bounded.
    const text = JSON.stringify(style)
    const known = this.styleIds.get(text)
    if (known !== undefined) {
      this.styleByObject.set(style, known)
      return known
    }
    if (this.styleIds.size >= MAX_STYLES) {
      const [oldest, id] = this.styleIds.entries().next().value!
      this.styleIds.delete(oldest)
      this.liveStyles.delete(id)
      this.freeStyles.push(id)
      this.encoder.dropStyle(id)
    }
    const id = this.freeStyles.pop() ?? this.nextStyle++
    this.styleIds.set(text, id)
    this.liveStyles.add(id)
    this.styleByObject.set(style, id)
    this.encoder.style(id, style)
    return id
  }

  listen(host: Host): number | null {
    if (host.props.onEvent === undefined) return null
    if (typeof host.props.onEvent !== "function") throw Error("onEvent must be a function")
    const subscription = ++this.nextSubscription
    this.callbacks.set(subscription, { target: host.id, fn: host.props.onEvent })
    return subscription
  }
}

const MAX_ID = 0xffff_fffd
const MAX_STYLES = 4096

/** Create a node, placing it in the same operation when its parent is known. */
function materialize(host: Host, parent: Host | BridgeRoot | null = null, before: Host | null = null): void {
  if (host.mounted) return
  host.id = host.root.allocateId()
  host.mounted = true
  host.subscription = host.root.listen(host)
  if (host.root.ready()) {
    host.root.encoder.create(host.id, host.kind, host.component, host.props, host.subscription,
      parent ? (parent instanceof BridgeRoot ? null : parent.id) : undefined, parent && before ? before.id : undefined)
  }
  for (const child of host.initial) place(host, child)
  host.initial = [] // The worker retains no mounted child topology.
}

function place(parent: Host | BridgeRoot, child: Host, before: Host | null = null): void {
  if (!(parent instanceof BridgeRoot)) materialize(parent)
  if (!child.mounted) {
    materialize(child, parent, before)
    return
  }
  if (child.root.ready()) child.root.encoder.place(parent instanceof BridgeRoot ? null : parent.id, child.id, before?.id ?? null)
}

function hide(host: Host): void { if (host.root.ready()) host.root.encoder.hidden(host.id, true) }
function unhide(host: Host): void { if (host.root.ready()) host.root.encoder.hidden(host.id, false) }

let priority = 0
const context = Object.freeze({})
const noop = () => {}
const no = () => false
const config = {
  rendererVersion: "0.1.0", rendererPackageName: "@gpui-react/core",
  supportsMutation: true, supportsPersistence: false, supportsHydration: false,
  isPrimaryRenderer: true, supportsMicrotasks: true, scheduleMicrotask: queueMicrotask,
  createInstance: (type: string, props: Props, root: BridgeRoot) => root.host(type, props),
  createTextInstance: (text: string, root: BridgeRoot) => root.host("text", { text }),
  appendInitialChild: (parent: Host, child: Host) => { parent.initial.push(child) },
  appendChild: place, appendChildToContainer: place,
  insertBefore: place, insertInContainerBefore: place,
  removeChild: (_: Host, child: Host) => { if (child.root.ready()) child.root.encoder.remove(child.id) },
  removeChildFromContainer: (_: BridgeRoot, child: Host) => { if (child.root.ready()) child.root.encoder.remove(child.id) },
  commitUpdate: (host: Host, _: string, oldProps: Props, props: Props) => {
    host.props = props
    const root = host.root
    if (root.ready()) root.encoder.update(host.id, host.kind, host.component, oldProps, props)
    if (oldProps.onEvent !== props.onEvent) {
      host.subscription = root.listen(host)
      if (root.ready()) root.encoder.listen(host.id, host.subscription)
    }
  },
  commitTextUpdate: (host: Host, _: string, text: string) => {
    host.props = { text }
    if (host.root.ready()) host.root.encoder.update(host.id, host.kind, host.component, {}, host.props)
  },
  hideInstance: hide, hideTextInstance: hide, unhideInstance: unhide, unhideTextInstance: unhide,
  getPublicInstance: (host: Host) => host.root.publicInstance(host),
  getRootHostContext: () => context, getChildHostContext: () => context,
  shouldSetTextContent: no, finalizeInitialChildren: no,
  prepareForCommit: () => null, resetAfterCommit: noop,
  // Removal is recursive on the native owner, including host text nodes.
  // React calls this for every host instance in a deleted subtree.
  detachDeletedInstance: (host: Host) => host.root.released(host),
  clearContainer: noop, commitMount: noop,
  scheduleTimeout: setTimeout, cancelTimeout: clearTimeout, noTimeout: -1,
  setCurrentUpdatePriority: (value: number) => { priority = value },
  getCurrentUpdatePriority: () => priority,
  resolveUpdatePriority: () => priority || DefaultEventPriority,
  shouldAttemptEagerTransition: no,
  maySuspendCommit: no, maySuspendCommitOnUpdate: no, maySuspendCommitInSyncRender: no,
  NotPendingTransition: null, HostTransitionContext: createContext(null),
  resetFormInstance: noop, requestPostPaintCallback: noop, trackSchedulerEvent: noop,
  resolveEventType: () => null, resolveEventTimeStamp: () => -1,
  preloadInstance: () => true, startSuspendingCommit: noop, suspendInstance: noop,
  waitForCommitToBeReady: () => null, preparePortalMount: noop,
  getInstanceFromNode: () => null, beforeActiveInstanceBlur: noop, afterActiveInstanceBlur: noop,
  prepareScopeUpdate: noop, getInstanceFromScope: () => null,
}

// React 19.2's host interface is newer than DefinitelyTyped's reconciler types.
const reconciler: any = ReactReconciler(config as any)
reconciler.injectIntoDevTools({ bundleType: 1, version: React.version, rendererPackageName: "@gpui-react/core" })

export function createRoot(transport: Transport, options?: Options): BridgeRoot {
  return new BridgeRoot(transport, options)
}
