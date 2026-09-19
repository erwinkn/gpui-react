import React, { createContext, createElement, type ReactNode, type Ref } from "react"
import ReactReconciler from "react-reconciler"
import { ConcurrentRoot, DefaultEventPriority } from "react-reconciler/constants.js"
import type { NativeEvent, NativeProps, NativeRef, Operation, Transaction, TransactionReply, Transport } from "./protocol.js"
export type { FrameInfo, NativeEvent, NativeRef, Operation, Transaction, TransactionReply, Transport } from "./protocol.js"

export function nativeComponent<Props extends object, Event = never, Command = unknown, Query = unknown, Reply = unknown>(name: string) {
  if (!/^[a-z0-9-]+$/.test(name)) throw Error("Invalid native component name")
  return function NativeComponent(props: Props & { children?: ReactNode; onEvent?: (event: Event) => void; ref?: Ref<NativeRef<Command, Query, Reply>> }): React.ReactElement {
    return createElement(name, props)
  }
}

interface Options { maxPending?: number; maxBytes?: number; onError?: (error: Error) => void }
type Props = Record<string, any>
type PendingCall = { resolve: (value: any) => void; reject: (error: Error) => void }
interface Host {
  id: number
  component: string
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
  private operations: Operation[] = []
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

  constructor(private transport: Transport, options: Options = {}) {
    if (attached.has(transport)) throw Error("Transport already has a React root")
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
    this.operations = []
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
    this.operations = []
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

  record(operation: Operation): void {
    if (this.disposed) return
    this.check()
    this.operations.push(operation)
    if (!this.scheduled) {
      this.scheduled = true
      // Seal after synchronous layout effects and their native commands.
      queueMicrotask(() => {
        this.scheduled = false
        try { this.seal() } catch (error) { this.fail(error) }
      })
    }
  }

  private seal(): void {
    if (!this.operations.length || this.failure) return
    const transaction: Transaction = { version: 1, sequence: ++this.sequence, operations: this.operations }
    this.operations = []
    const removed = this.removed
    this.removed = []
    // Values were validated when they were recorded, so this is plain
    // serialization; a replacer would run per value and cost more than the
    // serialization itself.
    const encoded = JSON.stringify(transaction)
    const bytes = typeof Buffer === "function" ? Buffer.byteLength(encoded) : new TextEncoder().encode(encoded).byteLength
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
        if (reply.sequence !== transaction.sequence) throw Error("Native transaction acknowledgement is out of order")
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
        for (const operation of transaction.operations) {
          if ((operation.op === "command" || operation.op === "query") && this.calls.has(operation.request)) throw Error("Missing native request result")
        }
      } catch (error) { this.fail(error) }
      finally { this.pending--; this.bytes -= bytes }
    })
  }

  host(component: string, props: Props): Host {
    return { id: 0, component, props, root: this, initial: [], mounted: false, subscription: null, public: null }
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
        this.record({ op, id: host.id, component: host.component, request, value })
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
      this.record({ op: "dropStyle", id })
    }
    const id = this.freeStyles.pop() ?? this.nextStyle++
    this.styleIds.set(text, id)
    this.liveStyles.add(id)
    this.styleByObject.set(style, id)
    this.record({ op: "style", id, style: style as NativeProps })
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

const excluded = new Set(["children", "ref", "key", "onEvent"])
const MAX_ID = 0xffff_fffd
const MAX_STYLES = 4096

/** Reject values JSON.stringify would silently drop or mangle. */
function validate(value: unknown): void {
  switch (typeof value) {
    case "string": case "boolean": return
    case "number": if (Number.isFinite(value)) return; break
    case "object":
      if (value === null) return
      if (Array.isArray(value)) { for (const item of value) validate(item); return }
      for (const key in value) validate((value as Record<string, unknown>)[key])
      return
    case "undefined": return
  }
  throw Error("Native props and commands must be JSON data")
}

/** The props that cross the bridge: everything except React's own fields.
 * A style object is replaced by the id of its shared definition. */
function nativeProps(root: BridgeRoot, props: Props): NativeProps {
  const result: NativeProps = {}
  for (const key in props) {
    const value = props[key]
    if (excluded.has(key) || value === undefined) continue
    validate(value)
    result[key] = key === "style" && typeof value === "object" && value !== null ? root.styleId(value) : value
  }
  return result
}

/** Structural equality for JSON props. A re-rendered inline style object with
 * unchanged values must not cross the bridge. */
function sameValue(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false
  if (Array.isArray(a) !== Array.isArray(b)) return false
  const keys = Object.keys(a)
  if (keys.length !== Object.keys(b).length) return false
  return keys.every(key => Object.hasOwn(b, key) && sameValue((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key]))
}
/** Structural equality of two React props objects over their native fields,
 * without building filtered copies. */
function sameProps(a: Props, b: Props): boolean {
  let count = 0
  for (const key in a) {
    if (excluded.has(key) || a[key] === undefined) continue
    count++
    if (!Object.hasOwn(b, key) || !sameValue(a[key], b[key])) return false
  }
  for (const key in b) {
    if (excluded.has(key) || b[key] === undefined) continue
    count--
  }
  return count === 0
}

/** Create a node, placing it in the same operation when its parent is known. */
function materialize(host: Host, parent: Host | BridgeRoot | null = null, before: Host | null = null): void {
  if (host.mounted) return
  host.id = host.root.allocateId()
  host.mounted = true
  host.subscription = host.root.listen(host)
  const operation: Operation = { op: "create", id: host.id, component: host.component, props: nativeProps(host.root, host.props), subscription: host.subscription }
  if (parent) {
    operation.parent = parent instanceof BridgeRoot ? null : parent.id
    if (before) operation.before = before.id
  }
  host.root.record(operation)
  for (const child of host.initial) place(host, child)
  host.initial = [] // The worker retains no mounted child topology.
}

function place(parent: Host | BridgeRoot, child: Host, before: Host | null = null): void {
  if (!(parent instanceof BridgeRoot)) materialize(parent)
  if (!child.mounted) {
    materialize(child, parent, before)
    return
  }
  child.root.record({ op: "place", parent: parent instanceof BridgeRoot ? null : parent.id, child: child.id, before: before?.id ?? null })
}

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
  removeChild: (_: Host, child: Host) => child.root.record({ op: "remove", id: child.id }),
  removeChildFromContainer: (_: BridgeRoot, child: Host) => child.root.record({ op: "remove", id: child.id }),
  commitUpdate: (host: Host, _: string, oldProps: Props, props: Props) => {
    host.props = props
    if (!sameProps(oldProps, props)) host.root.record({ op: "props", id: host.id, component: host.component, props: nativeProps(host.root, props) })
    if (oldProps.onEvent !== props.onEvent) {
      host.subscription = host.root.listen(host)
      host.root.record({ op: "listen", id: host.id, subscription: host.subscription })
    }
  },
  commitTextUpdate: (host: Host, _: string, text: string) => {
    host.props = { text }
    host.root.record({ op: "props", id: host.id, component: host.component, props: { text } })
  },
  hideInstance: (host: Host) => host.root.record({ op: "hidden", id: host.id, hidden: true }),
  hideTextInstance: (host: Host) => host.root.record({ op: "hidden", id: host.id, hidden: true }),
  unhideInstance: (host: Host) => host.root.record({ op: "hidden", id: host.id, hidden: false }),
  unhideTextInstance: (host: Host) => host.root.record({ op: "hidden", id: host.id, hidden: false }),
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
