import { useEffect } from "react"
import { useNavigate } from "react-router-dom"
import { useTranslation } from "react-i18next"
import { Trash2, FileArchive, Loader2, Lock, Unlock, Download } from "lucide-react"
import { save } from "@tauri-apps/plugin-dialog"
import { writeTextFile } from "@tauri-apps/plugin-fs"
import { cn } from "@/lib/utils"
import { useTaskStore } from "@/stores/taskStore"
import { formatFileSize, formatDateTime } from "@/lib/format"
import { exportTasks } from "@/services/api"
import type { Task, ExportFormat } from "@/types"

const TYPE_BADGE_COLORS: Record<Task["archive_type"], string> = {
  zip: "bg-blue-100 text-blue-800",
  sevenz: "bg-green-100 text-green-800",
  rar: "bg-orange-100 text-orange-800",
  unknown: "bg-gray-100 text-gray-800",
}

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

const EXPORTABLE_STATUSES: Task["status"][] = [
  "succeeded",
  "exhausted",
  "cancelled",
  "failed",
  "interrupted",
]

export default function TaskPage() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const { tasks, loading, fetchTasks, removeTask } = useTaskStore()

  useEffect(() => {
    void fetchTasks()
  }, [fetchTasks])

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

  const handleExportAll = async (format: ExportFormat) => {
    if (exportableTasks.length === 0) {
      window.alert(t("export_no_tasks"))
      return
    }
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
      const ids = exportableTasks.map((t) => t.id)
      const content = await exportTasks(ids, format)
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
              className="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm text-blue-600 hover:bg-blue-50 transition-colors border border-blue-200"
              title={t("export_all") + " (JSON)"}
            >
              <Download className="h-4 w-4" />
              {t("export_all")} JSON
            </button>
            <button
              onClick={() => void handleExportAll("csv")}
              className="flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm text-blue-600 hover:bg-blue-50 transition-colors border border-blue-200"
              title={t("export_all") + " (CSV)"}
            >
              <Download className="h-4 w-4" />
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
