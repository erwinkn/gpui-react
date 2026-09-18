import React from "react"
import { describe, expect, it } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

describeNative("native horizontal scroll groups", () => {
  it("shares painted positions, detaches members, and removes unused groups", () => {
    const root = createTestRoot({ width: 400, height: 300 })
    const renderer = root.renderer
    const fixture = (second: string | undefined) => (
      <div style={{ display: "flex", flexDirection: "column", width: 300 }}>
        {["table", second, "other"].map((group, i) => (
          <div key={i} testId={`pane-${i}`} scrollGroup={group}
            style={{ width: 300, height: 60, overflowX: "scroll", flexShrink: 0 }}>
            <div testId={`content-${i}`} style={{ width: 800, minWidth: 800, height: 60, flexShrink: 0 }} />
          </div>
        ))}
      </div>
    )
    const id = (name: string) => renderer.findByTestId(name)!.id
    const offset = (i: number) => renderer.getScrollOffset(id(`pane-${i}`))![0]
    try {
      root.render(fixture("table"))
      renderer.nativeSimulateScrollWheel(80, 30, -80, 0)
      expect(offset(0)).toBe(-80)
      expect(offset(1)).toBe(-80)
      expect(offset(2)).toBe(0)
      expect(renderer.getElementBounds(id("content-0"))!.x).toBe(
        renderer.getElementBounds(id("content-1"))!.x
      )
      root.render(fixture(undefined))
      expect(offset(0)).toBe(-80)
      expect(offset(1)).toBe(0)
      renderer.scrollTo(id("pane-1"), -20, 0)
      expect(offset(0)).toBe(-80)
      expect(offset(1)).toBe(-20)
      root.render(fixture("other"))
      expect(offset(1)).toBe(0)
      renderer.scrollTo(id("pane-2"), -100, 0)
      expect(offset(1)).toBe(-100)
      expect(offset(0)).toBe(-80)
      root.render(<div />)
      root.render(fixture("table"))
      expect(offset(0)).toBe(0)
      expect(offset(1)).toBe(0)
    } finally {
      root.unmount()
    }
  })
})
