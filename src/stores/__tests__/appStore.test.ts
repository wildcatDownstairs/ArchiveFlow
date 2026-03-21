/**
 * @fileoverview 文件功能：实现 appStore.test 状态管理
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies 无
 */

import { useAppStore } from "../appStore"

describe("appStore", () => {
  beforeEach(() => {
    // Reset store to initial state before each test
    useAppStore.setState({
      locale: "zh",
      sidebarCollapsed: false,
    })
  })

  it("has initial locale of 'zh'", () => {
    expect(useAppStore.getState().locale).toBe("zh")
  })

  it("has initial sidebarCollapsed of false", () => {
    expect(useAppStore.getState().sidebarCollapsed).toBe(false)
  })

  it("setLocale changes locale to 'en'", () => {
    useAppStore.getState().setLocale("en")
    expect(useAppStore.getState().locale).toBe("en")
  })

  it("toggleSidebar sets sidebarCollapsed to true", () => {
    useAppStore.getState().toggleSidebar()
    expect(useAppStore.getState().sidebarCollapsed).toBe(true)
  })

  it("toggleSidebar twice returns sidebarCollapsed to false", () => {
    useAppStore.getState().toggleSidebar()
    useAppStore.getState().toggleSidebar()
    expect(useAppStore.getState().sidebarCollapsed).toBe(false)
  })
})
