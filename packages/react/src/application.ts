import {
  Worker,
  isMainThread,
  parentPort,
  workerData,
} from "node:worker_threads"
import { NativeClient, NativeHost } from "@gpuix/native"
import type { EventPayload, WindowOptions } from "@gpuix/native"
import type { NativeRenderer } from "./types/host.js"
import { handleGpuixEvent } from "./reconciler/event-registry.js"
import { serveApplicationAutomation } from "./application-automation.js"

const MAX_PENDING = 256
// Reserve space for the transaction envelope and up to 256 separators.
const MAX_BYTES = 4 * 1024 * 1024 - 512

type Pending = {
  encoded: string
  bytes: number
  resolve: (value: any) => void
  reject: (error: Error) => void
}

/** Application-side renderer. Commands never wait synchronously for the UI. */
export class ApplicationRenderer implements NativeRenderer {
  private client: NativeClient
  private sequence = 0
  private pending = new Map<number, Pending>()
  private outbound: number[] = []
  private bytes = 0
  private failure: Error | null = null
  private lastCommit: Promise<unknown> = Promise.resolve()
  private sendScheduled = false
  private idleWaiters = new Set<{
    resolve: () => void
    reject: (error: Error) => void
  }>()
  private automationInput?: (bytes: number[]) => void
  private disposing = false

  constructor(session: number) {
    this.client = new NativeClient(session)
    if (this.client.protocolVersion !== 1)
      throw Error(
        "GPUiX application protocol mismatch; rebuild the native and React packages together",
      )
    void this.receive().catch((error) => this.fail(error))
  }

  private fail(reason: unknown, report = true): void {
    const error = reason instanceof Error ? reason : Error(String(reason))
    if (this.failure) return
    this.failure = error
    for (const pending of this.pending.values()) pending.reject(error)
    for (const waiter of this.idleWaiters) waiter.reject(error)
    this.idleWaiters.clear()
    this.pending.clear()
    this.outbound = []
    this.bytes = 0
    if (report) console.error("[gpuix] application transport failed:", error)
    this.client.close(error.message)
  }

  private sendReady(): void {
    if (!this.outbound.length) return
    const commands = this.outbound.map((id) =>
      JSON.parse(this.pending.get(id)!.encoded),
    )
    if (
      this.client.send(
        JSON.stringify({ id: 0, method: "transaction", args: [commands] }),
      )
    )
      this.outbound = []
  }

  private scheduleSend(): void {
    if (this.sendScheduled) return
    this.sendScheduled = true
    // React's layout effects finish before this microtask. Send their focus and
    // anchor commands with the scene mutations, before native code can paint.
    queueMicrotask(() => {
      this.sendScheduled = false
      try {
        this.sendReady()
      } catch (error) {
        this.fail(error)
      }
    })
  }

  private async receive(): Promise<void> {
    while (!this.client.closed) {
      for (const encoded of (await this.client.receive()) as string[]) {
        const message = JSON.parse(encoded)
        if (message.automationBytes) {
          this.automationInput?.(message.automationBytes)
        } else if (message.event) {
          try {
            handleGpuixEvent(message.event as EventPayload, this)
          } catch (error) {
            queueMicrotask(() => {
              throw error
            })
          }
        } else {
          const pending = this.pending.get(message.id)
          if (!pending) {
            if (message.error) throw Error(message.error)
            continue
          }
          this.pending.delete(message.id)
          this.bytes -= pending.bytes
          if (message.error) pending.reject(Error(message.error))
          else pending.resolve(message.value)
        }
      }
      if (this.pending.size === 0) {
        for (const waiter of this.idleWaiters) waiter.resolve()
        this.idleWaiters.clear()
      }
      this.scheduleSend()
    }
    this.fail(
      Error(this.client.closeReason || "Native application session closed"),
      false,
    )
  }

  /** Ordered live query or native command. Resolves after native execution. */
  query<T = unknown>(method: string, ...args: unknown[]): Promise<T> {
    if (this.failure) return Promise.reject(this.failure)
    const id = ++this.sequence
    const encoded = JSON.stringify({ id, method, args })
    const bytes = Buffer.byteLength(encoded)
    if (this.pending.size >= MAX_PENDING || this.bytes + bytes > MAX_BYTES) {
      return Promise.reject(
        Error(
          "Application transport is full; await whenIdle() before more commits",
        ),
      )
    }
    const result = new Promise<T>((resolve, reject) => {
      this.pending.set(id, { encoded, bytes, resolve, reject })
    })
    this.bytes += bytes
    this.outbound.push(id)
    this.scheduleSend()
    return result
  }

  applyBatch(json: string): number[] {
    if (this.disposing) return this.client.prepareBatch(json)
    if (this.failure) throw this.failure
    // Reserve capacity before changing the application-side commit model.
    if (
      this.pending.size >= MAX_PENDING ||
      this.bytes +
        Buffer.byteLength(
          JSON.stringify({
            id: this.sequence + 1,
            method: "applyBatch",
            args: [json],
          }),
        ) >
        MAX_BYTES
    ) {
      throw Error(
        "Application transport is full; await whenIdle() before more commits",
      )
    }
    const destroyed = this.client.prepareBatch(json)
    this.lastCommit = this.query("applyBatch", json)
    void this.lastCommit.catch((error) => this.fail(error))
    return destroyed
  }

  /** Wait for accepted commits and commands. Does not request a frame. */
  async whenIdle(): Promise<void> {
    if (this.failure) throw this.failure
    if (this.pending.size === 0) return
    await new Promise<void>((resolve, reject) =>
      this.idleWaiters.add({ resolve, reject }),
    )
  }

  private command(method: string, ...args: unknown[]): void {
    if (this.disposing) return
    void this.query(method, ...args).catch((error) => this.fail(error))
  }
  private snapshot<T>(method: string, id?: number): T {
    return JSON.parse(this.client.readSnapshot(method, id))
  }

  registerFonts(fonts: Uint8Array[]): void {
    this.client.registerFonts(fonts.map((font) => Buffer.from(font)))
  }
  measureTextWidths(
    family: string,
    size: number,
    weight: number,
    texts: string[],
  ): number[] {
    return this.client.measureTextWidths(family, size, weight, texts)
  }
  highlightCode(source: string, path?: string, language?: string) {
    return this.client.highlightCode(source, path, language)
  }
  getWindowSize() {
    return this.snapshot<{ width: number; height: number }>("getWindowSize")
  }
  getWindowInsets() {
    return this.snapshot<import("./types/host.js").NativeWindowInsets>(
      "getWindowInsets",
    )
  }
  getElementBounds(id: number) {
    return this.snapshot<import("./types/host.js").ElementBounds | null>(
      "getElementBounds",
      id,
    )
  }
  getFocusedElementId() {
    return this.snapshot<number | null>("getFocusedElementId")
  }
  getListScrollTop(id: number) {
    return this.snapshot<number[] | null>("getListScrollTop", id)
  }
  getScrollOffset(id: number) {
    return this.snapshot<number[] | null>("getScrollOffset", id)
  }
  getSelectedText() {
    return this.snapshot<string | null>("getSelectedText")
  }
  getSelectionInfo() {
    return this.snapshot<string>("getSelectionInfo")
  }
  getAutomationTree() {
    return this.snapshot<string>("getAutomationTree")
  }
  focusElement(id: number) {
    this.command("focusElement", id)
  }
  focusNext() {
    this.command("focusNext")
  }
  focusPrevious() {
    this.command("focusPrevious")
  }
  focusNextWithin(id: number) {
    this.command("focusNextWithin", id)
  }
  focusPreviousWithin(id: number) {
    this.command("focusPreviousWithin", id)
  }
  blur() {
    this.command("blur")
  }
  clearSelection() {
    this.command("clearSelection")
  }
  setWindowTitle(title: string) {
    this.command("setWindowTitle", title)
  }
  setWindowKeyEvents(down: boolean, up: boolean, id: number) {
    this.command("setWindowKeyEvents", down, up, id)
  }
  setWindowSelectionChange(enabled: boolean, id: number) {
    this.command("setWindowSelectionChange", enabled, id)
  }
  scrollTo(id: number, x: number, y: number) {
    this.command("scrollTo", id, x, y)
  }
  scrollToItem(id: number, index: number, offset = 0) {
    this.command("scrollToItem", id, index, offset)
  }
  shutdown() {
    return this.query("shutdown")
  }
  reportStartupError(error: unknown) {
    this.fail(error)
  }
  connectAutomation(
    input: (bytes: number[]) => void,
  ): (message: string) => void {
    this.automationInput = input
    this.client.enableStdio()
    return (message) => this.client.writeStdio(message)
  }
  dispose(): void {
    this.disposing = true
    const slot = Reflect.get(globalThis, "__gpuixRenderHost")
    slot?.root?.unmount()
  }
}

/** Call in an explicit worker entry before importing application code. */
export function attachApplication(): ApplicationRenderer {
  if (isMainThread || !workerData?.gpuixSession)
    throw Error("attachApplication requires a GPUiX application worker")
  const renderer = new ApplicationRenderer(workerData.gpuixSession)
  Reflect.set(globalThis, "__gpuixRenderHost", { renderer })
  if (process.stdin && !process.stdin.isTTY)
    serveApplicationAutomation(renderer)
  const startupError = (error: unknown) => {
    if (!Reflect.get(globalThis, "__gpuixRenderHost")?.root)
      renderer.reportStartupError(error)
  }
  process.on("uncaughtException", startupError)
  process.on("unhandledRejection", startupError)
  parentPort?.on("message", (message) => {
    if (!message?.gpuixShutdown) return
    try {
      renderer.dispose()
    } finally {
      process.exit(0)
    }
  })
  parentPort?.postMessage({ gpuixReady: true })
  return renderer
}

/** Run on the macOS main thread. The worker entry owns React and application imports. */
export async function runApplication(
  entry: string | URL,
  options: WindowOptions = {},
): Promise<void> {
  if (!isMainThread) throw Error("runApplication must run on the main thread")
  if (typeof NativeHost !== "function")
    throw Error(
      "Native-owned application startup currently requires macOS; use render() on this platform",
    )
  const host = new NativeHost({
    ...options,
    ...(process.env.GPUIX_BACKGROUND === "1" ? { focus: false } : {}),
  })
  let worker: Worker | undefined
  let failure: Error | undefined
  let exited: Promise<unknown> = Promise.resolve()
  try {
    worker = new Worker(entry, { workerData: { gpuixSession: host.id } })
    exited = new Promise((resolve) => worker!.once("exit", resolve))
    await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(
        () =>
          reject(
            Error(
              "Application worker did not call attachApplication() within 10 seconds",
            ),
          ),
        10000,
      )
      worker!.on("error", (error) => {
        clearTimeout(timeout)
        failure = error instanceof Error ? error : Error(String(error))
        reject(failure)
      })
      worker!.once("exit", (code) => {
        clearTimeout(timeout)
        reject(Error(`Application worker exited before startup: ${code}`))
      })
      worker!.on("message", (message) => {
        if (message?.gpuixReady) {
          clearTimeout(timeout)
          resolve()
        }
      })
    })
  } catch (error) {
    failure = error instanceof Error ? error : Error(String(error))
    host.requestShutdown(failure.message)
  }
  let reason: string
  try {
    reason = host.run()
  } finally {
    if (worker) {
      worker.postMessage({ gpuixShutdown: true })
      let timer: ReturnType<typeof setTimeout> | undefined
      const stopped = await Promise.race([
        exited.then(() => true),
        new Promise<false>((resolve) => {
          timer = setTimeout(() => resolve(false), 2000)
        }),
      ])
      clearTimeout(timer)
      if (!stopped) await worker.terminate()
    }
  }
  if (failure) throw failure
  if (
    reason !== "Application requested shutdown" &&
    reason !== "Native window closed" &&
    !reason.startsWith("Process signal ")
  )
    throw Error(reason)
}
