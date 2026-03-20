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
} from "lucide-react"
import { save } from "@tauri-apps/plugin-dialog"
import { writeTextFile } from "@tauri-apps/plugin-fs"
import { cn } from "@/lib/utils"
import { useTaskStore } from "@/stores/taskStore"
import { formatFileSize, formatDateTime } from "@/lib/format"
import { buildFileTree, type TreeNode } from "@/lib/fileTree"
import { exportTasks } from "@/services/api"
import RecoveryPanel from "@/components/RecoveryPanel"
import type { Task, ExportFormat } from "@/types"

const STATUS_BADGE_COLORS: Record<Task["status"], string> = {
  ready: "bg-cyan-100 text-cyan-800",
  processing: "bg-indigo-100 text-indigo-800",
  succeeded: "bg-green-100 text-green-800",
  exhausted: "bg-amber-100 text-amber-800",
  cancelled: "bg-gray-200 text-gray-800",
  failed: "bg-red-100 text-red-800",
  unsupported: "bg-slate-200 text-slate-800",
  interrupted: "bg-orange-100 text-orange-800",
}

const TYPE_BADGE_COLORS: Record<Task["archive_type"], string> = {
  zip: "bg-blue-100 text-blue-800",
  sevenz: "bg-green-100 text-green-800",
  rar: "bg-orange-100 text-orange-800",
  unknown: "bg-gray-100 text-gray-800",
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

export default function TaskDetailPage() {
  const { t } = useTranslation()
  const { taskId } = useParams<{ taskId: string }>()
  const navigate = useNavigate()
  const { currentTask, fetchTask, removeTask } = useTaskStore()
  const [loading, setLoading] = useState(true)

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

  const handleDelete = async () => {
    if (!taskId || !window.confirm(t("delete_confirm"))) return
    try {
      await removeTask(taskId)
      navigate("/tasks")
    } catch (err) {
      console.error("Failed to delete task:", err)
    }
  }

  const handleExport = async (format: ExportFormat) => {
    if (!currentTask) return
    const timestamp = new Date()
      .toISOString()
      .replace(/[-:]/g, "")
      .replace("T", "-")
      .slice(0, 15)
    const defaultName = `archiveflow-export-${timestamp}.${format}`
    const ext = format === "csv" ? "csv" : "json"

    const filePath = await save({
      defaultPath: defaultName,
      filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
    })
    if (!filePath) return

    try {
      const content = await exportTasks([currentTask.id], format)
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
            className="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm text-red-600 hover:bg-red-50 transition-colors border border-red-200"
          >
            <Trash2 className="h-4 w-4" />
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

      {/* 概览卡片 */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        {/* 类型 */}
        <div className="rounded-lg border p-4 space-y-1">
          <p className="text-xs text-muted-foreground">{t("file_type")}</p>
          <span
            className={cn(
              "inline-flex items-center rounded-full px-2.5 py-1 text-sm font-medium",
              TYPE_BADGE_COLORS[task.archive_type],
            )}
          >
            {t(`type_${task.archive_type}`)}
          </span>
        </div>

        {/* 状态 */}
        <div className="rounded-lg border p-4 space-y-1">
          <p className="text-xs text-muted-foreground">{t("file_status")}</p>
          <span
            className={cn(
              "inline-flex items-center rounded-full px-2.5 py-1 text-sm font-medium",
              STATUS_BADGE_COLORS[task.status],
            )}
          >
            {t(`status_${task.status}`)}
          </span>
        </div>

        {/* 文件大小 */}
        <div className="rounded-lg border p-4 space-y-1">
          <p className="text-xs text-muted-foreground">{t("file_size")}</p>
          <div className="flex items-center gap-2">
            <HardDrive className="h-4 w-4 text-muted-foreground" />
            <span className="text-sm font-medium">
              {formatFileSize(task.file_size)}
            </span>
          </div>
        </div>

        {/* 加密状态 */}
        <div className="rounded-lg border p-4 space-y-1">
          <p className="text-xs text-muted-foreground">{t("encryption")}</p>
          {info ? (
            info.is_encrypted ? (
              <div className="flex items-center gap-2 text-amber-600">
                <ShieldAlert className="h-4 w-4" />
                <span className="text-sm font-medium">{t("encrypted")}</span>
              </div>
            ) : (
              <div className="flex items-center gap-2 text-green-600">
                <ShieldCheck className="h-4 w-4" />
                <span className="text-sm font-medium">{t("not_encrypted")}</span>
              </div>
            )
          ) : (
            <span className="text-sm text-muted-foreground">-</span>
          )}
        </div>
      </div>

      {/* 归档详情 */}
      {info && (
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div className="rounded-lg border p-4 flex items-center gap-3">
            <Files className="h-5 w-5 text-muted-foreground" />
            <div>
              <p className="text-xs text-muted-foreground">{t("total_entries")}</p>
              <p className="text-lg font-semibold">{info.total_entries}</p>
            </div>
          </div>
          <div className="rounded-lg border p-4 flex items-center gap-3">
            <HardDrive className="h-5 w-5 text-muted-foreground" />
            <div>
              <p className="text-xs text-muted-foreground">{t("uncompressed_size")}</p>
              <p className="text-lg font-semibold">{formatFileSize(info.total_size)}</p>
            </div>
          </div>
          <div className="rounded-lg border p-4 flex items-center gap-3">
            <Lock className="h-5 w-5 text-muted-foreground" />
            <div>
              <p className="text-xs text-muted-foreground">{t("encrypted_entries")}</p>
              <p className="text-lg font-semibold">
                {info.entries.filter((e) => e.is_encrypted).length} / {info.entries.filter((e) => !e.is_directory).length}
              </p>
            </div>
          </div>
        </div>
      )}

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

      {/* 文件树 */}
      {info && info.entries.length > 0 && (
        <section>
          <h2 className="text-lg font-semibold mb-3">{t("archive_contents")}</h2>
          <div className="rounded-lg border p-4 max-h-96 overflow-y-auto">
            {fileTree.map((node) => (
              <FileTreeNode key={node.path} node={node} t={t} />
            ))}
          </div>
        </section>
      )}

      {/* 密码恢复面板 - 仅在有加密文件时显示 */}
      {(task.archive_type === "zip" || task.archive_type === "sevenz" || task.archive_type === "rar") && info?.is_encrypted && (
        <RecoveryPanel
          task={task}
          onTaskStatusChange={handleRecoveryStatusChange}
        />
      )}
    </div>
  )
}
