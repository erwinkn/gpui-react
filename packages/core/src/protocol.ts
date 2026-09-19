export type NativeProps = Record<string, unknown>

export type Operation =
  /** `parent` present means the node is placed by this operation; null is the root list. */
  | { op: "create"; id: number; component: string; props: NativeProps; subscription: number | null; parent?: number | null; before?: number }
  | { op: "props"; id: number; component: string; props: NativeProps }
  | { op: "listen"; id: number; subscription: number | null }
  | { op: "place"; parent: number | null; child: number; before: number | null }
  | { op: "remove"; id: number }
  | { op: "hidden"; id: number; hidden: boolean }
  | { op: "command" | "query"; id: number; component: string; request: number; value: unknown }
  /** A style definition, referenced by later props as `style: id`. */
  | { op: "style"; id: number; style: NativeProps }
  | { op: "dropStyle"; id: number }

export interface Transaction { version: 1; sequence: number; operations: Operation[] }

export interface TransactionReply {
  sequence: number
  /** Retire callbacks only after preceding events have reached the receiver. */
  retired: number[]
  results: Array<{ request: number; value?: unknown; error?: string }>
}

export interface NativeEvent { target: number; subscription: number; payload: unknown }

/** How one prop travels on the binary wire. */
export type WireType = "bool" | "i32" | "u32" | "f32" | "f64" | "str" | "style" | "value"
export interface WireField { name: string; type: WireType; required: boolean }
/** One native component as the worker sees it. Index order is the wire's kind order.
 * `fields` is null when the props type has no positional schema and travels as a map. */
export interface KindSchema {
  name: string
  capabilities: { events: boolean; commands: boolean; queries: boolean; children: boolean; view: boolean }
  fields: WireField[] | null
}

/** A transport is bound to one root/session. It must preserve transaction and
 * event/ack ordering. send resolves after native application, not presentation.
 * The payload is encoded once here and decoded once at native admission. */
export interface Transport {
  /** JSON text or the binary wire. A binary payload is valid until `send` settles. */
  send(transaction: string | Uint8Array): Promise<TransactionReply>
  subscribe(receiver: (event: NativeEvent) => void, onError?: (error: Error) => void): () => void
  close(reason: string): void
}

export interface NativeRef<Command = unknown, Query = unknown, Reply = unknown> {
  readonly id: number
  command(value: Command): Promise<void>
  query(value: Query): Promise<Reply>
}

/** Native draw metadata. This is not an OS presentation acknowledgement. */
export interface FrameInfo {
  root: string
  frame: number
  commit: number
  viewportWidth: number
  viewportHeight: number
  scaleFactor: number
}
