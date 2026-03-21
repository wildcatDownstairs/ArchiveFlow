import { useEffect, useState, type MouseEvent } from "react"
import { useNavigate } from "react-router-dom"
import { useTranslation } from "react-i18next"
import {
  Trash2,
  FileArchive,
  Loader2,
  Lock,
  Unlock,
  Download,
  ArrowRight,
} from "lucide-react"
import { save, ask } from "@tauri-apps/plugin-dialog"
import { writeTextFile } from "@tauri-apps/plugin-fs"
import { useAppStore } from "@/stores/appStore"
import { useTaskStore } from "@/stores/taskStore"
import { formatFileSize, formatDateTime, buildExportFileName } from "@/lib/format"
import {
  DANGER_BUTTON_CLASS,
  GHOST_BUTTON_CLASS,
  TASK_STATUS_BADGE_CLASSES,
  TASK_TYPE_BADGE_CLASSES,
} from "@/lib/ui"
import { exportTasks } from "@/services/api"
import type { ExportFormat, Task } from "@/types"

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
  const recoveryPreferences = useAppStore((state) => state.recoveryPreferences)
  const { tasks, loading, fetchTasks, removeTask } = useTaskStore()
  const [deletingId, setDeletingId] = useState<string | null>(null)

  useEffect(() => {
    void fetchTasks()
  }, [fetchTasks])

  const handleDelete = async (event: MouseEvent, taskId: string) => {
    event.stopPropagation()
    const confirmed = await ask(t("delete_confirm"), { kind: "warning" })
    if (!confirmed) return

    setDeletingId(taskId)
    try {
      await removeTask(taskId)
    } catch (error) {
      console.error("Failed to delete task:", error)
    } finally {
      setDeletingId(null)
    }
  }

  const exportableTasks = tasks.filter((task) => EXPORTABLE_STATUSES.includes(task.status))
  const encryptedTasks = tasks.filter((task) => task.archive_info?.is_encrypted).length

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
      const ids = exportableTasks.map((task) => task.id)
      const content = await exportTasks(ids, format, {
        maskPasswords: recoveryPreferences.exportMaskPasswords,
        includeAuditEvents: recoveryPreferences.exportIncludeAuditEvents,
      })
      await writeTextFile(filePath, content)
      window.alert(t("export_success"))
    } catch (error) {
      console.error("Export failed:", error)
      window.alert(t("export_error"))
    }
  }

  if (loading) {
    return (
      <div className="af-page flex items-center gap-2 text-muted-foreground">
        <Loader2 className="h-5 w-5 animate-spin" />
        <span>{t("loading")}</span>
      </div>
    )
  }

  return (
    <div className="af-page af-scrollbar-none overflow-y-auto">
      <div className="mx-auto max-w-[1120px] space-y-8">
        <div className="flex flex-col gap-5 xl:flex-row xl:items-end xl:justify-between">
          <div>
            <h1 className="af-page-title">{t("tasks")}</h1>
            <p className="mt-2 text-sm text-muted-foreground">
              {t("total_events", { count: tasks.length })}
            </p>
          </div>

          <div className="flex flex-wrap gap-2">
            <button
              onClick={() => void handleExportAll("json")}
              className={`${GHOST_BUTTON_CLASS} ${exportableTasks.length === 0 ? "pointer-events-none opacity-50" : ""}`}
              disabled={exportableTasks.length === 0}
            >
              <Download className="h-4 w-4 text-primary" />
              {t("export_all")} JSON
            </button>
            <button
              onClick={() => void handleExportAll("csv")}
              className={`${GHOST_BUTTON_CLASS} ${exportableTasks.length === 0 ? "pointer-events-none opacity-50" : ""}`}
              disabled={exportableTasks.length === 0}
            >
              <Download className="h-4 w-4 text-primary" />
              {t("export_all")} CSV
            </button>
          </div>
        </div>

        <div className="grid gap-3 md:grid-cols-3">
          <div className="af-panel-soft px-4 py-3">
            <div className="af-kicker">{t("tasks")}</div>
            <div className="mt-2 af-stat-number text-2xl font-bold text-foreground">
              {tasks.length}
            </div>
          </div>
          <div className="af-panel-soft px-4 py-3">
            <div className="af-kicker">{t("export_results")}</div>
            <div className="mt-2 af-stat-number text-2xl font-bold text-foreground">
              {exportableTasks.length}
            </div>
          </div>
          <div className="af-panel-soft px-4 py-3">
            <div className="af-kicker">{t("encrypted")}</div>
            <div className="mt-2 af-stat-number text-2xl font-bold text-foreground">
              {encryptedTasks}
            </div>
          </div>
        </div>

        {tasks.length === 0 ? (
          <div className="af-panel px-6 py-12 text-center">
            <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-secondary text-muted-foreground">
              <FileArchive className="h-6 w-6" />
            </div>
            <p className="mt-4 text-sm text-muted-foreground">{t("no_tasks")}</p>
          </div>
        ) : (
          <div className="space-y-3">
            {tasks.map((task) => (
              <article
                key={task.id}
                role="button"
                tabIndex={0}
                onClick={() => navigate(`/tasks/${task.id}`)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault()
                    navigate(`/tasks/${task.id}`)
                  }
                }}
                className="af-panel cursor-pointer px-5 py-4 transition-all hover:bg-secondary/70"
              >
                <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-start gap-4">
                      <div className="mt-0.5 flex h-11 w-11 flex-shrink-0 items-center justify-center rounded-[14px] bg-primary/14 text-primary">
                        <FileArchive className="h-[18px] w-[18px]" />
                      </div>

                      <div className="min-w-0 flex-1">
                        <div className="truncate text-sm font-medium text-foreground">
                          {task.file_name}
                        </div>
                        <div className="mt-1 break-all text-xs text-muted-foreground">
                          {task.file_path}
                        </div>

                        <div className="mt-4 grid gap-3 text-sm text-muted-foreground sm:grid-cols-2 xl:grid-cols-4">
                          <div>
                            <div className="af-kicker mb-2">{t("created_at")}</div>
                            <div className="text-sm text-foreground">
                              {formatDateTime(task.created_at)}
                            </div>
                          </div>
                          <div>
                            <div className="af-kicker mb-2">{t("file_size")}</div>
                            <div className="text-sm text-foreground">
                              {formatFileSize(task.file_size)}
                            </div>
                          </div>
                          <div>
                            <div className="af-kicker mb-2">{t("total_entries")}</div>
                            <div className="text-sm text-foreground">
                              {task.archive_info?.total_entries ?? "-"}
                            </div>
                          </div>
                          <div>
                            <div className="af-kicker mb-2">{t("encryption")}</div>
                            <div className="inline-flex items-center gap-1.5 text-sm text-foreground">
                              {task.archive_info ? (
                                task.archive_info.is_encrypted ? (
                                  <>
                                    <Lock className="h-3.5 w-3.5 text-amber-300" />
                                    {t("encrypted")}
                                  </>
                                ) : (
                                  <>
                                    <Unlock className="h-3.5 w-3.5 text-emerald-300" />
                                    {t("not_encrypted")}
                                  </>
                                )
                              ) : (
                                "-"
                              )}
                            </div>
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>

                  <div className="flex flex-wrap items-center gap-2 lg:max-w-[320px] lg:justify-end">
                    <span className={TASK_TYPE_BADGE_CLASSES[task.archive_type]}>
                      {t(`type_${task.archive_type}`)}
                    </span>
                    <span className={TASK_STATUS_BADGE_CLASSES[task.status]}>
                      {t(`status_${task.status}`)}
                    </span>

                    <span className="inline-flex items-center gap-1 text-xs text-primary">
                      {task.id.slice(0, 8)}
                      <ArrowRight className="h-3 w-3" />
                    </span>

                    <button
                      onClick={(event) => void handleDelete(event, task.id)}
                      disabled={deletingId === task.id}
                      className={`${DANGER_BUTTON_CLASS} min-w-[92px]`}
                    >
                      {deletingId === task.id ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        <Trash2 className="h-4 w-4" />
                      )}
                      {t("delete")}
                    </button>
                  </div>
                </div>
              </article>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
