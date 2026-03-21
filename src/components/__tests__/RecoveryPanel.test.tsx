import { act, render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { beforeEach, describe, expect, it, vi } from "vitest"
import "@/i18n"
import RecoveryPanel from "@/components/RecoveryPanel"
import * as api from "@/services/api"
import { useAppStore } from "@/stores/appStore"
import type {
  RecoveryCheckpoint,
  RecoveryProgress,
  RecoverySchedulerSnapshot,
  ScheduledRecovery,
  Task,
} from "@/types"
import { open } from "@tauri-apps/plugin-dialog"
import { readTextFile } from "@tauri-apps/plugin-fs"

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

const READY_TASK: Task = {
  ...RUNNING_TASK,
  status: "interrupted",
}

const STALE_QUEUED_RECOVERY: ScheduledRecovery = {
  task_id: RUNNING_TASK.id,
  mode: { type: "dictionary", wordlist: ["alpha"] },
  priority: 2,
  backend: "cpu",
  hashcat_path: null,
  state: "queued",
  requested_at: "2026-03-21T00:00:00Z",
  started_at: null,
}

const STALE_RUNNING_RECOVERY: ScheduledRecovery = {
  task_id: RUNNING_TASK.id,
  mode: { type: "dictionary", wordlist: ["alpha"] },
  priority: 2,
  backend: "cpu",
  hashcat_path: null,
  state: "running",
  requested_at: "2026-03-21T00:00:00Z",
  started_at: "2026-03-21T00:00:01Z",
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
        defaultTaskPriority: 2,
        autoIncludeFilenamePatterns: false,
        autoClearDictionaryInput: false,
        resultRetentionPolicy: "plaintext",
        exportMaskPasswords: false,
        exportIncludeAuditEvents: true,
        maxConcurrentRecoveries: 1,
        hashcatPath: "C:/Tools/hashcat/hashcat.exe",
      },
      recoveryDrafts: {
        dictionaryText: "",
        dictionarySourceName: null,
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

  it("恢复断点卡片会显示上次保存的优先级和时间", async () => {
    const checkpoint: RecoveryCheckpoint = {
      task_id: READY_TASK.id,
      mode: { type: "mask", mask: "?d?d?d?d" },
      archive_type: "zip",
      priority: 5,
      tried: 320,
      total: 10_000,
      updated_at: "2026-03-21T08:30:00Z",
    }

    vi.mocked(api.getRecoveryCheckpoint).mockResolvedValue(checkpoint)

    render(<RecoveryPanel task={READY_TASK} />)

    expect(await screen.findByText("检测到可继续的恢复断点")).toBeInTheDocument()
    expect(screen.getByText("优先级: 5")).toBeInTheDocument()
    expect(
      screen.getByText((content) => content.startsWith("最近断点保存:")),
    ).toBeInTheDocument()
  })

  it("导入的字典文本会在面板重新挂载后保留", async () => {
    const user = userEvent.setup()
    vi.mocked(open).mockResolvedValue("C:/tmp/passwords.txt")
    vi.mocked(readTextFile).mockResolvedValue("alpha\nbeta")

    const { unmount } = render(<RecoveryPanel task={READY_TASK} />)

    await user.click(await screen.findByRole("button", { name: "导入字典文件" }))

    await waitFor(() => {
      expect(screen.getByRole("textbox")).toHaveValue("alpha\nbeta")
    })
    expect(screen.getByText("passwords.txt")).toBeInTheDocument()

    unmount()

    render(<RecoveryPanel task={READY_TASK} />)

    await waitFor(() => {
      expect(screen.getByRole("textbox")).toHaveValue("alpha\nbeta")
    })
    expect(screen.getByText("passwords.txt")).toBeInTheDocument()
  })

  it("字典恢复结束后即使父任务状态还没刷新也能重新切换模式", async () => {
    vi.mocked(api.getScheduledRecovery).mockResolvedValue(STALE_QUEUED_RECOVERY)
    vi.mocked(api.getRecoverySchedulerSnapshot).mockResolvedValue({
      ...EMPTY_SCHEDULER_SNAPSHOT,
      queued_count: 1,
      tasks: [STALE_QUEUED_RECOVERY],
    })

    render(<RecoveryPanel task={RUNNING_TASK} />)

    await waitFor(() => {
      expect(api.onRecoveryProgress).toHaveBeenCalledTimes(1)
    })

    act(() => {
      progressListener?.({
        task_id: RUNNING_TASK.id,
        tried: 12,
        total: 12,
        speed: 100,
        status: "exhausted",
        found_password: null,
        elapsed_seconds: 0.2,
        worker_count: 4,
        last_checkpoint_at: "2026-03-21T00:00:00Z",
      })
    })

    expect(await screen.findByText("已穷尽所有候选密码")).toBeInTheDocument()
    expect(await screen.findByRole("button", { name: "暴力破解" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "开始恢复" })).toBeInTheDocument()
  })

  it("字典恢复结束后即使API返回运行中的调度记录也能切换模式", async () => {
    vi.mocked(api.getScheduledRecovery).mockResolvedValue(STALE_RUNNING_RECOVERY)
    vi.mocked(api.getRecoverySchedulerSnapshot).mockResolvedValue({
      ...EMPTY_SCHEDULER_SNAPSHOT,
      running_count: 1,
      tasks: [STALE_RUNNING_RECOVERY],
    })

    render(<RecoveryPanel task={RUNNING_TASK} />)

    await waitFor(() => {
      expect(api.onRecoveryProgress).toHaveBeenCalledTimes(1)
    })

    act(() => {
      progressListener?.({
        task_id: RUNNING_TASK.id,
        tried: 12,
        total: 12,
        speed: 100,
        status: "exhausted",
        found_password: null,
        elapsed_seconds: 0.2,
        worker_count: 4,
        last_checkpoint_at: "2026-03-21T00:00:00Z",
      })
    })

    expect(await screen.findByText("已穷尽所有候选密码")).toBeInTheDocument()
    expect(await screen.findByRole("button", { name: "暴力破解" })).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "开始恢复" })).toBeInTheDocument()
  })

  it("选择 GPU 后端时会把后端和 hashcat 路径传给启动命令", async () => {
    const user = userEvent.setup()
    vi.mocked(api.startRecovery).mockResolvedValue("running")

    render(<RecoveryPanel task={READY_TASK} />)

    await user.click(await screen.findByLabelText("外部 GPU (hashcat)"))
    await user.click(screen.getByRole("button", { name: "暴力破解" }))
    await user.click(screen.getByRole("button", { name: "开始恢复" }))

    await waitFor(() => {
      expect(api.startRecovery).toHaveBeenCalledWith(
        READY_TASK.id,
        "bruteforce",
        expect.any(String),
        2,
        "gpu",
        "C:/Tools/hashcat/hashcat.exe",
      )
    })
  })
})
