import { useState, useEffect, useCallback } from "react"
import { Link, useNavigate } from "react-router-dom"
import { useTranslation } from "react-i18next"
import {
  Upload,
  FileArchive,
  Lock,
  Unlock,
  Loader2,
  ArrowRight,
} from "lucide-react"
import { open } from "@tauri-apps/plugin-dialog"
import { stat } from "@tauri-apps/plugin-fs"
import { listen } from "@tauri-apps/api/event"
import { cn } from "@/lib/utils"
import { useTaskStore } from "@/stores/taskStore"
import { formatDateTime, formatFileSize, getFileNameFromPath } from "@/lib/format"
import { TASK_STATUS_BADGE_CLASSES, TASK_TYPE_BADGE_CLASSES } from "@/lib/ui"
import type { Task } from "@/types"

const ALLOWED_EXTENSIONS = ["zip", "7z", "rar"]
const isTauriEnvironment =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window

export default function HomePage() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const { tasks, fetchTasks } = useTaskStore()
  const importArchive = useTaskStore((state) => state.importArchive)

  const [dragging, setDragging] = useState(false)
  const [importing, setImporting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    void fetchTasks()
  }, [fetchTasks])

  const handleImport = useCallback(
    async (filePath: string) => {
      setImporting(true)
      setError(null)
      try {
        const fileName = getFileNameFromPath(filePath)
        let fileSize = 0
        try {
          const info = await stat(filePath)
          fileSize = info.size
        } catch {
          // Ignore stat failures and let the backend determine the actual size.
        }

        const task = await importArchive(filePath, fileName, fileSize)
        navigate(`/tasks/${task.id}`)
      } catch (event) {
        setError(String(event))
      } finally {
        setImporting(false)
      }
    },
    [importArchive, navigate],
  )

  const isAllowedFile = (path: string): boolean => {
    const ext = path.split(".").pop()?.toLowerCase() ?? ""
    return ALLOWED_EXTENSIONS.includes(ext)
  }

  useEffect(() => {
    if (!isTauriEnvironment) {
      return
    }

    const unlistenDrop = listen<{ paths: string[] }>("tauri://drag-drop", (event) => {
      setDragging(false)
      const filePath = event.payload.paths[0]
      if (filePath && isAllowedFile(filePath)) {
        void handleImport(filePath)
      }
    })

    const unlistenOver = listen("tauri://drag-over", () => {
      setDragging(true)
    })

    const unlistenLeave = listen("tauri://drag-leave", () => {
      setDragging(false)
    })

    return () => {
      void unlistenDrop.then((dispose) => dispose())
      void unlistenOver.then((dispose) => dispose())
      void unlistenLeave.then((dispose) => dispose())
    }
  }, [handleImport])

  const handleSelectFile = useCallback(async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Archives", extensions: ALLOWED_EXTENSIONS }],
      })

      if (selected) {
        void handleImport(selected)
      }
    } catch {
      // User cancelled the system dialog.
    }
  }, [handleImport])

  const recentTasks = tasks.slice(0, 5)
  const encryptedTaskCount = tasks.filter((task) => task.archive_info?.is_encrypted).length
  const finishedTaskCount = tasks.filter((task) =>
    ["succeeded", "exhausted", "cancelled", "failed", "interrupted"].includes(task.status),
  ).length

  return (
    <div className="af-page af-scrollbar-none overflow-y-auto">
      <div className="mx-auto max-w-[1080px] space-y-8">
        <div className="flex flex-col gap-5 xl:flex-row xl:items-end xl:justify-between">
          <div>
            <h1 className="af-page-title">{t("home")}</h1>
            <p className="mt-2 text-sm text-muted-foreground">
              {t("drag_hint")}
            </p>
          </div>

          <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <div className="af-panel-soft min-w-[150px] px-4 py-3">
              <div className="af-kicker">{t("tasks")}</div>
              <div className="mt-2 af-stat-number text-2xl font-bold text-foreground">
                {tasks.length}
              </div>
            </div>
            <div className="af-panel-soft min-w-[150px] px-4 py-3">
              <div className="af-kicker">{t("encrypted")}</div>
              <div className="mt-2 af-stat-number text-2xl font-bold text-foreground">
                {encryptedTaskCount}
              </div>
            </div>
            <div className="af-panel-soft min-w-[150px] px-4 py-3">
              <div className="af-kicker">{t("status_succeeded")}</div>
              <div className="mt-2 af-stat-number text-2xl font-bold text-foreground">
                {finishedTaskCount}
              </div>
            </div>
          </div>
        </div>

        <section
          onClick={() => void handleSelectFile()}
          className={cn(
            "af-panel cursor-pointer px-6 py-10 text-center transition-all sm:px-10 sm:py-14",
            dragging
              ? "border-primary/45 bg-primary/8 shadow-[0_20px_60px_rgba(124,106,247,0.22)]"
              : "hover:border-primary/40 hover:bg-primary/6",
          )}
        >
          <div
            className={cn(
              "mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-secondary transition-all",
              dragging && "bg-primary/14 text-primary",
            )}
          >
            {importing ? (
              <Loader2 className="h-6 w-6 animate-spin text-primary" />
            ) : (
              <Upload
                className={cn(
                  "h-6 w-6 text-muted-foreground transition-colors",
                  dragging && "text-primary",
                )}
              />
            )}
          </div>

          <h2 className="mt-4 text-[15px] font-medium text-foreground">
            {importing ? t("importing") : t("drag_hint")}
          </h2>
          <p className="mt-2 text-sm text-muted-foreground">
            {t("or")}{" "}
            <span className="border-b border-primary/35 text-primary">
              {t("select_file")}
            </span>
          </p>

          <div className="mt-5 flex flex-wrap justify-center gap-2">
            {ALLOWED_EXTENSIONS.map((extension) => (
              <span key={extension} className="af-chip uppercase">
                {extension}
              </span>
            ))}
          </div>
        </section>

        {error && (
          <div className="rounded-2xl border border-rose-400/20 bg-rose-400/10 px-4 py-3 text-sm text-rose-200">
            {t("import_error")}: {error}
          </div>
        )}

        <section>
          <div className="mb-4 flex items-center justify-between gap-4">
            <h2 className="af-display text-[15px] font-bold text-foreground">
              {t("recent_tasks")}
            </h2>
            <Link
              to="/tasks"
              className="inline-flex items-center gap-1 text-xs text-muted-foreground transition-colors hover:text-primary"
            >
              {t("tasks")}
              <ArrowRight className="h-3 w-3" />
            </Link>
          </div>

          {recentTasks.length === 0 ? (
            <div className="af-panel px-6 py-10 text-center text-sm text-muted-foreground">
              {t("no_tasks")}
            </div>
          ) : (
            <div className="space-y-3">
              {recentTasks.map((task) => (
                <RecentTaskRow
                  key={task.id}
                  task={task}
                  onOpen={() => navigate(`/tasks/${task.id}`)}
                />
              ))}
            </div>
          )}
        </section>
      </div>
    </div>
  )
}

function RecentTaskRow({
  task,
  onOpen,
}: {
  task: Task
  onOpen: () => void
}) {
  const { t } = useTranslation()

  return (
    <button
      type="button"
      onClick={onOpen}
      className="af-panel flex w-full flex-col gap-4 px-5 py-4 text-left transition-all hover:bg-secondary/70 sm:flex-row sm:items-center"
    >
      <div className="flex items-center gap-4">
        <div className="flex h-11 w-11 flex-shrink-0 items-center justify-center rounded-[14px] bg-primary/14 text-primary">
          <FileArchive className="h-[18px] w-[18px]" />
        </div>

        <div className="min-w-0">
          <div className="truncate text-sm font-medium text-foreground">
            {task.file_name}
          </div>
          <div className="mt-1 text-xs text-muted-foreground">
            {formatDateTime(task.created_at)}
          </div>
        </div>
      </div>

      <div className="flex flex-1 flex-wrap items-center gap-2 sm:justify-end">
        {task.archive_info && (
          <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
            {task.archive_info.is_encrypted ? (
              <Lock className="h-3.5 w-3.5 text-amber-300" />
            ) : (
              <Unlock className="h-3.5 w-3.5 text-emerald-300" />
            )}
            {task.archive_info.is_encrypted ? t("encrypted") : t("not_encrypted")}
          </span>
        )}

        <span className="text-xs text-muted-foreground">
          {formatFileSize(task.file_size)}
        </span>

        <span className={TASK_TYPE_BADGE_CLASSES[task.archive_type]}>
          {t(`type_${task.archive_type}`)}
        </span>

        <span className={TASK_STATUS_BADGE_CLASSES[task.status]}>
          {t(`status_${task.status}`)}
        </span>
      </div>
    </button>
  )
}
