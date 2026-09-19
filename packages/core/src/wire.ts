// The two transaction encoders. The JSON encoder keeps operation objects and
// stringifies them at seal. The binary encoder writes bytes as React commits:
// props are read by name from the native schema and written positionally, so
// there is no filtered copy, no operation object, and no second pass.
import type { KindSchema, NativeProps, Operation, Transaction, WireField } from "./protocol.js"

export type Props = Record<string, any>
export type Encoded = string | Uint8Array

/** What every transaction encoder records. Ids are wire ids; `parent`
 * undefined means the node is not placed, null means the root list. */
export interface Encoder {
  readonly count: number
  create(id: number, kind: number, component: string, props: Props, subscription: number | null, parent: number | null | undefined, before: number | undefined): void
  /** Records a props operation when something native changed. */
  update(id: number, kind: number, component: string, oldProps: Props, props: Props): boolean
  listen(id: number, subscription: number | null): void
  place(parent: number | null, child: number, before: number | null): void
  remove(id: number): void
  hidden(id: number, hidden: boolean): void
  call(op: "command" | "query", id: number, kind: number, component: string, request: number, value: unknown): void
  style(id: number, style: object): void
  dropStyle(id: number): void
  /** The transaction so far, or null when nothing was recorded. Starts the next one. */
  seal(sequence: number): Encoded | null
  /** Request ids in the last sealed transaction. */
  requests(): number[]
  /** Forget everything recorded since the last seal. */
  reset(): void
  /** The sealed payload has been sent; its memory may be reused. */
  release(encoded: Encoded): void
}

export const excluded = new Set(["children", "ref", "key", "onEvent"])
export const DEV = typeof process !== "undefined" && process.env?.NODE_ENV !== "production"

/** Reject values JSON.stringify would silently drop or mangle. */
export function validate(value: unknown): void {
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

/** Structural equality for JSON props. A re-rendered inline style object with
 * unchanged values must not cross the bridge. */
export function sameValue(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false
  if (Array.isArray(a) !== Array.isArray(b)) return false
  const keys = Object.keys(a)
  if (keys.length !== Object.keys(b).length) return false
  return keys.every(key => Object.hasOwn(b, key) && sameValue((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key]))
}
/** Structural equality of two React props objects over their native fields,
 * without building filtered copies. */
export function sameProps(a: Props, b: Props): boolean {
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

// ---- JSON ------------------------------------------------------------------

export class JsonEncoder implements Encoder {
  private operations: Operation[] = []
  private sealedRequests: number[] = []
  constructor(private styleId: (style: object) => number) {}
  get count() { return this.operations.length }
  /** The props that cross the bridge: everything except React's own fields.
   * A style object is replaced by the id of its shared definition. */
  private nativeProps(props: Props): NativeProps {
    const result: NativeProps = {}
    for (const key in props) {
      const value = props[key]
      if (excluded.has(key) || value === undefined) continue
      validate(value)
      result[key] = key === "style" && typeof value === "object" && value !== null ? this.styleId(value) : value
    }
    return result
  }
  create(id: number, _kind: number, component: string, props: Props, subscription: number | null, parent: number | null | undefined, before: number | undefined) {
    const operation: Operation = { op: "create", id, component, props: this.nativeProps(props), subscription }
    if (parent !== undefined) {
      operation.parent = parent
      if (before !== undefined) operation.before = before
    }
    this.operations.push(operation)
  }
  update(id: number, _kind: number, component: string, oldProps: Props, props: Props) {
    if (sameProps(oldProps, props)) return false
    this.operations.push({ op: "props", id, component, props: this.nativeProps(props) })
    return true
  }
  listen(id: number, subscription: number | null) { this.operations.push({ op: "listen", id, subscription }) }
  place(parent: number | null, child: number, before: number | null) { this.operations.push({ op: "place", parent, child, before }) }
  remove(id: number) { this.operations.push({ op: "remove", id }) }
  hidden(id: number, hidden: boolean) { this.operations.push({ op: "hidden", id, hidden }) }
  call(op: "command" | "query", id: number, _kind: number, component: string, request: number, value: unknown) {
    this.operations.push({ op, id, component, request, value })
  }
  style(id: number, style: object) { this.operations.push({ op: "style", id, style: style as NativeProps }) }
  dropStyle(id: number) { this.operations.push({ op: "dropStyle", id }) }
  seal(sequence: number): string | null {
    if (!this.operations.length) return null
    const transaction: Transaction = { version: 1, sequence, operations: this.operations }
    this.operations = []
    this.sealedRequests = transaction.operations.flatMap(op => op.op === "command" || op.op === "query" ? [op.request] : [])
    // Values were validated when they were recorded, so this is plain
    // serialization; a replacer would run per value and cost more than the
    // serialization itself.
    return JSON.stringify(transaction)
  }
  requests() { return this.sealedRequests }
  reset() { this.operations = [] }
  release() {}
}

// ---- binary ----------------------------------------------------------------

export const NONE = 0xffff_ffff
/** An explicit null parent: the root list. Distinct from "not placed". */
export const NULL = 0xffff_fffe
const enum Tag { Create = 1, Props, Listen, Place, Remove, Hidden, Command, Query, Style, DropStyle }
const enum T { Null = 0, False, True, Int, F64, Str, Arr, Map }
const enum Ty { Bool = 0, I32, U32, F32, F64, Str, Style, Value }
const TYPES: Record<WireField["type"], Ty> = { bool: Ty.Bool, i32: Ty.I32, u32: Ty.U32, f32: Ty.F32, f64: Ty.F64, str: Ty.Str, style: Ty.Style, value: Ty.Value }
const HEADER = 9 // version u8, sequence u32, count u32
const MAX_FIELDS = 32

/** One component's encoding plan, derived once from its schema. */
interface Kind {
  name: string
  /** null: no positional schema; props travel as a tagged map. */
  names: string[] | null
  types: Ty[]
  required: number
  maskBytes: number
  /** Indices of style fields, resolved before the record is written. */
  styles: number[]
  known: Set<string>
}

function plan(schema: KindSchema[]): Kind[] {
  return schema.map(kind => {
    if (!kind.fields) return { name: kind.name, names: null, types: [], required: 0, maskBytes: 0, styles: [], known: new Set() }
    if (kind.fields.length > MAX_FIELDS) throw Error(`${kind.name} has more than ${MAX_FIELDS} props`)
    const types = kind.fields.map(f => TYPES[f.type])
    let required = 0
    kind.fields.forEach((f, i) => { if (f.required) required |= 1 << i })
    return {
      name: kind.name,
      names: kind.fields.map(f => f.name),
      types,
      required: required >>> 0,
      maskBytes: (kind.fields.length + 7) >> 3,
      styles: types.flatMap((t, i) => t === Ty.Style ? [i] : []),
      known: new Set(kind.fields.map(f => f.name)),
    }
  })
}

const B: typeof Buffer | null = typeof Buffer === "function" ? Buffer : null
/** Node's undocumented direct UTF-8 write: one call, returns the byte count, no result object. */
interface Utf8Write { utf8Write(s: string, offset: number): number }
const FAST_UTF8 = B !== null && typeof (B.prototype as unknown as Utf8Write).utf8Write === "function"
const textEncoder = new TextEncoder()

/** A growable little-endian byte writer. */
class Writer {
  bytes: Uint8Array
  view: DataView
  at = 0
  keys = new Map<string, number>()
  keyList: string[] = []
  constructor(bytes?: Uint8Array) {
    this.bytes = bytes ?? Writer.allocate(1 << 16)
    this.view = new DataView(this.bytes.buffer, this.bytes.byteOffset, this.bytes.byteLength)
  }
  static allocate(size: number): Uint8Array { return B ? B.allocUnsafe(size) : new Uint8Array(size) }
  /** A whole allocation as the writer's byte type, so string writes keep their fast path. */
  static wrap(buffer: ArrayBufferLike, offset: number): Uint8Array { return B ? B.from(buffer, offset) : new Uint8Array(buffer, offset) }
  reserve(extra: number) {
    if (this.at + extra <= this.bytes.length) return
    let size = this.bytes.length * 2
    while (size < this.at + extra) size *= 2
    const next = Writer.allocate(size)
    next.set(this.bytes.subarray(0, this.at))
    this.bytes = next
    this.view = new DataView(next.buffer, next.byteOffset, next.byteLength)
  }
  u8(v: number) { this.reserve(1); this.bytes[this.at++] = v }
  u16(v: number) { this.reserve(2); const b = this.bytes, at = this.at; b[at] = v; b[at + 1] = v >>> 8; this.at = at + 2 }
  u32(v: number) { this.reserve(4); const b = this.bytes, at = this.at; b[at] = v; b[at + 1] = v >>> 8; b[at + 2] = v >>> 16; b[at + 3] = v >>> 24; this.at = at + 4 }
  u32At(at: number, v: number) { const b = this.bytes; b[at] = v; b[at + 1] = v >>> 8; b[at + 2] = v >>> 16; b[at + 3] = v >>> 24 }
  f32(v: number) { this.reserve(4); this.view.setFloat32(this.at, v, true); this.at += 4 }
  f64(v: number) { this.reserve(8); this.view.setFloat64(this.at, v, true); this.at += 8 }
  str(s: string) {
    this.reserve(4 + s.length * 3)
    const at = this.at
    const written = FAST_UTF8 ? (this.bytes as unknown as Utf8Write).utf8Write(s, at + 4)
      : B ? (this.bytes as Buffer).write(s, at + 4)
      : textEncoder.encodeInto(s, this.bytes.subarray(at + 4)).written
    this.u32At(at, written)
    this.at = at + 4 + written
  }
  key(k: string) {
    let ix = this.keys.get(k)
    if (ix === undefined) { ix = this.keyList.length; this.keys.set(k, ix); this.keyList.push(k) }
    this.u16(ix)
  }
  /** The tagged tree for free-form values. */
  value(v: unknown) {
    switch (typeof v) {
      case "string": this.u8(T.Str); this.str(v); return
      case "boolean": this.u8(v ? T.True : T.False); return
      case "number":
        if (Number.isInteger(v) && v >= -0x8000_0000 && v <= 0x7fff_ffff) { this.u8(T.Int); this.u32(v >>> 0) }
        else if (Number.isFinite(v)) { this.u8(T.F64); this.f64(v) }
        else break
        return
      case "object":
        if (v === null) { this.u8(T.Null); return }
        if (Array.isArray(v)) { this.u8(T.Arr); this.u32(v.length); for (const item of v) this.value(item); return }
        this.map(v as Record<string, unknown>, null)
        return
    }
    throw Error("Native props and commands must be JSON data")
  }
  /** A tagged map. With `styleId`, a top-level `style` object is interned and React's fields are skipped. */
  map(o: Record<string, unknown>, styleId: ((style: object) => number) | null) {
    this.u8(T.Map)
    const countAt = this.at
    this.u16(0)
    let count = 0
    for (const k in o) {
      const v = o[k]
      if (v === undefined || (styleId && excluded.has(k))) continue
      this.key(k)
      if (styleId && k === "style" && typeof v === "object" && v !== null) { this.u8(T.Int); this.u32(styleId(v)) }
      else this.value(v)
      count++
    }
    const b = this.bytes; b[countAt] = count; b[countAt + 1] = count >>> 8
  }
  finishTrailer() {
    const tableAt = this.at
    this.u16(this.keyList.length)
    for (const k of this.keyList) this.str(k)
    this.u32(tableAt)
  }
  restart() { this.at = 0; this.keys.clear(); this.keyList.length = 0 }
}

export class BinaryEncoder implements Encoder {
  count = 0
  private kinds: Kind[]
  private w: Writer
  private pool: Uint8Array[] = []
  private pending: number[] = []
  private sealedRequests: number[] = []
  private warned = new Set<string>()
  /** Number checks: exact integers and finite floats. Off, integers are
   * rounded and clamped by the write and floats pass through. Measured within
   * noise at 5,000 numeric rows, so they stay on. */
  constructor(schema: KindSchema[], private styleId: (style: object) => number, private checks = true) {
    this.kinds = plan(schema)
    this.w = new Writer()
    this.begin()
  }
  private begin() {
    const w = this.w
    w.restart()
    w.u8(1); w.u32(0); w.u32(0)
    this.count = 0
    this.pending = []
  }
  /** Style fields are interned before the record starts so their
   * definitions precede the record on the wire. */
  private resolveStyles(kind: Kind, props: Props) {
    if (kind.names) {
      for (const i of kind.styles) { const v = props[kind.names[i]!]; if (typeof v === "object" && v !== null) this.styleId(v) }
    } else {
      const v = props.style
      if (typeof v === "object" && v !== null) this.styleId(v)
    }
  }
  /** Whether any schema field differs between two props objects. Scalars
   * compare by value; styles and free-form values structurally. */
  private changed(kind: Kind, old: Props, props: Props): boolean {
    const names = kind.names
    if (!names) return !sameProps(old, props)
    const types = kind.types
    for (let i = 0; i < names.length; i++) {
      const name = names[i]!, v = props[name], o = old[name]
      if (types[i]! >= Ty.Style ? !sameValue(o, v) : !Object.is(o, v)) return true
    }
    return false
  }
  /** Writes the props record. */
  private props(kind: Kind, props: Props): void {
    const w = this.w
    const names = kind.names
    if (!names) {
      w.map(props, this.styleId)
      return
    }
    const types = kind.types, checks = this.checks
    const maskAt = w.at
    w.reserve(kind.maskBytes)
    w.at += kind.maskBytes
    let mask = 0
    for (let i = 0; i < names.length; i++) {
      const name = names[i]!, v = props[name]
      if (v === undefined) continue
      mask |= 1 << i
      switch (types[i]!) {
        case Ty.Bool:
          if (typeof v !== "boolean") throw this.bad(kind, name, "a boolean")
          w.u8(v ? 1 : 0)
          break
        case Ty.I32:
          if (typeof v !== "number" || (checks && !(Number.isInteger(v) && v >= -0x8000_0000 && v <= 0x7fff_ffff))) throw this.bad(kind, name, "a 32-bit integer")
          w.u32(v >>> 0)
          break
        case Ty.U32:
          if (typeof v !== "number" || (checks && !(Number.isInteger(v) && v >= 0 && v <= 0xffff_ffff))) throw this.bad(kind, name, "an unsigned 32-bit integer")
          w.u32(v >>> 0)
          break
        case Ty.F32:
          if (typeof v !== "number" || (checks && !Number.isFinite(v))) throw this.bad(kind, name, "a finite number")
          w.f32(v)
          break
        case Ty.F64:
          if (typeof v !== "number" || (checks && !Number.isFinite(v))) throw this.bad(kind, name, "a finite number")
          w.f64(v)
          break
        case Ty.Str:
          if (typeof v !== "string") throw this.bad(kind, name, "a string")
          w.str(v)
          break
        case Ty.Style:
          if (typeof v === "object" && v !== null) w.u32(this.styleId(v))
          else if (typeof v === "number" && Number.isInteger(v) && v >= 0) w.u32(v)
          else throw this.bad(kind, name, "a style object")
          break
        default:
          w.value(v)
      }
    }
    if ((mask & kind.required) !== kind.required) {
      const missing = names.filter((_, i) => (kind.required & ~mask) & (1 << i))
      throw Error(`${kind.name} is missing required props: ${missing.join(", ")}`)
    }
    const b = w.bytes
    for (let i = 0; i < kind.maskBytes; i++) b[maskAt + i] = mask >>> (8 * i)
    if (DEV) this.unknown(kind, props)
  }
  private bad(kind: Kind, name: string, expected: string) { return Error(`${kind.name}.${name} must be ${expected}`) }
  private unknown(kind: Kind, props: Props) {
    for (const key in props) {
      if (kind.known.has(key) || excluded.has(key) || props[key] === undefined) continue
      const id = `${kind.name}.${key}`
      if (this.warned.has(id)) continue
      this.warned.add(id)
      console.warn(`gpui-react: ${kind.name} has no prop "${key}"; it was not sent to native`)
    }
  }
  create(id: number, kindIndex: number, _component: string, props: Props, subscription: number | null, parent: number | null | undefined, before: number | undefined) {
    const kind = this.kinds[kindIndex]!
    this.resolveStyles(kind, props)
    const w = this.w, at = w.at
    try {
      w.u8(Tag.Create); w.u32(id); w.u16(kindIndex); w.u32(subscription ?? NONE)
      w.u32(parent === undefined ? NONE : parent === null ? NULL : parent); w.u32(before ?? NONE)
      this.props(kind, props)
    } catch (error) { w.at = at; throw error }
    this.count++
  }
  update(id: number, kindIndex: number, _component: string, oldProps: Props, props: Props) {
    const kind = this.kinds[kindIndex]!
    if (!this.changed(kind, oldProps, props)) return false
    this.resolveStyles(kind, props)
    const w = this.w, at = w.at
    try {
      w.u8(Tag.Props); w.u32(id); w.u16(kindIndex)
      this.props(kind, props)
    } catch (error) { w.at = at; throw error }
    this.count++
    return true
  }
  listen(id: number, subscription: number | null) { const w = this.w; w.u8(Tag.Listen); w.u32(id); w.u32(subscription ?? NONE); this.count++ }
  place(parent: number | null, child: number, before: number | null) { const w = this.w; w.u8(Tag.Place); w.u32(parent ?? NONE); w.u32(child); w.u32(before ?? NONE); this.count++ }
  remove(id: number) { const w = this.w; w.u8(Tag.Remove); w.u32(id); this.count++ }
  hidden(id: number, hidden: boolean) { const w = this.w; w.u8(Tag.Hidden); w.u32(id); w.u8(hidden ? 1 : 0); this.count++ }
  call(op: "command" | "query", id: number, kindIndex: number, _component: string, request: number, value: unknown) {
    const w = this.w, at = w.at
    try {
      w.u8(op === "command" ? Tag.Command : Tag.Query); w.u32(id); w.u16(kindIndex); w.u32(request); w.value(value)
    } catch (error) { w.at = at; throw error }
    this.pending.push(request)
    this.count++
  }
  style(id: number, style: object) { const w = this.w; w.u8(Tag.Style); w.u32(id); w.map(style as Record<string, unknown>, null); this.count++ }
  dropStyle(id: number) { const w = this.w; w.u8(Tag.DropStyle); w.u32(id); this.count++ }
  seal(sequence: number): Uint8Array | null {
    if (!this.count) return null
    const w = this.w
    w.u32At(1, sequence)
    w.u32At(5, this.count)
    w.finishTrailer()
    const sealed = w.bytes.subarray(0, w.at)
    this.sealedRequests = this.pending
    // The sealed view stays valid until the transport releases it; the next
    // transaction writes into another buffer.
    this.w = new Writer(this.pool.pop() ?? Writer.allocate(w.bytes.length))
    this.begin()
    return sealed
  }
  requests() { return this.sealedRequests }
  reset() { this.begin() }
  release(encoded: Encoded) {
    if (typeof encoded === "string") return
    // Recycle the whole allocation, not the sealed slice.
    this.pool.push(Writer.wrap(encoded.buffer, encoded.byteOffset))
  }
}

// ---- decoding, for tests and tools --------------------------------------------

/** Decodes a binary transaction into the same operation objects the JSON wire
 * carries, with a positional props object per schema. Not used at runtime. */
export function decodeWire(bytes: Uint8Array, schema: KindSchema[]): Transaction {
  const kinds = plan(schema)
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const decoder = new TextDecoder()
  let at = 0
  const u8 = () => bytes[at++]!
  const u16 = () => { const v = view.getUint16(at, true); at += 2; return v }
  const u32 = () => { const v = view.getUint32(at, true); at += 4; return v }
  const i32 = () => { const v = view.getInt32(at, true); at += 4; return v }
  const f32 = () => { const v = view.getFloat32(at, true); at += 4; return v }
  const f64 = () => { const v = view.getFloat64(at, true); at += 8; return v }
  const str = () => { const n = u32(); const s = decoder.decode(bytes.subarray(at, at + n)); at += n; return s }
  const id = () => { const v = u32(); return v === NONE ? null : v }
  const tableAt = view.getUint32(bytes.byteLength - 4, true)
  at = tableAt
  const keys: string[] = []
  for (let n = u16(); n > 0; n--) keys.push(str())
  at = 0
  const value = (): unknown => {
    switch (u8()) {
      case T.Null: return null
      case T.False: return false
      case T.True: return true
      case T.Int: return i32()
      case T.F64: return f64()
      case T.Str: return str()
      case T.Arr: { const n = u32(); const a = []; for (let i = 0; i < n; i++) a.push(value()); return a }
      case T.Map: return mapBody()
    }
    throw Error("bad wire value tag")
  }
  const mapBody = () => { const n = u16(); const o: Record<string, unknown> = {}; for (let i = 0; i < n; i++) { const k = keys[u16()]!; o[k] = value() } return o }
  const map = () => { if (u8() !== T.Map) throw Error("expected a map"); return mapBody() }
  const props = (kind: Kind): NativeProps => {
    if (!kind.names) return map()
    let mask = 0
    for (let i = 0; i < kind.maskBytes; i++) mask |= u8() << (8 * i)
    const o: NativeProps = {}
    for (let i = 0; i < kind.names.length; i++) {
      if (!(mask & (1 << i))) continue
      const name = kind.names[i]!
      switch (kind.types[i]!) {
        case Ty.Bool: o[name] = u8() === 1; break
        case Ty.I32: o[name] = i32(); break
        case Ty.U32: case Ty.Style: o[name] = u32(); break
        case Ty.F32: o[name] = f32(); break
        case Ty.F64: o[name] = f64(); break
        case Ty.Str: o[name] = str(); break
        default: o[name] = value()
      }
    }
    return o
  }
  const version = u8() as 1, sequence = u32(), count = u32()
  const operations: Operation[] = []
  for (let i = 0; i < count; i++) {
    switch (u8()) {
      case Tag.Create: {
        const id_ = u32(), kind = kinds[u16()]!, subscription = id(), parent = u32(), before = id()
        const op: Operation = { op: "create", id: id_, component: kind.name, props: props(kind), subscription }
        if (parent !== NONE) { op.parent = parent === NULL ? null : parent; if (before !== null) op.before = before }
        operations.push(op); break
      }
      case Tag.Props: { const id_ = u32(), kind = kinds[u16()]!; operations.push({ op: "props", id: id_, component: kind.name, props: props(kind) }); break }
      case Tag.Listen: operations.push({ op: "listen", id: u32(), subscription: id() }); break
      case Tag.Place: operations.push({ op: "place", parent: id(), child: u32(), before: id() }); break
      case Tag.Remove: operations.push({ op: "remove", id: u32() }); break
      case Tag.Hidden: operations.push({ op: "hidden", id: u32(), hidden: u8() === 1 }); break
      case Tag.Command: case Tag.Query: {
        const command = bytes[at - 1] === Tag.Command
        const id_ = u32(), kind = kinds[u16()]!, request = u32()
        operations.push({ op: command ? "command" : "query", id: id_, component: kind.name, request, value: value() }); break
      }
      case Tag.Style: operations.push({ op: "style", id: u32(), style: map() }); break
      case Tag.DropStyle: operations.push({ op: "dropStyle", id: u32() }); break
      default: throw Error("bad wire operation tag")
    }
  }
  if (at !== tableAt) throw Error("wire operations do not end at the key table")
  return { version, sequence, operations }
}
