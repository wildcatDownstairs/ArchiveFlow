/**
 * @fileoverview 文件功能：实现 HomePage 页面组件
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies react, react-router-dom, react-i18next, lucide-react, @tauri-apps/plugin-dialog, @tauri-apps/plugin-fs, @tauri-apps/api/event
 */

import { useState, useEffect, useCallback } from "react"
import { useNavigate } from "react-router-dom"
import { useTranslation } from "react-i18next"
import { Upload, FileArchive, Lock, Unlock } from "lucide-react"
import { open } from "@tauri-apps/plugin-dialog"
import { stat } from "@tauri-apps/plugin-fs"
import { listen } from "@tauri-apps/api/event"
import { cn } from "@/lib/utils"
import { useTaskStore } from "@/stores/taskStore"
import { formatDateTime, formatFileSize, getFileNameFromPath } from "@/lib/format"
import type { Task } from "@/types"

const ALLOWED_EXTENSIONS = ["zip", "7z", "rar"]

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

/**
 * 该方法/组件暂无详细描述，由自动脚本补充
 * @returns {any} 默认返回
 */
export default function HomePage() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const { tasks, fetchTasks } = useTaskStore()
  const importArchive = useTaskStore((s) => s.importArchive)

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
          // stat may fail; backend will read the actual size
        }
        const task = await importArchive(filePath, fileName, fileSize)
        navigate(`/tasks/${task.id}`)
      } catch (e) {
        setError(String(e))
      } finally {
        setImporting(false)
      }
    },
    [importArchive, navigate],
  )

  /**
   * 检查文件后缀是否在允许的列表中
   * @param path - 文件路径
   * @returns {boolean} 是否允许
   */
  const isAllowedFile = (path: string): boolean => {
    const ext = path.split(".").pop()?.toLowerCase() ?? ""
    return ALLOWED_EXTENSIONS.includes(ext)
  }

  // Tauri v2 intercepts OS-level drag-drop before DOM events reach the WebView.
  // We must use Tauri's native drag-drop events instead of React's onDrop/onDragOver.
  useEffect(() => {
    const unlistenDrop = listen<{ paths: string[] }>("tauri://drag-drop", (event) => {
      setDragging(false)
      const paths = event.payload.paths
      if (paths.length > 0) {
        const filePath = paths[0]
        if (isAllowedFile(filePath)) {
          void handleImport(filePath)
        }
      }
    })

    const unlistenOver = listen("tauri://drag-over", () => {
      setDragging(true)
    })

    const unlistenLeave = listen("tauri://drag-leave", () => {
      setDragging(false)
    })

    return () => {
      void unlistenDrop.then((f) => f())
      void unlistenOver.then((f) => f())
      void unlistenLeave.then((f) => f())
    }
  }, [handleImport])

  const handleClick = useCallback(async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Archives", extensions: ["zip", "7z", "rar"] }],
      })
      if (selected) {
        void handleImport(selected)
      }
    } catch {
      // user cancelled dialog
    }
  }, [handleImport])

  const recentTasks = tasks.slice(0, 5)

  return (
    <div className="p-6 space-y-8">
      <h1 className="text-2xl font-bold">{t("home")}</h1>

      {/* Drop zone */}
      <div
        onClick={() => void handleClick()}
        className={cn(
          "flex flex-col items-center justify-center rounded-lg border-2 border-dashed p-12 text-center transition-colors cursor-pointer",
          dragging
            ? "border-primary bg-primary/5"
            : "border-gray-300 hover:border-primary",
        )}
      >
        <Upload
          className={cn(
            "h-12 w-12 mb-4 transition-colors",
            dragging ? "text-primary" : "text-muted-foreground",
          )}
        />
        {importing ? (
          <p className="text-primary text-lg">{t("importing")}</p>
        ) : (
          <>
            <p className="text-muted-foreground text-lg">{t("drag_hint")}</p>
            <p className="text-muted-foreground text-sm mt-2">
              {t("or")}{" "}
              <span className="text-primary underline">{t("select_file")}</span>
            </p>
          </>
        )}
      </div>

      {error && (
        <div className="rounded-md bg-red-50 border border-red-200 p-3 text-red-700 text-sm">
          {t("import_error")}: {error}
        </div>
      )}

      {/* Recent tasks */}
      <section>
        <h2 className="text-xl font-semibold mb-4">{t("recent_tasks")}</h2>
        {recentTasks.length === 0 ? (
          <p className="text-muted-foreground">{t("no_tasks")}</p>
        ) : (
          <div className="space-y-2">
            {recentTasks.map((task) => (
              <div
                key={task.id}
                className="flex items-center gap-4 rounded-lg border p-3 hover:bg-muted/50 transition-colors cursor-pointer"
                onClick={() => navigate(`/tasks/${task.id}`)}
              >
                <FileArchive className="h-5 w-5 text-muted-foreground flex-shrink-0" />
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium truncate">
                    {task.file_name}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    {formatDateTime(task.created_at)}
                  </p>
                </div>
                {/* 加密状态指示 */}
                {task.archive_info && (
                  <div className="flex items-center gap-1 text-xs">
                    {task.archive_info.is_encrypted ? (
                      <span className="flex items-center gap-1 text-amber-600">
                        <Lock className="h-3.5 w-3.5" />
                        {t("encrypted")}
                      </span>
                    ) : (
                      <span className="flex items-center gap-1 text-green-600">
                        <Unlock className="h-3.5 w-3.5" />
                        {t("not_encrypted")}
                      </span>
                    )}
                  </div>
                )}
                {/* 文件大小 */}
                <span className="text-xs text-muted-foreground">
                  {formatFileSize(task.file_size)}
                </span>
                <span
                  className={cn(
                    "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium",
                    TYPE_BADGE_COLORS[task.archive_type],
                  )}
                >
                  {t(`type_${task.archive_type}`)}
                </span>
                <span
                  className={cn(
                    "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium",
                    STATUS_BADGE_COLORS[task.status],
                  )}
                >
                  {t(`status_${task.status}`)}
                </span>
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  )
}
