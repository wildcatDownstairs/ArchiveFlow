import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import type { Task, ArchiveInfo, RecoveryProgress } from "@/types"

export async function getTasks(): Promise<Task[]> {
  return invoke<Task[]>("get_tasks")
}

export async function createTask(
  filePath: string,
  fileName: string,
  fileSize: number,
): Promise<Task> {
  return invoke<Task>("create_task", { filePath, fileName, fileSize })
}

export async function getTask(taskId: string): Promise<Task | null> {
  return invoke<Task | null>("get_task", { taskId })
}

export async function deleteTask(taskId: string): Promise<void> {
  return invoke<void>("delete_task", { taskId })
}

export async function updateTaskStatus(
  taskId: string,
  status: string,
  errorMessage?: string | null,
): Promise<void> {
  return invoke<void>("update_task_status", { taskId, status, errorMessage })
}

export async function inspectArchive(
  filePath: string,
): Promise<ArchiveInfo> {
  return invoke<ArchiveInfo>("inspect_archive", { filePath })
}

/// 一站式导入：创建任务 + 检测归档内容
export async function importArchive(
  filePath: string,
  fileName: string,
  fileSize: number,
): Promise<Task> {
  return invoke<Task>("import_archive", { filePath, fileName, fileSize })
}

// --- 密码恢复 ---

/// 启动密码恢复
export async function startRecovery(
  taskId: string,
  mode: "dictionary" | "bruteforce",
  configJson: string,
): Promise<void> {
  return invoke<void>("start_recovery", { taskId, mode, configJson })
}

/// 取消密码恢复
export async function cancelRecovery(taskId: string): Promise<void> {
  return invoke<void>("cancel_recovery", { taskId })
}

/// 监听恢复进度事件
export function onRecoveryProgress(
  callback: (progress: RecoveryProgress) => void,
): Promise<UnlistenFn> {
  return listen<RecoveryProgress>("recovery-progress", (event) => {
    callback(event.payload)
  })
}
