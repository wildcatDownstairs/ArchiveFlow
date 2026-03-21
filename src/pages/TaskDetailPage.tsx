/**
 * @fileoverview 文件功能：实现 TaskDetailPage 页面组件
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies react, react-router-dom, react-i18next, @tauri-apps/plugin-dialog, @tauri-apps/plugin-fs
 */

import { useCallback, useEffect, useState } from "react"
import { useParams, useNavigate } from "react-router-dom"
import { useTranslation } from "react-i18next"
import {
  ArrowLeft,
  FileArchive,
  Lock,
  Folder,
  File,
  Loader2,
  Trash2,
  HardDrive,
  Files,
  ShieldCheck,
  ShieldAlert,
  Download,
  Zap,
  ChevronDown,
  ChevronRight,
} from "lucide-react"
import { save, ask } from "@tauri-apps/plugin-dialog"
import { writeTextFile } from "@tauri-apps/plugin-fs"
import { cn } from "@/lib/utils"
import { useAppStore } from "@/stores/appStore"
import { useTaskStore } from "@/stores/taskStore"
import { formatFileSize, formatDateTime, buildExportFileName } from "@/lib/format"
import { buildFileTree, type TreeNode } from "@/lib/fileTree"
import { exportTasks } from "@/services/api"
import RecoveryPanel from "@/components/RecoveryPanel"
import type { Task, ExportFormat } from "@/types"

const STATUS_BADGE_COLORS: Record<Task["status"], string> = {
  ready: "bg-cyan-500/15 text-cyan-700 dark:text-cyan-400",
  processing: "bg-indigo-500/15 text-indigo-700 dark:text-indigo-400",
  succeeded: "bg-green-500/15 text-green-700 dark:text-green-400",
  exhausted: "bg-amber-500/15 text-amber-700 dark:text-amber-400",
  cancelled: "bg-gray-500/15 text-gray-700 dark:text-gray-400",
  failed: "bg-red-500/15 text-red-700 dark:text-red-400",
  unsupported: "bg-slate-500/15 text-slate-700 dark:text-slate-400",
  interrupted: "bg-orange-500/15 text-orange-700 dark:text-orange-400",
}

const TYPE_BADGE_COLORS: Record<Task["archive_type"], string> = {
  zip: "bg-blue-500/15 text-blue-700 dark:text-blue-400",
  sevenz: "bg-green-500/15 text-green-700 dark:text-green-400",
  rar: "bg-orange-500/15 text-orange-700 dark:text-orange-400",
  unknown: "bg-gray-500/15 text-gray-700 dark:text-gray-400",
}

const EXPORTABLE_STATUSES: Task["status"][] = [
  "succeeded",
  "exhausted",
  "cancelled",
  "failed",
  "interrupted",
]

const EXPORT_BUTTON_CLASS_NAME =
  "flex items-center gap-1.5 rounded-md border border-border bg-background px-3 py-1.5 text-sm text-foreground hover:bg-muted transition-colors"

/**
 *
 * @param root0
 * @param root0.node
 * @param root0.t
  * @returns {JSX.Element} 渲染的 React 元素
 */
function FileTreeNode({ node, t }: { node: TreeNode; t: (key: string) => string }) {
  const [expanded, setExpanded] = useState(true)

  if (node.isDirectory) {
    return (
      <div>
        <button
          onClick={() => setExpanded(!expanded)}
          className="flex items-center gap-2 py-1 px-2 w-full text-left hover:bg-muted/50 rounded text-sm"
        >
          <Folder className="h-4 w-4 text-amber-500 flex-shrink-0" />
          <span className="font-medium">{node.name}</span>
          <span className="text-xs text-muted-foreground ml-auto">
            {node.children.length} {t("items")}
          </span>
        </button>
        {expanded && node.children.length > 0 && (
          <div className="ml-4 border-l border-gray-200 pl-2">
            {node.children.map((child) => (
              <FileTreeNode key={child.path} node={child} t={t} />
            ))}
          </div>
        )}
      </div>
    )
  }

  const entry = node.entry
  return (
    <div className="flex items-center gap-2 py-1 px-2 text-sm hover:bg-muted/50 rounded">
      <File className="h-4 w-4 text-muted-foreground flex-shrink-0" />
      <span className="truncate flex-1">{node.name}</span>
      {entry?.is_encrypted && (
        <Lock className="h-3.5 w-3.5 text-amber-500 flex-shrink-0" />
      )}
      {entry && (
        <span className="text-xs text-muted-foreground flex-shrink-0">
          {formatFileSize(entry.size)}
        </span>
      )}
    </div>
  )
}

/**
 * 该方法/组件暂无详细描述，由自动脚本补充
 * @returns {any} 默认返回
 */
export default function TaskDetailPage() {
  const { t } = useTranslation()
  const { taskId } = useParams<{ taskId: string }>()
  const navigate = useNavigate()
  const recoveryPreferences = useAppStore((state) => state.recoveryPreferences)
  const { currentTask, fetchTask, removeTask } = useTaskStore()
  const [loading, setLoading] = useState(true)
  const [isDeleting, setIsDeleting] = useState(false)
  const [isFileTreeExpanded, setIsFileTreeExpanded] = useState(true)



  const loadTask = useCallback(async (id: string) => {
    setLoading(true)
    try {
      await fetchTask(id)
    } finally {
      setLoading(false)
    }
  }, [fetchTask])

  useEffect(() => {
    if (!taskId) return
    void loadTask(taskId)
  }, [taskId, loadTask])

  // 恢复状态变化时刷新任务数据
  const handleRecoveryStatusChange = useCallback(() => {
    if (taskId) {
      // 延迟一点让后端 DB 更新完成
      setTimeout(() => void fetchTask(taskId), 500)
    }
  }, [taskId, fetchTask])

  /**
 * 该方法/组件暂无详细描述，由自动脚本补充
 * @returns {any} 默认返回
 */
  const handleDelete = async () => {
    if (!taskId) return
    const confirmed = await ask(t("delete_confirm"), { kind: "warning" })
    if (!confirmed) return
    setIsDeleting(true)
    try {
      await removeTask(taskId)
      navigate("/tasks")
    } catch (err) {
      console.error("Failed to delete task:", err)
    } finally {
      setIsDeleting(false)
    }
  }

  /**
   *
   * @param format
    * @returns {any} 执行结果
 */
const handleExport = async (format: ExportFormat) => {
    if (!currentTask) return
    const defaultName = buildExportFileName(format, currentTask.file_name)
    const ext = format === "csv" ? "csv" : "json"

    const filePath = await save({
      defaultPath: defaultName,
      filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
    })
    if (!filePath) return

    try {
      const content = await exportTasks([currentTask.id], format, {
        maskPasswords: recoveryPreferences.exportMaskPasswords,
        includeAuditEvents: recoveryPreferences.exportIncludeAuditEvents,
      })
      await writeTextFile(filePath, content)
      window.alert(t("export_success"))
    } catch (err) {
      console.error("Export failed:", err)
      window.alert(t("export_error"))
    }
  }

  if (loading) {
    return (
      <div className="p-6 flex items-center gap-2 text-muted-foreground">
        <Loader2 className="h-5 w-5 animate-spin" />
        <span>{t("loading")}</span>
      </div>
    )
  }

  if (!currentTask) {
    return (
      <div className="p-6 space-y-4">
        <button
          onClick={() => navigate("/tasks")}
          className="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors"
        >
          <ArrowLeft className="h-4 w-4" />
          {t("back_to_tasks")}
        </button>
        <p className="text-muted-foreground">{t("task_not_found")}</p>
      </div>
    )
  }

  const task = currentTask
  const info = task.archive_info
  const fileTree = info ? buildFileTree(info.entries) : []
  const canExportTask = EXPORTABLE_STATUSES.includes(task.status)

  return (
    <div className="p-6 space-y-6">
      {/* 导航 */}
      <div className="flex items-center justify-between">
        <button
          onClick={() => navigate("/tasks")}
          className="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors"
        >
          <ArrowLeft className="h-4 w-4" />
          {t("back_to_tasks")}
        </button>
        <div className="flex items-center gap-2">
          {canExportTask && (
            <>
              <button
                onClick={() => void handleExport("json")}
                className={EXPORT_BUTTON_CLASS_NAME}
                title={t("export_results") + " (JSON)"}
              >
                <Download className="h-4 w-4 text-amber-500" />
                JSON
              </button>
              <button
                onClick={() => void handleExport("csv")}
                className={EXPORT_BUTTON_CLASS_NAME}
                title={t("export_results") + " (CSV)"}
              >
                <Download className="h-4 w-4 text-amber-500" />
                CSV
              </button>
            </>
          )}
          <button
            onClick={() => void handleDelete()}
            disabled={isDeleting}
            className="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm text-red-600 hover:bg-red-50 transition-colors border border-red-200 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {isDeleting ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Trash2 className="h-4 w-4" />
            )}
            {t("delete")}
          </button>
        </div>
      </div>

      {/* 标题 */}
      <div className="flex items-center gap-3">
        <FileArchive className="h-8 w-8 text-muted-foreground" />
        <div>
          <h1 className="text-2xl font-bold">{task.file_name}</h1>
          <p className="text-sm text-muted-foreground truncate max-w-xl">
            {task.file_path}
          </p>
        </div>
      </div>

      {/* 综合信息条 */}
      <div className="rounded-lg border bg-card shadow-sm p-4 md:p-5">
        <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:flex lg:flex-row lg:flex-wrap lg:items-center lg:justify-between gap-6">
          {/* 类型 */}
          <div className="space-y-1.5">
            <p className="text-xs text-muted-foreground font-medium flex items-center gap-1.5">
              <FileArchive className="h-3.5 w-3.5" />
              {t("file_type")}
            </p>
            <div>
              <span className={cn("inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-semibold", TYPE_BADGE_COLORS[task.archive_type])}>
                {t(`type_${task.archive_type}`)}
              </span>
            </div>
          </div>

          {/* 状态 */}
          <div className="space-y-1.5">
            <p className="text-xs text-muted-foreground font-medium flex items-center gap-1.5">
              <Zap className="h-3.5 w-3.5" />
              {t("file_status")}
            </p>
            <div>
              <span className={cn("inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-semibold", STATUS_BADGE_COLORS[task.status])}>
                {t(`status_${task.status}`)}
              </span>
            </div>
          </div>

          {/* 大小 */}
          <div className="space-y-1.5">
            <p className="text-xs text-muted-foreground font-medium flex items-center gap-1.5">
              <HardDrive className="h-3.5 w-3.5" />
              {t("file_size")}
            </p>
            <p className="text-sm font-semibold">{formatFileSize(task.file_size)}</p>
          </div>

          {/* 加密状态 */}
          <div className="space-y-1.5">
            <p className="text-xs text-muted-foreground font-medium flex items-center gap-1.5">
              <ShieldCheck className="h-3.5 w-3.5" />
              {t("encryption")}
            </p>
            <div className="flex items-center">
              {info ? (
                info.is_encrypted ? (
                  <span className="flex items-center gap-1 text-sm font-semibold text-amber-600 dark:text-amber-500">
                    <ShieldAlert className="h-4 w-4" />
                    {t("encrypted")}
                  </span>
                ) : (
                  <span className="flex items-center gap-1 text-sm font-semibold text-green-600 dark:text-green-500">
                    <ShieldCheck className="h-4 w-4" />
                    {t("not_encrypted")}
                  </span>
                )
              ) : (
                <span className="text-sm font-semibold text-muted-foreground">-</span>
              )}
            </div>
          </div>

          {info && (
            <>
              {/* 大屏分割线 */}
              <div className="hidden lg:block w-px h-8 bg-border mx-2"></div>

              {/* 总文件数 */}
              <div className="space-y-1.5">
                <p className="text-xs text-muted-foreground font-medium flex items-center gap-1.5">
                  <Files className="h-3.5 w-3.5" />
                  {t("total_entries")}
                </p>
                <p className="text-sm font-semibold">{info.total_entries}</p>
              </div>

              {/* 解压大小 */}
              <div className="space-y-1.5">
                <p className="text-xs text-muted-foreground font-medium flex items-center gap-1.5">
                  <HardDrive className="h-3.5 w-3.5" />
                  {t("uncompressed_size")}
                </p>
                <p className="text-sm font-semibold">{formatFileSize(info.total_size)}</p>
              </div>

              {/* 加密文件数 */}
              <div className="space-y-1.5">
                <p className="text-xs text-muted-foreground font-medium flex items-center gap-1.5">
                  <Lock className="h-3.5 w-3.5" />
                  {t("encrypted_entries")}
                </p>
                <p className="text-sm font-semibold">
                  <span className={info.entries.some((e) => e.is_encrypted) ? "text-amber-600 dark:text-amber-500" : ""}>
                    {info.entries.filter((e) => e.is_encrypted).length}
                  </span>
                  <span className="text-muted-foreground font-normal ml-0.5">/ {info.entries.filter((e) => !e.is_directory).length}</span>
                </p>
              </div>
            </>
          )}
        </div>
      </div>

      {/* 时间信息 */}
      <div className="flex gap-6 text-sm text-muted-foreground">
        <span>{t("created_at")}: {formatDateTime(task.created_at)}</span>
        <span>{t("updated_at")}: {formatDateTime(task.updated_at)}</span>
      </div>

      {/* 错误信息 */}
      {task.error_message && (
        <div className="rounded-md bg-red-50 border border-red-200 p-3 text-red-700 text-sm">
          {task.error_message}
        </div>
      )}

      {/* 主体内容：主区 + 右侧边栏 */}
      <div className="flex flex-col xl:flex-row gap-6 items-start">
        {/* 左侧：文件树 */}
        {info && info.entries.length > 0 && (
          <div className="flex flex-col gap-6 flex-1 min-w-0">
            <section className="space-y-3">
              <button 
                onClick={() => setIsFileTreeExpanded(!isFileTreeExpanded)}
                className="flex items-center gap-2 text-lg font-semibold hover:text-indigo-600 transition-colors focus:outline-none"
              >
                {isFileTreeExpanded ? (
                  <ChevronDown className="h-5 w-5" />
                ) : (
                  <ChevronRight className="h-5 w-5" />
                )}
                {t("archive_contents")}
              </button>
              
              {isFileTreeExpanded && (
                <div className="max-h-[calc(100vh-24rem)] rounded-lg border bg-card p-4 overflow-y-auto">
                  {fileTree.map((node) => (
                    <FileTreeNode key={node.path} node={node} t={t} />
                  ))}
                </div>
              )}
            </section>
          </div>
        )}

        {/* 右侧：密码恢复面板 - 仅在有加密文件时显示 */}
        {(task.archive_type === "zip" || task.archive_type === "sevenz" || task.archive_type === "rar") && info?.is_encrypted && (
          <div className="flex flex-col gap-6 flex-1 min-w-0">
            <RecoveryPanel
              task={task}
              onTaskStatusChange={handleRecoveryStatusChange}
            />
          </div>
        )}
      </div>
    </div>
  )
}
