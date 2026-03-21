import { useCallback, useEffect, useState, type ReactNode } from "react"
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
  ShieldCheck,
  ShieldAlert,
  Download,
  ChevronDown,
  ChevronRight,
} from "lucide-react"
import { save, ask } from "@tauri-apps/plugin-dialog"
import { writeTextFile } from "@tauri-apps/plugin-fs"
import { useAppStore } from "@/stores/appStore"
import { useTaskStore } from "@/stores/taskStore"
import { formatFileSize, formatDateTime, buildExportFileName } from "@/lib/format"
import { buildFileTree, type TreeNode } from "@/lib/fileTree"
import {
  DANGER_BUTTON_CLASS,
  GHOST_BUTTON_CLASS,
  TASK_STATUS_BADGE_CLASSES,
  TASK_TYPE_BADGE_CLASSES,
} from "@/lib/ui"
import { exportTasks, getTaskAuditEvents } from "@/services/api"
import RecoveryPanel from "@/components/RecoveryPanel"
import type { AuditEvent, ExportFormat, Task } from "@/types"

const EXPORTABLE_STATUSES: Task["status"][] = [
  "succeeded",
  "exhausted",
  "cancelled",
  "failed",
  "interrupted",
]

function FileTreeNode({
  node,
  t,
}: {
  node: TreeNode
  t: (key: string) => string
}) {
  const [expanded, setExpanded] = useState(true)

  if (node.isDirectory) {
    return (
      <div>
        <button
          type="button"
          onClick={() => setExpanded(!expanded)}
          className="flex w-full items-center gap-2 rounded-[10px] px-2 py-2 text-left text-sm transition-colors hover:bg-secondary"
        >
          <Folder className="h-4 w-4 flex-shrink-0 text-amber-300" />
          <span className="truncate text-foreground">{node.name}</span>
          <span className="ml-auto text-xs text-muted-foreground">
            {node.children.length} {t("items")}
          </span>
        </button>

        {expanded && node.children.length > 0 && (
          <div className="ml-4 border-l border-white/6 pl-2">
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
    <div className="flex items-center gap-2 rounded-[10px] px-2 py-2 text-sm transition-colors hover:bg-secondary">
      <File className="h-4 w-4 flex-shrink-0 text-muted-foreground" />
      <span className="min-w-0 flex-1 truncate text-muted-foreground">{node.name}</span>
      {entry?.is_encrypted && <Lock className="h-3.5 w-3.5 flex-shrink-0 text-amber-300" />}
      {entry && (
        <span className="flex-shrink-0 text-xs text-muted-foreground">
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
  const recoveryPreferences = useAppStore((state) => state.recoveryPreferences)
  const { currentTask, fetchTask, removeTask } = useTaskStore()
  const [loading, setLoading] = useState(true)
  const [isDeleting, setIsDeleting] = useState(false)
  const [isFileTreeExpanded, setIsFileTreeExpanded] = useState(true)
  const [auditEvents, setAuditEvents] = useState<AuditEvent[]>([])

  const loadTask = useCallback(async (id: string) => {
    setLoading(true)
    try {
      await fetchTask(id)
    } finally {
      setLoading(false)
    }
  }, [fetchTask])

  const loadAuditEvents = useCallback(async (id: string) => {
    try {
      const events = await getTaskAuditEvents(id)
      setAuditEvents(events.slice(0, 5))
    } catch {
      setAuditEvents([])
    }
  }, [])

  useEffect(() => {
    if (!taskId) return
    void loadTask(taskId)
    void loadAuditEvents(taskId)
  }, [taskId, loadTask, loadAuditEvents])

  const handleRecoveryStatusChange = useCallback(() => {
    if (!taskId) return

    setTimeout(() => {
      void fetchTask(taskId)
      void loadAuditEvents(taskId)
    }, 500)
  }, [taskId, fetchTask, loadAuditEvents])

  const handleDelete = async () => {
    if (!taskId) return
    const confirmed = await ask(t("delete_confirm"), { kind: "warning" })
    if (!confirmed) return

    setIsDeleting(true)
    try {
      await removeTask(taskId)
      navigate("/tasks")
    } catch (error) {
      console.error("Failed to delete task:", error)
    } finally {
      setIsDeleting(false)
    }
  }

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

  if (!currentTask) {
    return (
      <div className="af-page space-y-4">
        <button
          onClick={() => navigate("/tasks")}
          className="inline-flex items-center gap-2 text-sm text-muted-foreground transition-colors hover:text-foreground"
        >
          <ArrowLeft className="h-4 w-4" />
          {t("back_to_tasks")}
        </button>
        <p className="text-sm text-muted-foreground">{t("task_not_found")}</p>
      </div>
    )
  }

  const task = currentTask
  const info = task.archive_info
  const fileTree = info ? buildFileTree(info.entries) : []
  const canExportTask = EXPORTABLE_STATUSES.includes(task.status)
  const hasRecoveryPanel =
    (task.archive_type === "zip" ||
      task.archive_type === "sevenz" ||
      task.archive_type === "rar") &&
    !!info?.is_encrypted

  return (
    <div className="af-page af-scrollbar-none overflow-y-auto">
      <div className="mx-auto max-w-[1320px]">
        <div className="flex flex-col gap-4 border-b border-white/6 pb-5 xl:flex-row xl:items-center xl:justify-between">
          <button
            onClick={() => navigate("/tasks")}
            className="inline-flex items-center gap-2 text-sm text-muted-foreground transition-colors hover:text-foreground"
          >
            <ArrowLeft className="h-4 w-4" />
            {t("back_to_tasks")}
          </button>

          <div className="flex flex-wrap gap-2">
            {canExportTask && (
              <>
                <button
                  onClick={() => void handleExport("json")}
                  className={GHOST_BUTTON_CLASS}
                >
                  <Download className="h-4 w-4 text-primary" />
                  JSON
                </button>
                <button
                  onClick={() => void handleExport("csv")}
                  className={GHOST_BUTTON_CLASS}
                >
                  <Download className="h-4 w-4 text-primary" />
                  CSV
                </button>
              </>
            )}

            <button
              onClick={() => void handleDelete()}
              disabled={isDeleting}
              className={`${DANGER_BUTTON_CLASS} min-w-[92px]`}
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

        <div className="mt-7 flex flex-col gap-7 xl:flex-row">
          <div className="min-w-0 flex-1">
            <div className="mb-6 flex items-start gap-4">
              <div className="mt-1 flex h-12 w-12 flex-shrink-0 items-center justify-center rounded-[14px] bg-primary/14 text-primary">
                <FileArchive className="h-5 w-5" />
              </div>

              <div className="min-w-0">
                <h1 className="af-display break-all text-[22px] font-bold text-foreground">
                  {task.file_name}
                </h1>
                <p className="mt-1 break-all font-mono text-xs text-muted-foreground">
                  {task.file_path}
                </p>
              </div>
            </div>

            <div className="af-panel mb-5 flex flex-wrap items-center gap-4 px-5 py-4 text-sm text-muted-foreground">
              <StatusMeta
                label={t("file_status")}
                value={<span className={TASK_STATUS_BADGE_CLASSES[task.status]}>{t(`status_${task.status}`)}</span>}
              />
              <span className="text-muted-foreground/40">•</span>
              <StatusMeta
                label={t("file_type")}
                value={<span className={TASK_TYPE_BADGE_CLASSES[task.archive_type]}>{t(`type_${task.archive_type}`)}</span>}
              />
              <span className="text-muted-foreground/40">•</span>
              <StatusMeta
                label={t("encryption")}
                value={
                  info?.is_encrypted ? (
                    <span className="inline-flex items-center gap-1.5 text-amber-300">
                      <ShieldAlert className="h-3.5 w-3.5" />
                      {t("encrypted")}
                    </span>
                  ) : (
                    <span className="inline-flex items-center gap-1.5 text-emerald-300">
                      <ShieldCheck className="h-3.5 w-3.5" />
                      {t("not_encrypted")}
                    </span>
                  )
                }
              />
              <span className="text-muted-foreground/40">•</span>
              <StatusMeta
                label={t("total_entries")}
                value={<span className="text-foreground">{info?.total_entries ?? 0}</span>}
              />
            </div>

            {task.error_message && (
              <div className="mb-5 rounded-2xl border border-rose-400/20 bg-rose-400/10 px-4 py-3 text-sm text-rose-200">
                {task.error_message}
              </div>
            )}

            {hasRecoveryPanel && (
              <div className="af-panel p-5">
                <RecoveryPanel
                  task={task}
                  onTaskStatusChange={handleRecoveryStatusChange}
                  onAuditEventsChange={setAuditEvents}
                />
              </div>
            )}
          </div>

          <aside className="w-full flex-shrink-0 space-y-6 xl:w-[280px]">
            <div className="af-panel p-5">
              <div className="mb-4 af-kicker">文件信息</div>
              <InfoRow
                label={t("file_type")}
                value={<span className={TASK_TYPE_BADGE_CLASSES[task.archive_type]}>{t(`type_${task.archive_type}`)}</span>}
              />
              <InfoRow
                label={t("file_status")}
                value={<span className={TASK_STATUS_BADGE_CLASSES[task.status]}>{t(`status_${task.status}`)}</span>}
              />
              <InfoRow label={t("file_size")} value={formatFileSize(task.file_size)} />
              <InfoRow
                label={t("encryption")}
                value={
                  info?.is_encrypted ? (
                    <span className="text-amber-300">{t("encrypted")}</span>
                  ) : (
                    <span className="text-emerald-300">{t("not_encrypted")}</span>
                  )
                }
              />
              {info && (
                <>
                  <InfoRow label={t("total_entries")} value={info.total_entries} />
                  <InfoRow label={t("uncompressed_size")} value={formatFileSize(info.total_size)} />
                  <InfoRow
                    label={t("encrypted_entries")}
                    value={`${info.entries.filter((entry) => entry.is_encrypted).length} / ${info.entries.filter((entry) => !entry.is_directory).length}`}
                  />
                </>
              )}
              <InfoRow label={t("created_at")} value={formatDateTime(task.created_at)} />
              <InfoRow label={t("updated_at")} value={formatDateTime(task.updated_at)} />
            </div>

            <div className="af-panel p-5">
              <div className="mb-4 af-kicker">{t("recent_audit_events")}</div>
              {auditEvents.length === 0 ? (
                <div className="text-sm text-muted-foreground">{t("no_recent_audit_events")}</div>
              ) : (
                <div className="space-y-4">
                  {auditEvents.map((event) => (
                    <div key={event.id} className="border-b border-white/6 pb-4 last:border-b-0 last:pb-0">
                      <div className="font-mono text-[11px] text-muted-foreground">
                        {formatDateTime(event.timestamp)}
                      </div>
                      <div className="mt-1 text-sm leading-6 text-muted-foreground">
                        {event.description}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>

            {info && info.entries.length > 0 && (
              <div className="af-panel p-5">
                <button
                  onClick={() => setIsFileTreeExpanded(!isFileTreeExpanded)}
                  className="flex w-full items-center justify-between gap-3 text-left"
                >
                  <div className="af-kicker">{t("archive_contents")}</div>
                  {isFileTreeExpanded ? (
                    <ChevronDown className="h-4 w-4 text-muted-foreground" />
                  ) : (
                    <ChevronRight className="h-4 w-4 text-muted-foreground" />
                  )}
                </button>

                {isFileTreeExpanded && (
                  <div className="af-panel-soft mt-4 max-h-[320px] overflow-y-auto p-3">
                    {fileTree.map((node) => (
                      <FileTreeNode key={node.path} node={node} t={t} />
                    ))}
                  </div>
                )}
              </div>
            )}
          </aside>
        </div>
      </div>
    </div>
  )
}

function StatusMeta({
  label,
  value,
}: {
  label: string
  value: ReactNode
}) {
  return (
    <div className="inline-flex items-center gap-2">
      <span className="text-muted-foreground">{label}</span>
      {value}
    </div>
  )
}

function InfoRow({
  label,
  value,
}: {
  label: string
  value: ReactNode
}) {
  return (
    <div className="flex items-start justify-between gap-4 border-b border-white/6 py-3 last:border-b-0 last:pb-0 first:pt-0">
      <span className="text-sm text-muted-foreground">{label}</span>
      <span className="text-right text-sm font-medium text-foreground">{value}</span>
    </div>
  )
}
