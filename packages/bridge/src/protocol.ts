export type NativeProps = Record<string, unknown>

export type Operation =
  | { op: "create"; id: number; component: string; props: NativeProps; subscription: number | null }
  | { op: "props"; id: number; props: NativeProps }
  | { op: "listen"; id: number; subscription: number | null }
  | { op: "place"; parent: number | null; child: number; before: number | null }
  | { op: "remove"; id: number }
  | { op: "hidden"; id: number; hidden: boolean }
  | { op: "command" | "query"; id: number; request: number; value: unknown }

export interface Transaction { version: 1; sequence: number; operations: Operation[] }

export interface TransactionReply {
  sequence: number
  /** Retire callbacks only after preceding events have reached the receiver. */
  retired: number[]
  results: Array<{ request: number; value?: unknown; error?: string }>
}

export interface NativeEvent { target: number; subscription: number; payload: unknown }

/** A transport is bound to one root/session. It must preserve transaction and
 * event/ack ordering. send resolves after native application, not presentation.
 * The payload is encoded once here and decoded once at native admission. */
export interface Transport {
  send(transaction: string): Promise<TransactionReply>
  subscribe(receiver: (event: NativeEvent) => void, onError?: (error: Error) => void): () => void
  close(reason: string): void
}

export interface NativeRef<Command = unknown, Query = unknown, Reply = unknown> {
  readonly id: number
  command(value: Command): Promise<void>
  query(value: Query): Promise<Reply>
}
