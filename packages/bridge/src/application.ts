import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads"
import { createRoot, type BridgeRoot } from "./index.js"
import type { NativeEvent, TransactionReply, Transport } from "./protocol.js"

export interface WindowOptions { title?: string; width?: number; height?: number; show?: boolean }
export interface Client {
  /** The component kind table as JSON. */
  schema(): string
  send(transaction: string | Uint8Array): void
  receive(): Promise<string[]>
  close(reason: string): void
}
export interface Bindings {
  bridgeRuntimeVersion(): number
  NativeHost: new (options?: WindowOptions) => { readonly id: number; run(): string; close(reason: string): void }
  NativeClient: new (id: number) => Client
}

function verify(bindings: Bindings): void {
  if (bindings.bridgeRuntimeVersion() !== 2) throw Error("Native bridge protocol mismatch")
}

export class NativeTransport implements Transport {
  private receiver?: (event: NativeEvent) => void
  private onError?: (error: Error) => void
  private pending?: { resolve: (reply: TransactionReply) => void; reject: (error: Error) => void }
  private closed: Error | null = null
  constructor(private client: Client) {}

  subscribe(receiver: (event: NativeEvent) => void, onError?: (error: Error) => void): () => void {
    if (this.receiver) throw Error("Native transport already has a receiver")
    this.receiver = receiver
    this.onError = onError
    void this.receive()
    return () => { this.receiver = undefined; this.onError = undefined }
  }

  send(transaction: string | Uint8Array): Promise<TransactionReply> {
    if (this.closed) return Promise.reject(this.closed)
    if (this.pending) return Promise.reject(Error("Native transport already has an in-flight transaction"))
    return new Promise((resolve, reject) => {
      this.pending = { resolve, reject }
      try { this.client.send(transaction) }
      catch (error) { this.pending = undefined; reject(error) }
    })
  }

  private async receive(): Promise<void> {
    try {
      while (!this.closed) {
        for (const encoded of await this.client.receive()) {
          const message = JSON.parse(encoded)
          if (message.event) this.receiver?.(message.event)
          else if (message.reply && this.pending) {
            const pending = this.pending
            this.pending = undefined
            pending.resolve(message.reply)
          } else throw Error("Unexpected native bridge message")
        }
      }
    } catch (reason) {
      if (this.closed) return
      const error = reason instanceof Error ? reason : Error(String(reason))
      const report = this.onError
      this.close(error.message)
      report?.(error)
    }
  }

  close(reason: string): void {
    if (this.closed) return
    this.closed = Error(reason)
    this.pending?.reject(this.closed)
    this.pending = undefined
    this.client.close(reason)
  }
}

/** Call from the explicit worker entry, with the same runtime as the launcher. */
export function attachApplication(bindings: Bindings): BridgeRoot {
  verify(bindings)
  if (isMainThread || !workerData?.gpuiReactSession) throw Error("attachApplication requires an application worker")
  const client = new bindings.NativeClient(workerData.gpuiReactSession)
  const root = createRoot(new NativeTransport(client), { schema: JSON.parse(client.schema()), wire: "binary" })
  const failed = (error: unknown) => client.close(error instanceof Error ? error.message : String(error))
  process.on("uncaughtExceptionMonitor", failed)
  process.on("unhandledRejection", failed)
  parentPort?.on("message", message => {
    if (message?.gpuiReactShutdown) {
      try { root.dispose("Native host shut down") } finally { process.exit(0) }
    }
  })
  parentPort?.postMessage({ gpuiReactReady: true })
  return root
}

/** The launcher enters the native loop. Application code lives in the worker.
 * Windows are shown inactive; this API never activates the application. */
export async function runApplication(bindings: Bindings, entry: string | URL, options: WindowOptions = {}): Promise<void> {
  verify(bindings)
  if (!isMainThread) throw Error("runApplication requires the main thread")
  if (typeof bindings.NativeHost !== "function") throw Error("This runtime does not provide a native application host")
  const host = new bindings.NativeHost(options)
  let worker: Worker | undefined
  let stopped: Promise<void> = Promise.resolve()
  let failure: Error | undefined
  try {
    worker = new Worker(entry, { workerData: { gpuiReactSession: host.id } })
    stopped = new Promise(resolve => worker!.once("exit", () => resolve()))
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => reject(Error("Application worker did not attach within 10 seconds")), 10_000)
      worker!.once("error", reason => { clearTimeout(timer); const error = reason instanceof Error ? reason : Error(String(reason)); failure = error; host.close(error.message); reject(error) })
      worker!.once("exit", code => { clearTimeout(timer); reject(Error(`Worker exited before attaching: ${code}`)) })
      worker!.on("message", message => { if (message?.gpuiReactReady) { clearTimeout(timer); resolve() } })
    })
  } catch (error) {
    failure = error instanceof Error ? error : Error(String(error))
    host.close(failure.message)
  }
  let reason: string
  try { reason = host.run() }
  finally {
    if (worker) {
      worker.postMessage({ gpuiReactShutdown: true })
      let timer: ReturnType<typeof setTimeout> | undefined
      const clean = await Promise.race([stopped.then(() => true), new Promise<false>(resolve => { timer = setTimeout(() => resolve(false), 2000) })])
      clearTimeout(timer)
      if (!clean) await worker.terminate()
    }
  }
  if (failure) throw failure
  if (reason !== "React root unmounted" && reason !== "Native window closed" && reason !== "Process signal 2" && reason !== "Process signal 15") throw Error(reason)
}
