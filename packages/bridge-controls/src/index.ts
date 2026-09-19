import { nativeComponent, type NativeRef } from "@gpuix/bridge"

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
  painted: { x: number; y: number; width: number; height: number; revision: number } | null
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
