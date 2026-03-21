import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import type {
  Task,
  ArchiveInfo,
  RecoveryProgress,
  AuditEvent,
  ExportFormat,
  ExportOptions,
  RecoveryCheckpoint,
  RecoverySchedulerSnapshot,
  ScheduledRecovery,
  ScheduledRecoveryState,
  RecoveryBackend,
  HashcatDetectionResult,
} from "@/types"

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
  status: Task["status"],
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
  mode: "dictionary" | "bruteforce" | "mask",
  configJson: string,
  priority?: number,
  backend?: RecoveryBackend,
  hashcatPath?: string,
): Promise<ScheduledRecoveryState> {
  return invoke<ScheduledRecoveryState>("start_recovery", {
    taskId,
    mode,
    configJson,
    priority: priority ?? null,
    backend: backend ?? "cpu",
    hashcatPath: hashcatPath ?? null,
  })
}

export async function detectHashcat(
  customPath?: string,
): Promise<HashcatDetectionResult> {
  return invoke<HashcatDetectionResult>("detect_hashcat", {
    customPath: customPath?.trim() ? customPath.trim() : null,
  })
}

/// 取消密码恢复
export async function cancelRecovery(taskId: string): Promise<void> {
  return invoke<void>("cancel_recovery", { taskId })
}

export async function getRecoveryCheckpoint(
  taskId: string,
): Promise<RecoveryCheckpoint | null> {
  return invoke<RecoveryCheckpoint | null>("get_recovery_checkpoint", { taskId })
}

export async function resumeRecovery(taskId: string): Promise<ScheduledRecoveryState> {
  return invoke<ScheduledRecoveryState>("resume_recovery", { taskId })
}

export async function getScheduledRecovery(
  taskId: string,
): Promise<ScheduledRecovery | null> {
  return invoke<ScheduledRecovery | null>("get_scheduled_recovery", { taskId })
}

export async function getRecoverySchedulerSnapshot(): Promise<RecoverySchedulerSnapshot> {
  return invoke<RecoverySchedulerSnapshot>("get_recovery_scheduler_snapshot")
}

export async function setRecoverySchedulerLimit(
  maxConcurrent: number,
): Promise<RecoverySchedulerSnapshot> {
  return invoke<RecoverySchedulerSnapshot>("set_recovery_scheduler_limit", {
    maxConcurrent,
  })
}

export async function pauseRecovery(taskId: string): Promise<void> {
  return invoke<void>("pause_recovery", { taskId })
}

// --- Audit events ---

export async function getAuditEvents(limit?: number): Promise<AuditEvent[]> {
  return invoke<AuditEvent[]>("get_audit_events", { limit: limit ?? null })
}

export async function getTaskAuditEvents(taskId: string): Promise<AuditEvent[]> {
  return invoke<AuditEvent[]>("get_task_audit_events", { taskId })
}

// --- Recovery progress listener ---
export function onRecoveryProgress(
  callback: (progress: RecoveryProgress) => void,
): Promise<UnlistenFn> {
  return listen<RecoveryProgress>("recovery-progress", (event) => {
    callback(event.payload)
  })
}

// --- Settings ---

export async function getAppDataDir(): Promise<string> {
  return invoke<string>("get_app_data_dir")
}

export async function clearAllTasks(): Promise<number> {
  return invoke<number>("clear_all_tasks")
}

export async function clearAuditEvents(): Promise<number> {
  return invoke<number>("clear_audit_events")
}

export async function recordSettingChange(
  settingKey: string,
  oldValue: string | null,
  newValue: string,
): Promise<void> {
  return invoke<void>("record_setting_change", {
    settingKey,
    oldValue,
    newValue,
  })
}

export async function getStats(): Promise<[number, number]> {
  return invoke<[number, number]>("get_stats")
}

// --- Export ---

export async function exportTasks(
  taskIds: string[],
  format: ExportFormat,
  options?: ExportOptions,
): Promise<string> {
  return invoke<string>("export_tasks", { taskIds, format, options: options ?? null })
}
