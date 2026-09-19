import { nativeComponent, type NativeRef, type FrameInfo } from "@gpui-react/core"
import type { Style, Rect } from "@gpui-react/controls"

export type Color = [number, number, number, number]
export interface TextureProps {
  style?: Style
  initialColor?: Color
  radius?: number
  label?: string
  reducedMotion?: boolean
}
export type TextureCommand = { type: "transition"; to: Color; durationMs: number } | { type: "cancel" }
export type TextureEvent = { type: "click"; generation: number } | { type: "error"; message: string }
export interface TextureSnapshot {
  color: Color
  animating: boolean
  painted: { bounds: Rect; size: [number, number]; color: Color; generation: number; frame: FrameInfo | null } | null
  error: string | null
}
export type TextureRef = NativeRef<TextureCommand, null, TextureSnapshot>
export const Texture = nativeComponent<TextureProps, TextureEvent, TextureCommand, null, TextureSnapshot>("example-texture")
