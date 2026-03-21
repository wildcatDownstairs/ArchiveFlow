/**
 * @fileoverview 文件功能：实现 TaskPage 页面组件
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies react, react-router-dom, react-i18next, lucide-react, @tauri-apps/plugin-dialog, @tauri-apps/plugin-fs
 */

import { useEffect } from "react"
import { useNavigate } from "react-router-dom"
import { useTranslation } from "react-i18next"
import { Trash2, FileArchive, Loader2, Lock, Unlock, Download } from "lucide-react"
import { save } from "@tauri-apps/plugin-dialog"
import { writeTextFile } from "@tauri-apps/plugin-fs"
import { cn } from "@/lib/utils"
import { useAppStore } from "@/stores/appStore"
import { useTaskStore } from "@/stores/taskStore"
import { formatFileSize, formatDateTime, buildExportFileName } from "@/lib/format"
import { exportTasks } from "@/services/api"
import type { Task, ExportFormat } from "@/types"

const TYPE_BADGE_COLORS: Record<Task["archive_type"], string> = {
  zip: "bg-blue-500/15 text-blue-700 dark:text-blue-400",
  sevenz: "bg-green-500/15 text-green-700 dark:text-green-400",
  rar: "bg-orange-500/15 text-orange-700 dark:text-orange-400",
  unknown: "bg-gray-500/15 text-gray-700 dark:text-gray-400",
}

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

const EXPORTABLE_STATUSES: Task["status"][] = [
  "succeeded",
  "exhausted",
  "cancelled",
  "failed",
  "interrupted",
]

/**
 * 该方法/组件暂无详细描述，由自动脚本补充
 * @returns {any} 默认返回
 */
export default function TaskPage() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const recoveryPreferences = useAppStore((state) => state.recoveryPreferences)
  const { tasks, loading, fetchTasks, removeTask } = useTaskStore()

  useEffect(() => {
    void fetchTasks()
  }, [fetchTasks])

  /**
   *
   * @param e
   * @param taskId
   */
  const handleDelete = async (e: React.MouseEvent, taskId: string) => {
    e.stopPropagation()
    if (!window.confirm(t("delete_confirm"))) return
    try {
      await removeTask(taskId)
    } catch (err) {
      console.error("Failed to delete task:", err)
    }
  }

  const exportableTasks = tasks.filter((task) => EXPORTABLE_STATUSES.includes(task.status))
  const exportButtonClassName =
    "flex items-center gap-1.5 rounded-md border border-border bg-background px-3 py-1.5 text-sm text-foreground hover:bg-muted transition-colors"



  /**
   *
   * @param format
    * @returns {any} 执行结果
 */
const handleExportAll = async (format: ExportFormat) => {
    if (exportableTasks.length === 0) {
      window.alert(t("export_no_tasks"))
      return
    }
    const defaultName = buildExportFileName(format)
    const ext = format === "csv" ? "csv" : "json"

    const filePath = await save({
      defaultPath: defaultName,
      filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
    })
    if (!filePath) return

    try {
      const ids = exportableTasks.map((t) => t.id)
      const content = await exportTasks(ids, format, {
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

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">{t("tasks")}</h1>
        {exportableTasks.length > 0 && (
          <div className="flex items-center gap-2">
            <button
              onClick={() => void handleExportAll("json")}
              className={exportButtonClassName}
              title={t("export_all") + " (JSON)"}
            >
              <Download className="h-4 w-4 text-amber-500" />
              {t("export_all")} JSON
            </button>
            <button
              onClick={() => void handleExportAll("csv")}
              className={exportButtonClassName}
              title={t("export_all") + " (CSV)"}
            >
              <Download className="h-4 w-4 text-amber-500" />
              {t("export_all")} CSV
            </button>
          </div>
        )}
      </div>

      {tasks.length === 0 ? (
        <p className="text-muted-foreground">{t("no_tasks")}</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b text-left text-muted-foreground">
                <th className="pb-3 pr-4 font-medium">{t("file_name")}</th>
                <th className="pb-3 pr-4 font-medium">{t("file_type")}</th>
                <th className="pb-3 pr-4 font-medium">{t("file_status")}</th>
                <th className="pb-3 pr-4 font-medium">{t("encrypted")}</th>
                <th className="pb-3 pr-4 font-medium">{t("file_size")}</th>
                <th className="pb-3 pr-4 font-medium">{t("total_entries")}</th>
                <th className="pb-3 pr-4 font-medium">{t("created_at")}</th>
                <th className="pb-3 font-medium">{t("actions")}</th>
              </tr>
            </thead>
            <tbody>
              {tasks.map((task) => (
                <tr
                  key={task.id}
                  className="border-b last:border-b-0 hover:bg-muted/50 transition-colors cursor-pointer"
                  onClick={() => navigate(`/tasks/${task.id}`)}
                >
                  <td className="py-3 pr-4">
                    <div className="flex items-center gap-2">
                      <FileArchive className="h-4 w-4 text-muted-foreground flex-shrink-0" />
                      <span className="truncate max-w-[200px]">
                        {task.file_name}
                      </span>
                    </div>
                  </td>
                  <td className="py-3 pr-4">
                    <span
                      className={cn(
                        "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium",
                        TYPE_BADGE_COLORS[task.archive_type],
                      )}
                    >
                      {t(`type_${task.archive_type}`)}
                    </span>
                  </td>
                  <td className="py-3 pr-4">
                    <span
                      className={cn(
                        "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium",
                        STATUS_BADGE_COLORS[task.status],
                      )}
                    >
                      {t(`status_${task.status}`)}
                    </span>
                  </td>
                  <td className="py-3 pr-4">
                    {task.archive_info ? (
                      task.archive_info.is_encrypted ? (
                        <span className="flex items-center gap-1 text-xs text-amber-600">
                          <Lock className="h-3.5 w-3.5" />
                          {t("encrypted")}
                        </span>
                      ) : (
                        <span className="flex items-center gap-1 text-xs text-green-600">
                          <Unlock className="h-3.5 w-3.5" />
                          {t("not_encrypted")}
                        </span>
                      )
                    ) : (
                      <span className="text-xs text-muted-foreground">-</span>
                    )}
                  </td>
                  <td className="py-3 pr-4 text-muted-foreground">
                    {formatFileSize(task.file_size)}
                  </td>
                  <td className="py-3 pr-4 text-muted-foreground">
                    {task.archive_info ? task.archive_info.total_entries : "-"}
                  </td>
                  <td className="py-3 pr-4 text-muted-foreground">
                    {formatDateTime(task.created_at)}
                  </td>
                  <td className="py-3">
                    <button
                      onClick={(e) => void handleDelete(e, task.id)}
                      className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs text-red-600 hover:bg-red-50 transition-colors"
                      title={t("delete")}
                    >
                      <Trash2 className="h-4 w-4" />
                      {t("delete")}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
