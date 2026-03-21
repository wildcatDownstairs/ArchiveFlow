/**
 * @fileoverview 文件功能：实现 taskStore 状态管理
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies zustand
 */

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
  /**
 * 该方法/组件暂无详细描述，由自动脚本补充
 * @returns {any} 默认返回
 */
  fetchTasks: async () => {
    set({ loading: true, error: null })
    try {
      const tasks = await api.getTasks()
      set({ tasks, loading: false })
    } catch (e) {
      set({ error: String(e), loading: false })
    }
  },
  /**
   * 添加任务
   * @param filePath - 文件路径
   * @param fileName - 文件名
   * @param fileSize - 文件大小
   * @returns {Promise<Task>} 创建的任务
   */
  addTask: async (filePath, fileName, fileSize) => {
    const task = await api.createTask(filePath, fileName, fileSize)
    set({ tasks: [task, ...get().tasks] })
    return task
  },
  /**
   * 导入归档
   * @param filePath - 文件路径
   * @param fileName - 文件名
   * @param fileSize - 文件大小
   * @returns {Promise<Task>} 导入的任务
   */
  importArchive: async (filePath, fileName, fileSize) => {
    const task = await api.importArchive(filePath, fileName, fileSize)
    set({ tasks: [task, ...get().tasks] })
    return task
  },
  /**
   * 删除任务
   * @param taskId - 任务ID
   * @returns {Promise<void>}
   */
  removeTask: async (taskId) => {
    await api.deleteTask(taskId)
    set({ tasks: get().tasks.filter((t) => t.id !== taskId) })
  },
  /**
   * 获取任务详情
   * @param taskId - 任务ID
   * @returns {Promise<Task | null>} 任务对象
   */
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
  /**
   * 更新任务状态
   * @param taskId - 任务ID
   * @param status - 新状态
   * @param errorMessage - 错误信息
   * @returns {Promise<void>}
   */
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
