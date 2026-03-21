import { render, screen, waitFor } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { save } from "@tauri-apps/plugin-dialog"
import { writeTextFile } from "@tauri-apps/plugin-fs"
import { MemoryRouter } from "react-router-dom"
import { beforeEach, describe, expect, it, vi } from "vitest"
import "@/i18n"
import TaskPage from "@/pages/TaskPage"
import * as api from "@/services/api"
import { useAppStore } from "@/stores/appStore"
import { useTaskStore } from "@/stores/taskStore"
import type { Task } from "@/types"

vi.mock("@/services/api", () => ({
  exportTasks: vi.fn(),
}))

const TASK: Task = {
  id: "task-1",
  file_path: "C:/tmp/archive.zip",
  file_name: "archive.zip",
  file_size: 2048,
  archive_type: "zip",
  status: "succeeded",
  created_at: "2026-03-21T00:00:00Z",
  updated_at: "2026-03-21T00:00:00Z",
  error_message: null,
  found_password: "secret123",
  archive_info: {
    total_entries: 1,
    total_size: 2048,
    is_encrypted: true,
    has_encrypted_filenames: false,
    entries: [],
  },
}

describe("TaskPage", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    window.localStorage.clear()
    window.alert = vi.fn()

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
        defaultTaskPriority: 0,
        autoIncludeFilenamePatterns: false,
        autoClearDictionaryInput: false,
        resultRetentionPolicy: "plaintext",
        exportMaskPasswords: true,
        exportIncludeAuditEvents: false,
        maxConcurrentRecoveries: 1,
        hashcatPath: "",
      },
    }))

    useTaskStore.setState({
      tasks: [TASK],
      loading: false,
      error: null,
      currentTask: null,
      fetchTasks: vi.fn().mockResolvedValue(undefined),
      addTask: vi.fn(),
      importArchive: vi.fn(),
      removeTask: vi.fn(),
      fetchTask: vi.fn(),
      updateTaskStatus: vi.fn(),
    })

    vi.mocked(save).mockResolvedValue("C:/tmp/export.json")
    vi.mocked(writeTextFile).mockResolvedValue(undefined)
    vi.mocked(api.exportTasks).mockResolvedValue('{"ok":true}')
  })

  it("批量导出会带上设置页里的默认导出选项", async () => {
    const user = userEvent.setup()
    render(
      <MemoryRouter>
        <TaskPage />
      </MemoryRouter>,
    )

    await user.click(await screen.findByRole("button", { name: "导出全部 JSON" }))

    await waitFor(() => {
      expect(api.exportTasks).toHaveBeenCalledWith(["task-1"], "json", {
        maskPasswords: true,
        includeAuditEvents: false,
      })
    })
    expect(writeTextFile).toHaveBeenCalledWith("C:/tmp/export.json", '{"ok":true}')
  })
})
