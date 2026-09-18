import type { ApplicationRenderer } from "./application.js"
import {
  handleAutomationRequest,
  type AutomationBackend,
} from "./automation/client.js"
import {
  createSseDecoder,
  methods,
  PROTOCOL_VERSION,
  type MethodName,
  type ParamsOf,
  type ResultOf,
} from "./automation/protocol.js"

export function serveApplicationAutomation(
  renderer: ApplicationRenderer,
): void {
  const backend: AutomationBackend = {
    async close() {},
    async call<M extends MethodName>(
      method: M,
      raw: ParamsOf<M>,
    ): Promise<ResultOf<M>> {
      const params = methods[method].params.parse(raw) as any
      const query = (name: string, ...args: unknown[]) =>
        renderer.query<any>(name, ...args)
      let result: unknown
      switch (method) {
        case "initialize":
          result = {
            protocolVersion: PROTOCOL_VERSION,
            pid: process.pid,
            capabilities: ["input", "clock", "tree", "screenshot"],
            window: renderer.getWindowSize(),
          }
          break
        case "cancel":
          result = { ok: true }
          break
        case "getTree":
          result = { tree: JSON.parse(await query("getAutomationTree")) }
          break
        case "getBounds":
          result = { bounds: await query("getElementBounds", params.elementId) }
          break
        case "getScrollOffset":
          result = { offset: await query("getScrollOffset", params.elementId) }
          break
        case "getPaintedText":
          result = { text: await query("getPaintedText") }
          break
        case "getAllText":
          result = { text: await query("getAllText") }
          break
        case "getSelectedText":
          result = { text: await query("getSelectedText") }
          break
        case "screenshot":
          await query("getAutomationTree")
          await query("captureScreenshot", params.path)
          result = { path: params.path }
          break
        case "clockPause":
        case "clockResume":
          result = { nowMs: await query(method) }
          break
        case "clockSet":
          result = { nowMs: await query(method, params.nowMs) }
          break
        case "clockFastForward":
          result = { nowMs: await query(method, params.deltaMs) }
          break
        default: {
          const input: Record<string, [string, ...unknown[]]> = {
            click: [
              "simulateClick",
              params.x,
              params.y,
              params.button,
              params.modifiers,
            ],
            mouseDown: [
              "simulateMouseDown",
              params.x,
              params.y,
              params.button,
              params.modifiers,
            ],
            mouseUp: [
              "simulateMouseUp",
              params.x,
              params.y,
              params.button,
              params.modifiers,
            ],
            mouseMove: [
              "simulateMouseMove",
              params.x,
              params.y,
              params.pressedButton,
              params.modifiers,
            ],
            scrollWheel: [
              "simulateScrollWheel",
              params.x,
              params.y,
              params.deltaX,
              params.deltaY,
              params.modifiers,
            ],
            keystrokes: ["simulateKeystrokes", params.keys],
            keyDown: ["simulateKeyDown", params.key, params.isHeld],
            keyUp: ["simulateKeyUp", params.key],
            focus: ["focusElement", params.elementId],
            blur: ["blur"],
            scrollTo: ["scrollTo", params.elementId, params.x, params.y],
            clearSelection: ["clearSelection"],
          }
          if (
            params.elementId &&
            ["keystrokes", "keyDown", "keyUp"].includes(method)
          ) {
            await query("focusElement", params.elementId)
            await query("getAutomationTree")
          }
          const command = input[method]
          if (!command) throw Error(`Unsupported automation command: ${method}`)
          await query(...command)
          result = { ok: true }
        }
      }
      return methods[method].result.parse(result) as ResultOf<M>
    },
  }
  const decoder = createSseDecoder((message) => {
    if ("method" in message)
      void handleAutomationRequest(message, backend).then(write)
  })
  const utf8 = new TextDecoder()
  const write = renderer.connectAutomation((bytes) =>
    decoder.feed(utf8.decode(Uint8Array.from(bytes), { stream: true })),
  )
}
