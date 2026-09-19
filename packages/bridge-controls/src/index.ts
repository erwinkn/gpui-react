import { nativeComponent, type NativeRef, type FrameInfo } from "@gpui-react/core"
import { Children, createElement, type ReactNode, type Ref } from "react"

export type Length = number | "100%" | "auto"
export interface Style {
  width?: Length; height?: Length
  minWidth?: Length; minHeight?: Length; maxWidth?: Length; maxHeight?: Length
  direction?: "row" | "column" | "rowReverse" | "columnReverse"
  align?: "start" | "center" | "end" | "stretch"
  grow?: number; shrink?: number; gap?: number
  padding?: number; paddingX?: number; paddingY?: number
  background?: string; color?: string
  fontSize?: number; lineHeight?: number
  borderWidth?: number; borderColor?: string; radius?: number; opacity?: number
  hover?: Style; active?: Style; focus?: Style
}
export interface InputProps {
  /** Construction only. Native state owns the current value. */
  initialValue?: string
  /** Construction only. Remount to change editing mode. */
  initialMultiline?: boolean
  placeholder?: string
  label?: string
  readOnly?: boolean
  minRows?: number
  maxRows?: number
  submitOnEnter?: boolean
  /** Captured before native handling, except during IME composition. */
  captureKeys?: string[]
  style?: Style
  caretColor?: string
  selectionColor?: string
}
export interface Selection { start: number; end: number; reversed?: boolean }
export interface InputSnapshot {
  value: string
  /** Advances on text, selection, and composition changes. */
  revision: number
  /** UTF-16 offsets. */
  selection: Selection
  composing: boolean
  /** Last painted content bounds; the revision can be older than current state. */
  painted: { x: number; y: number; width: number; height: number; revision: number; frame: FrameInfo | null } | null
}
export type InputEvent =
  | { type: "change"; snapshot: InputSnapshot }
  | { type: "selection"; revision: number; selection: Selection; composing: boolean }
  | { type: "submit"; value: string; revision: number }
  | { type: "key"; key: string }
export type InputCommand =
  | { type: "focus" }
  | { type: "blur" }
  | { type: "replace"; value: string; expectedRevision: number }
  | { type: "select"; selection: Selection; expectedRevision: number }
export type InputRef = NativeRef<InputCommand, null, InputSnapshot>
export const Input = nativeComponent<InputProps, InputEvent, InputCommand, null, InputSnapshot>("input")

export interface Rect { x: number; y: number; width: number; height: number }
export type { FrameInfo } from "@gpui-react/core"
export interface Painted { bounds: Rect; revision: number; frame: FrameInfo | null }
export interface ContainerProps {
  style?: Style
  scroll?: "none" | "x" | "y" | "both"
  scrollGroup?: string
  blockMouse?: boolean
  focusable?: boolean
  label?: string
  /** Record painted bounds for `painted` in queries. Off by default. */
  measure?: boolean
}
export type ContainerEvent =
  | { type: "click"; x: number; y: number }
  | { type: "wheel"; x: number; y: number; dx: number; dy: number; offset: { x: number; y: number } }
export type ContainerCommand = { type: "focus" | "blur" } | { type: "scrollTo"; x: number; y: number }
export interface ContainerSnapshot {
  painted: Painted | null; revision: number; offset: { x: number; y: number }; childCount: number; focused: boolean
}
export type ContainerRef = NativeRef<ContainerCommand, null, ContainerSnapshot>
export const Container = nativeComponent<ContainerProps, ContainerEvent, ContainerCommand, null, ContainerSnapshot>("container")
export interface TextProps {
  text?: string; children?: ReactNode; style?: Style
  /** Stable logical identity inside a Document, including across row remounts. */
  textKey?: string
  selectable?: boolean
  searchable?: boolean
  /** Absolute match index before this logical text in a virtualized source. */
  matchIndexOffset?: number
  /** Record painted bounds for `painted` in queries. Off by default. */
  measure?: boolean
}
export interface TextSnapshot { text: string; revision: number; painted: Painted | null }
export type TextRef = NativeRef<never, null, TextSnapshot>
const NativeText = nativeComponent<Omit<TextProps, "children" | "text"> & { text: string }, never, never, null, TextSnapshot>("text")
export function Text({ text, children, ...props }: TextProps & { ref?: Ref<TextRef> }) {
  if (text !== undefined && children !== undefined) throw Error("Text accepts text or string/number children, not both")
  const value = text ?? Children.toArray(children).map(child => {
    if (typeof child !== "string" && typeof child !== "number") throw Error("Text children must be strings or numbers; use a native text component for styled runs")
    return String(child)
  }).join("")
  return createElement(NativeText, { ...props, text: value })
}

export interface DocumentQuery {
  query: string; regex: boolean; caseSensitive: boolean; wholeWord: boolean
}
export interface DocumentSearch extends Partial<Omit<DocumentQuery, "query">> {
  query: string
  activeIndex?: number | null; matchIndexOffset?: number
  color?: string; activeColor?: string
}
export interface DocumentProps { style?: Style; search?: DocumentSearch; selectionColor?: string }
export interface TextRange { key: string; start: number; end: number; rects: Rect[] }
export interface DocumentSnapshot {
  text: Array<{ key: string; text: string; bounds: Rect; selectable: boolean; searchable: boolean }>
  contentRevision: number
  selection: string | null; selectionRevision: number; paintedSelectionRevision: number
  ranges: TextRange[]
  highlights: Array<TextRange & { index: number; active: boolean }>
  matchCount: number; matchIndexOffset: number; query: DocumentQuery | null; frame: FrameInfo | null
}
export type DocumentEvent =
  | { type: "selection"; revision: number; hasSelection: boolean }
  | { type: "search"; query: DocumentQuery | null; frame: FrameInfo | null; contentRevision: number; count: number; indexOffset: number }
export type DocumentCommand =
  | { type: "clear" | "copy" | "selectAll" }
  | { type: "select"; start: { key: string; offset: number }; end: { key: string; offset: number }; expectedContentRevision: number }
export type DocumentRef = NativeRef<DocumentCommand, null, DocumentSnapshot>
export const Document = nativeComponent<DocumentProps, DocumentEvent, DocumentCommand, null, DocumentSnapshot>("document")
export interface ListProps {
  style?: Style
  itemCount?: number
  windowStart?: number
  estimatedItemHeight?: number
  overdraw?: number
  alignment?: "top" | "bottom"
  followTail?: boolean
}
export interface RowRange { start: number; end: number }
export type ListEvent =
  | { type: "needRows"; range: RowRange }
  | { type: "scroll"; range: RowRange; followingTail: boolean }
export type ListCommand =
  | { type: "scrollTo"; index: number; offset?: number }
  | { type: "end" }
  | { type: "remeasure"; start: number; end: number }
export interface ListSnapshot {
  itemCount: number; supplied: RowRange; anchor: { index: number; offset: number }; followingTail: boolean
  painted: Painted | null; revision: number; paintedRows: RowRange | null; maxScrollY: number
}
export type ListRef = NativeRef<ListCommand, null, ListSnapshot>
export const List = nativeComponent<ListProps, ListEvent, ListCommand, null, ListSnapshot>("list")
