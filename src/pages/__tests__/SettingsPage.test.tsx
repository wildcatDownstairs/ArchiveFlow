import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { beforeEach, describe, expect, it, vi } from "vitest"
import "@/i18n"
import SettingsPage from "@/pages/SettingsPage"
import { useAppStore } from "@/stores/appStore"
import * as api from "@/services/api"

vi.mock("@/services/api", () => ({
  getAppDataDir: vi.fn(),
  clearAllTasks: vi.fn(),
  clearAuditEvents: vi.fn(),
  detectHashcat: vi.fn(),
  getStats: vi.fn(),
  recordSettingChange: vi.fn(),
  setRecoverySchedulerLimit: vi.fn(),
}))

describe("SettingsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    window.localStorage.clear()

    useAppStore.setState((state) => ({
      ...state,
      locale: "zh",
      recoveryPreferences: {
        defaultCharsetFlags: {
          lowercase: true,
          uppercase: false,
          digits: true,
          special: false,
        },
        defaultMinLength: 1,
        defaultMaxLength: 4,
        defaultTaskPriority: 0,
        autoIncludeFilenamePatterns: false,
        autoClearDictionaryInput: false,
        resultRetentionPolicy: "plaintext",
        exportMaskPasswords: false,
        exportIncludeAuditEvents: true,
        maxConcurrentRecoveries: 1,
        hashcatPath: "",
      },
    }))

    vi.mocked(api.getAppDataDir).mockResolvedValue("C:/ArchiveFlow/data")
    vi.mocked(api.getStats).mockResolvedValue([3, 5])
    vi.mocked(api.recordSettingChange).mockResolvedValue()
    vi.mocked(api.setRecoverySchedulerLimit).mockResolvedValue({
      max_concurrent: 2,
      running_count: 0,
      queued_count: 0,
      paused_count: 0,
      tasks: [],
    })
    vi.mocked(api.clearAllTasks).mockResolvedValue(0)
    vi.mocked(api.clearAuditEvents).mockResolvedValue(0)
    vi.mocked(api.detectHashcat).mockResolvedValue({
      available: true,
      path: "C:/Tools/hashcat/hashcat.exe",
      version: "v7.0.0",
      devices: [{ id: 1, name: "NVIDIA GeForce RTX 4080", device_type: "GPU" }],
      error: null,
    })
  })

  it("切换导出脱敏默认值时会更新 store 并记录审计", async () => {
    const user = userEvent.setup()
    render(<SettingsPage />)

    const maskPasswordsCheckbox = await screen.findByLabelText("导出时默认脱敏密码")
    expect(maskPasswordsCheckbox).not.toBeChecked()

    await user.click(maskPasswordsCheckbox)

    await waitFor(() => {
      expect(useAppStore.getState().recoveryPreferences.exportMaskPasswords).toBe(true)
    })
    expect(api.recordSettingChange).toHaveBeenCalledWith(
      "recovery.exportMaskPasswords",
      "false",
      "true",
    )
  })

  it("检测 hashcat 后会展示版本和设备信息", async () => {
    const user = userEvent.setup()
    render(<SettingsPage />)

    await user.click(await screen.findByRole("button", { name: "检测 hashcat" }))

    expect(await screen.findByText("已检测到可用的 hashcat GPU 环境")).toBeInTheDocument()
    expect(screen.getByText(/v7\.0\.0/)).toBeInTheDocument()
    expect(screen.getByText(/RTX 4080/)).toBeInTheDocument()
  })
})
