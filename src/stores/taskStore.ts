import { create } from "zustand"
import type { Task } from "@/types"
import * as api from "@/services/api"

interface TaskState {
  tasks: Task[]
  loading: boolean
  error: string | null
  currentTask: Task | null
  fetchTasks: () => Promise<void>
  addTask: (
    filePath: string,
    fileName: string,
    fileSize: number,
  ) => Promise<Task>
  importArchive: (
    filePath: string,
    fileName: string,
    fileSize: number,
  ) => Promise<Task>
  removeTask: (taskId: string) => Promise<void>
  fetchTask: (taskId: string) => Promise<Task | null>
  updateTaskStatus: (
    taskId: string,
    status: Task["status"],
    errorMessage?: string | null,
  ) => Promise<void>
}

export const useTaskStore = create<TaskState>((set, get) => ({
  tasks: [],
  loading: false,
  error: null,
  currentTask: null,
  fetchTasks: async () => {
    set({ loading: true, error: null })
    try {
      const tasks = await api.getTasks()
      set({ tasks, loading: false })
    } catch (e) {
      set({ error: String(e), loading: false })
    }
  },
  addTask: async (filePath, fileName, fileSize) => {
    const task = await api.createTask(filePath, fileName, fileSize)
    set({ tasks: [task, ...get().tasks] })
    return task
  },
  importArchive: async (filePath, fileName, fileSize) => {
    const task = await api.importArchive(filePath, fileName, fileSize)
    set({ tasks: [task, ...get().tasks] })
    return task
  },
  removeTask: async (taskId) => {
    await api.deleteTask(taskId)
    set({ tasks: get().tasks.filter((t) => t.id !== taskId) })
  },
  fetchTask: async (taskId) => {
    try {
      const task = await api.getTask(taskId)
      set({ currentTask: task })
      return task
    } catch (e) {
      set({ error: String(e) })
      return null
    }
  },
  updateTaskStatus: async (taskId, status, errorMessage) => {
    await api.updateTaskStatus(taskId, status, errorMessage)
    set({
      tasks: get().tasks.map((t) =>
        t.id === taskId
          ? { ...t, status, error_message: errorMessage ?? null }
          : t,
      ),
    })
  },
}))
