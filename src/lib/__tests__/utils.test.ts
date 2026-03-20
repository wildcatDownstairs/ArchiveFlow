import { cn } from "../utils"

describe("cn", () => {
  it("merges multiple class strings", () => {
    expect(cn("px-2", "py-1")).toBe("px-2 py-1")
  })

  it("handles conditional (falsy) classes", () => {
    const shouldHide = false
    expect(cn("base", shouldHide && "hidden")).toBe("base")
  })

  it("resolves Tailwind conflicts by keeping the last value", () => {
    expect(cn("px-2", "px-4")).toBe("px-4")
  })

  it("handles undefined and null inputs", () => {
    expect(cn("base", undefined, null)).toBe("base")
  })

  it("returns empty string for no inputs", () => {
    expect(cn()).toBe("")
  })
})
