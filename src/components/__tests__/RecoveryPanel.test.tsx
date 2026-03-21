import { act, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import "@/i18n"
import RecoveryPanel from "@/components/RecoveryPanel"
import * as api from "@/services/api"
import { useAppStore } from "@/stores/appStore"
import type { RecoveryProgress, RecoverySchedulerSnapshot, Task } from "@/types"

vi.mock("@/services/api", () => ({
  onRecoveryProgress: vi.fn(),
  getRecoveryCheckpoint: vi.fn(),
  getScheduledRecovery: vi.fn(),
  getRecoverySchedulerSnapshot: vi.fn(),
  getTaskAuditEvents: vi.fn(),
  setRecoverySchedulerLimit: vi.fn(),
  startRecovery: vi.fn(),
  cancelRecovery: vi.fn(),
  pauseRecovery: vi.fn(),
  resumeRecovery: vi.fn(),
}))

const EMPTY_SCHEDULER_SNAPSHOT: RecoverySchedulerSnapshot = {
  max_concurrent: 1,
  running_count: 0,
  queued_count: 0,
  paused_count: 0,
  tasks: [],
}

const RUNNING_TASK: Task = {
  id: "task-1",
  file_path: "C:/tmp/archive.zip",
  file_name: "archive.zip",
  file_size: 1024,
  archive_type: "zip",
  status: "processing",
  created_at: "2026-03-21T00:00:00Z",
  updated_at: "2026-03-21T00:00:00Z",
  error_message: null,
  found_password: null,
  archive_info: null,
}

describe("RecoveryPanel", () => {
  let progressListener: ((progress: RecoveryProgress) => void) | null = null

  beforeEach(() => {
    vi.clearAllMocks()
    progressListener = null

    window.localStorage.clear()
    useAppStore.setState((state) => ({
      ...state,
      recoveryPreferences: {
        defaultCharsetFlags: {
          lowercase: true,
          uppercase: false,
          digits: true,
          special: false,
        },
        defaultMinLength: 1,
        defaultMaxLength: 4,
        autoIncludeFilenamePatterns: false,
        autoClearDictionaryInput: false,
        resultRetentionPolicy: "plaintext",
        maxConcurrentRecoveries: 1,
      },
    }))

    vi.mocked(api.getRecoveryCheckpoint).mockResolvedValue(null)
    vi.mocked(api.getScheduledRecovery).mockResolvedValue(null)
    vi.mocked(api.getRecoverySchedulerSnapshot).mockResolvedValue(
      EMPTY_SCHEDULER_SNAPSHOT,
    )
    vi.mocked(api.getTaskAuditEvents).mockResolvedValue([])
    vi.mocked(api.setRecoverySchedulerLimit).mockResolvedValue(
      EMPTY_SCHEDULER_SNAPSHOT,
    )
    vi.mocked(api.onRecoveryProgress).mockImplementation(async (callback) => {
      progressListener = callback
      return () => {}
    })
  })

  it("收到找到密码事件后立即隐藏运行中的进度和取消按钮", async () => {
    render(<RecoveryPanel task={RUNNING_TASK} />)

    await waitFor(() => {
      expect(api.onRecoveryProgress).toHaveBeenCalledTimes(1)
    })

    expect(
      screen.getByRole("button", { name: "取消恢复" }),
    ).toBeInTheDocument()

    act(() => {
      progressListener?.({
        task_id: RUNNING_TASK.id,
        tried: 43_030_871,
        total: 100_000_000,
        speed: 2_132_633,
        status: "found",
        found_password: "20260320",
        elapsed_seconds: 20.2,
        worker_count: 8,
        last_checkpoint_at: "2026-03-21T00:00:00Z",
      })
    })

    await waitFor(() => {
      expect(screen.getByText("密码已找到")).toBeInTheDocument()
    })

    expect(screen.queryByText("恢复进行中...")).not.toBeInTheDocument()
    expect(
      screen.queryByRole("button", { name: "取消恢复" }),
    ).not.toBeInTheDocument()
  })
})
