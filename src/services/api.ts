/**
 * @fileoverview 文件功能：提供 api 接口与服务调用
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies @tauri-apps/api/core, @tauri-apps/api/event
 */

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

type AppLogLevel = "DEBUG" | "INFO" | "WARN" | "ERROR"
type AppLogCategory = "boot" | "ui" | "user" | "process" | "error"
type CommandArgs = Record<string, unknown>

interface CommandLogOptions {
  category?: AppLogCategory
  message?: string
}

function formatUnknownError(error: unknown): string {
  if (error instanceof Error) return error.message
  return String(error)
}

function nowMs(): number {
  return typeof performance === "undefined" ? Date.now() : performance.now()
}

export async function appendAppLog(
  level: AppLogLevel,
  category: AppLogCategory,
  message: string,
): Promise<void> {
  return invoke<void>("append_app_log", { level, category, message })
}

export function recordAppLog(
  level: AppLogLevel,
  category: AppLogCategory,
  message: string,
): void {
  void appendAppLog(level, category, message).catch(() => undefined)
}

async function invokeCommand<T>(
  command: string,
  args?: CommandArgs,
  logOptions: CommandLogOptions = {},
): Promise<T> {
  const category = logOptions.category ?? "process"
  const action = logOptions.message ?? `调用后端命令: ${command}`
  const startedAt = nowMs()

  recordAppLog("INFO", category, `${action} - 开始`)
  try {
    const result = await invoke<T>(command, args)
    const elapsedMs = Math.round(nowMs() - startedAt)
    recordAppLog("INFO", category, `${action} - 完成 (${elapsedMs}ms)`)
    return result
  } catch (error) {
    const elapsedMs = Math.round(nowMs() - startedAt)
    recordAppLog(
      "ERROR",
      "error",
      `${action} - 失败 (${elapsedMs}ms): ${formatUnknownError(error)}`,
    )
    throw error
  }
}

/**
 * 该方法/组件暂无详细描述，由自动脚本补充
 * @returns {any} 默认返回
 */
export async function getTasks(): Promise<Task[]> {
  return invokeCommand<Task[]>("get_tasks", undefined, {
    message: "加载任务列表",
  })
}

/**
 *
 * @param filePath
 * @param fileName
 * @param fileSize
  * @returns {any} 执行结果
 */
export async function createTask(
  filePath: string,
  fileName: string,
  fileSize: number,
): Promise<Task> {
  return invokeCommand<Task>(
    "create_task",
    { filePath, fileName, fileSize },
    { category: "user", message: `创建任务: ${fileName}` },
  )
}

/**
 *
 * @param taskId
  * @returns {any} 执行结果
 */
export async function getTask(taskId: string): Promise<Task | null> {
  return invokeCommand<Task | null>("get_task", { taskId }, {
    message: `加载任务详情: ${taskId}`,
  })
}

/**
 *
 * @param taskId
  * @returns {any} 执行结果
 */
export async function deleteTask(taskId: string): Promise<void> {
  return invokeCommand<void>("delete_task", { taskId }, {
    category: "user",
    message: `删除任务: ${taskId}`,
  })
}

/**
 *
 * @param taskId
 * @param status
 * @param errorMessage
  * @returns {any} 执行结果
 */
export async function updateTaskStatus(
  taskId: string,
  status: Task["status"],
  errorMessage?: string | null,
): Promise<void> {
  return invokeCommand<void>(
    "update_task_status",
    { taskId, status, errorMessage },
    { category: "user", message: `更新任务状态: ${taskId} -> ${status}` },
  )
}

/**
 *
 * @param filePath
  * @returns {any} 执行结果
 */
export async function inspectArchive(
  filePath: string,
): Promise<ArchiveInfo> {
  return invokeCommand<ArchiveInfo>("inspect_archive", { filePath }, {
    category: "process",
    message: "检测压缩包",
  })
}

/// 一站式导入：创建任务 + 检测归档内容
/**
 *
 * @param filePath
 * @param fileName
 * @param fileSize
  * @returns {any} 执行结果
 */
export async function importArchive(
  filePath: string,
  fileName: string,
  fileSize: number,
): Promise<Task> {
  return invokeCommand<Task>(
    "import_archive",
    { filePath, fileName, fileSize },
    { category: "user", message: `导入压缩包: ${fileName}` },
  )
}

// --- 密码恢复 ---

/// 启动密码恢复
/**
 *
 * @param taskId
 * @param mode
 * @param configJson
 * @param priority
 * @param backend
 * @param hashcatPath
  * @returns {any} 执行结果
 */
export async function startRecovery(
  taskId: string,
  mode: "dictionary" | "bruteforce" | "mask",
  configJson: string,
  priority?: number,
  backend?: RecoveryBackend,
  hashcatPath?: string,
): Promise<ScheduledRecoveryState> {
  return invokeCommand<ScheduledRecoveryState>(
    "start_recovery",
    {
      taskId,
      mode,
      configJson,
      priority: priority ?? null,
      backend: backend ?? "cpu",
      hashcatPath: hashcatPath?.trim() || null,
    },
    {
      category: "user",
      message: `启动恢复: task=${taskId}, mode=${mode}, backend=${backend ?? "cpu"}`,
    },
  )
}

/**
 *
 * @param customPath
  * @returns {any} 执行结果
 */
export async function detectHashcat(
  customPath?: string,
): Promise<HashcatDetectionResult> {
  return invokeCommand<HashcatDetectionResult>(
    "detect_hashcat",
    {
      customPath: customPath?.trim() ? customPath.trim() : null,
    },
    { message: "检测 hashcat 环境" },
  )
}

/// 取消密码恢复
/**
 *
 * @param taskId
  * @returns {any} 执行结果
 */
export async function cancelRecovery(taskId: string): Promise<void> {
  return invokeCommand<void>("cancel_recovery", { taskId }, {
    category: "user",
    message: `取消恢复: ${taskId}`,
  })
}

/**
 *
 * @param taskId
  * @returns {any} 执行结果
 */
export async function getRecoveryCheckpoint(
  taskId: string,
): Promise<RecoveryCheckpoint | null> {
  return invokeCommand<RecoveryCheckpoint | null>(
    "get_recovery_checkpoint",
    { taskId },
    { message: `读取恢复断点: ${taskId}` },
  )
}

/**
 *
 * @param taskId
  * @returns {any} 执行结果
 */
export async function resumeRecovery(taskId: string): Promise<ScheduledRecoveryState> {
  return invokeCommand<ScheduledRecoveryState>("resume_recovery", { taskId }, {
    category: "user",
    message: `继续恢复: ${taskId}`,
  })
}

/**
 *
 * @param taskId
  * @returns {any} 执行结果
 */
export async function getScheduledRecovery(
  taskId: string,
): Promise<ScheduledRecovery | null> {
  return invokeCommand<ScheduledRecovery | null>(
    "get_scheduled_recovery",
    { taskId },
    { message: `读取恢复调度状态: ${taskId}` },
  )
}

/**
 * 该方法/组件暂无详细描述，由自动脚本补充
 * @returns {any} 默认返回
 */
export async function getRecoverySchedulerSnapshot(): Promise<RecoverySchedulerSnapshot> {
  return invokeCommand<RecoverySchedulerSnapshot>(
    "get_recovery_scheduler_snapshot",
    undefined,
    { message: "读取恢复调度快照" },
  )
}

/**
 *
 * @param maxConcurrent
  * @returns {any} 执行结果
 */
export async function setRecoverySchedulerLimit(
  maxConcurrent: number,
): Promise<RecoverySchedulerSnapshot> {
  return invokeCommand<RecoverySchedulerSnapshot>(
    "set_recovery_scheduler_limit",
    {
      maxConcurrent,
    },
    {
      category: "user",
      message: `设置恢复并发上限: ${maxConcurrent}`,
    },
  )
}

/**
 *
 * @param taskId
  * @returns {any} 执行结果
 */
export async function pauseRecovery(taskId: string): Promise<void> {
  return invokeCommand<void>("pause_recovery", { taskId }, {
    category: "user",
    message: `暂停恢复: ${taskId}`,
  })
}

// --- Audit events ---

/**
 *
 * @param limit
  * @returns {any} 执行结果
 */
export async function getAuditEvents(limit?: number): Promise<AuditEvent[]> {
  return invokeCommand<AuditEvent[]>(
    "get_audit_events",
    { limit: limit ?? null },
    { message: `读取审计日志: limit=${limit ?? 100}` },
  )
}

/**
 *
 * @param taskId
  * @returns {any} 执行结果
 */
export async function getTaskAuditEvents(taskId: string): Promise<AuditEvent[]> {
  return invokeCommand<AuditEvent[]>("get_task_audit_events", { taskId }, {
    message: `读取任务审计日志: ${taskId}`,
  })
}

// --- Recovery progress listener ---
/**
 *
 * @param callback
  * @returns {any} 执行结果
 */
export function onRecoveryProgress(
  callback: (progress: RecoveryProgress) => void,
): Promise<UnlistenFn> {
  return listen<RecoveryProgress>("recovery-progress", (event) => {
    callback(event.payload)
  })
}

// --- Settings ---

/**
 * 该方法/组件暂无详细描述，由自动脚本补充
 * @returns {any} 默认返回
 */
export async function getAppDataDir(): Promise<string> {
  return invokeCommand<string>("get_app_data_dir", undefined, {
    message: "读取应用数据目录",
  })
}

/**
 * 该方法/组件暂无详细描述，由自动脚本补充
 * @returns {any} 默认返回
 */
export async function clearAllTasks(): Promise<number> {
  return invokeCommand<number>("clear_all_tasks", undefined, {
    category: "user",
    message: "清空全部任务",
  })
}

/**
 * 该方法/组件暂无详细描述，由自动脚本补充
 * @returns {any} 默认返回
 */
export async function clearAuditEvents(): Promise<number> {
  return invokeCommand<number>("clear_audit_events", undefined, {
    category: "user",
    message: "清空审计日志",
  })
}

/**
 *
 * @param settingKey
 * @param oldValue
 * @param newValue
  * @returns {any} 执行结果
 */
export async function recordSettingChange(
  settingKey: string,
  oldValue: string | null,
  newValue: string,
): Promise<void> {
  return invokeCommand<void>(
    "record_setting_change",
    {
      settingKey,
      oldValue,
      newValue,
    },
    { category: "user", message: `记录设置变更: ${settingKey}` },
  )
}

/**
 * 该方法/组件暂无详细描述，由自动脚本补充
 * @returns {any} 默认返回
 */
export async function getStats(): Promise<[number, number]> {
  return invokeCommand<[number, number]>("get_stats", undefined, {
    message: "读取应用统计",
  })
}

// --- Export ---

/**
 *
 * @param taskIds
 * @param format
 * @param options
  * @returns {any} 执行结果
 */
export async function exportTasks(
  taskIds: string[],
  format: ExportFormat,
  options?: ExportOptions,
): Promise<string> {
  return invokeCommand<string>(
    "export_tasks",
    { taskIds, format, options: options ?? null },
    {
      category: "user",
      message: `导出任务: count=${taskIds.length}, format=${format}`,
    },
  )
}

export async function getLogDir(): Promise<string> {
  return invokeCommand<string>("get_log_dir", undefined, {
    message: "读取文本日志目录",
  })
}

export async function openLogDir(): Promise<string> {
  return invokeCommand<string>("open_log_dir", undefined, {
    category: "user",
    message: "打开文本日志目录",
  })
}
