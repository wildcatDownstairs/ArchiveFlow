import { vi } from "vitest"
import type { Task } from "@/types"

// Mock the entire api module before importing the store
vi.mock("@/services/api", () => ({
  getTasks: vi.fn(),
  createTask: vi.fn(),
  importArchive: vi.fn(),
  deleteTask: vi.fn(),
  getTask: vi.fn(),
  updateTaskStatus: vi.fn(),
  startRecovery: vi.fn(),
  cancelRecovery: vi.fn(),
  onRecoveryProgress: vi.fn(),
  getAuditEvents: vi.fn(),
  getTaskAuditEvents: vi.fn(),
  inspectArchive: vi.fn(),
  getAppDataDir: vi.fn(),
  clearAllTasks: vi.fn(),
  clearAuditEvents: vi.fn(),
  getStats: vi.fn(),
}))

// Import after mock is set up
import { useTaskStore } from "../taskStore"
import * as api from "@/services/api"

const mockedApi = vi.mocked(api)

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: "task-1",
    file_path: "/tmp/test.zip",
    file_name: "test.zip",
    file_size: 1024,
    archive_type: "zip",
    status: "ready",
    created_at: "2024-01-01T00:00:00Z",
    updated_at: "2024-01-01T00:00:00Z",
    error_message: null,
    found_password: null,
    archive_info: null,
    ...overrides,
  }
}

describe("taskStore", () => {
  beforeEach(() => {
    // Reset store state
    useTaskStore.setState({
      tasks: [],
      loading: false,
      error: null,
      currentTask: null,
    })
    vi.clearAllMocks()
  })

  describe("initial state", () => {
    it("has empty tasks array", () => {
      expect(useTaskStore.getState().tasks).toEqual([])
    })

    it("has loading false", () => {
      expect(useTaskStore.getState().loading).toBe(false)
    })

    it("has error null", () => {
      expect(useTaskStore.getState().error).toBeNull()
    })

    it("has currentTask null", () => {
      expect(useTaskStore.getState().currentTask).toBeNull()
    })
  })

  describe("fetchTasks", () => {
    it("sets loading=true during fetch and populates tasks on success", async () => {
      const tasks = [makeTask({ id: "t1" }), makeTask({ id: "t2" })]
      mockedApi.getTasks.mockResolvedValue(tasks)

      const fetchPromise = useTaskStore.getState().fetchTasks()

      // loading should be true while fetch is in progress
      expect(useTaskStore.getState().loading).toBe(true)
      expect(useTaskStore.getState().error).toBeNull()

      await fetchPromise

      expect(useTaskStore.getState().loading).toBe(false)
      expect(useTaskStore.getState().tasks).toEqual(tasks)
    })

    it("sets error on failure", async () => {
      mockedApi.getTasks.mockRejectedValue(new Error("Network error"))

      await useTaskStore.getState().fetchTasks()

      expect(useTaskStore.getState().loading).toBe(false)
      expect(useTaskStore.getState().error).toContain("Network error")
      expect(useTaskStore.getState().tasks).toEqual([])
    })
  })

  describe("importArchive", () => {
    it("adds returned task to the front of the list", async () => {
      const existing = makeTask({ id: "existing" })
      useTaskStore.setState({ tasks: [existing] })

      const newTask = makeTask({ id: "new-task" })
      mockedApi.importArchive.mockResolvedValue(newTask)

      const result = await useTaskStore.getState().importArchive("/tmp/a.zip", "a.zip", 2048)

      expect(result).toEqual(newTask)
      const tasks = useTaskStore.getState().tasks
      expect(tasks).toHaveLength(2)
      expect(tasks[0].id).toBe("new-task")
      expect(tasks[1].id).toBe("existing")
    })
  })

  describe("removeTask", () => {
    it("removes task by id from the list", async () => {
      const t1 = makeTask({ id: "t1" })
      const t2 = makeTask({ id: "t2" })
      useTaskStore.setState({ tasks: [t1, t2] })

      mockedApi.deleteTask.mockResolvedValue(undefined)

      await useTaskStore.getState().removeTask("t1")

      const tasks = useTaskStore.getState().tasks
      expect(tasks).toHaveLength(1)
      expect(tasks[0].id).toBe("t2")
    })
  })

  describe("fetchTask", () => {
    it("sets currentTask on success", async () => {
      const task = makeTask({ id: "task-detail" })
      mockedApi.getTask.mockResolvedValue(task)

      const result = await useTaskStore.getState().fetchTask("task-detail")

      expect(result).toEqual(task)
      expect(useTaskStore.getState().currentTask).toEqual(task)
    })

    it("sets error and returns null on failure", async () => {
      mockedApi.getTask.mockRejectedValue(new Error("Not found"))

      const result = await useTaskStore.getState().fetchTask("missing")

      expect(result).toBeNull()
      expect(useTaskStore.getState().error).toContain("Not found")
    })
  })

  describe("updateTaskStatus", () => {
    it("updates the status of a task in the list", async () => {
      const t1 = makeTask({ id: "t1", status: "ready" })
      useTaskStore.setState({ tasks: [t1] })

      mockedApi.updateTaskStatus.mockResolvedValue(undefined)

      await useTaskStore.getState().updateTaskStatus("t1", "processing")

      const updated = useTaskStore.getState().tasks[0]
      expect(updated.status).toBe("processing")
      expect(updated.error_message).toBeNull()
    })

    it("updates status with error message", async () => {
      const t1 = makeTask({ id: "t1", status: "processing" })
      useTaskStore.setState({ tasks: [t1] })

      mockedApi.updateTaskStatus.mockResolvedValue(undefined)

      await useTaskStore.getState().updateTaskStatus("t1", "failed", "Something broke")

      const updated = useTaskStore.getState().tasks[0]
      expect(updated.status).toBe("failed")
      expect(updated.error_message).toBe("Something broke")
    })
  })
})
