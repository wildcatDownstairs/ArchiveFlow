import { useState, useEffect, useCallback } from "react"
import { Link } from "react-router-dom"
import { useTranslation } from "react-i18next"
import { FileText, RefreshCw, Filter, Loader2, ArrowRight } from "lucide-react"
import { cn } from "@/lib/utils"
import { formatDateTime } from "@/lib/format"
import { getAuditEvents } from "@/services/api"
import type { AuditEvent, AuditEventType } from "@/types"

// Badge color mapping for each audit event type
const EVENT_BADGE_COLORS: Record<AuditEventType, string> = {
  file_imported: "bg-blue-100 text-blue-800",
  task_status_updated: "bg-sky-100 text-sky-800",
  task_deleted: "bg-orange-100 text-orange-800",
  tasks_cleared: "bg-orange-100 text-orange-800",
  task_failed: "bg-red-100 text-red-800",
  task_unsupported: "bg-slate-200 text-slate-800",
  task_interrupted: "bg-orange-100 text-orange-800",
  recovery_started: "bg-indigo-100 text-indigo-800",
  recovery_succeeded: "bg-green-100 text-green-800",
  recovery_exhausted: "bg-amber-100 text-amber-800",
  recovery_cancelled: "bg-gray-200 text-gray-800",
  recovery_failed: "bg-red-100 text-red-800",
  audit_logs_cleared: "bg-gray-100 text-gray-800",
  setting_changed: "bg-cyan-100 text-cyan-800",
  authorization_granted: "bg-purple-100 text-purple-800",
  result_exported: "bg-teal-100 text-teal-800",
  cache_cleared: "bg-gray-100 text-gray-800",
}

// All filterable event types
const ALL_EVENT_TYPES: AuditEventType[] = [
  "file_imported",
  "task_status_updated",
  "task_deleted",
  "tasks_cleared",
  "task_failed",
  "task_unsupported",
  "task_interrupted",
  "recovery_started",
  "recovery_succeeded",
  "recovery_exhausted",
  "recovery_cancelled",
  "recovery_failed",
  "audit_logs_cleared",
  "setting_changed",
  "authorization_granted",
  "result_exported",
  "cache_cleared",
]

export default function ReportPage() {
  const { t } = useTranslation()
  const [events, setEvents] = useState<AuditEvent[]>([])
  const [loading, setLoading] = useState(true)
  const [filter, setFilter] = useState<AuditEventType | "all">("all")

  const loadEvents = useCallback(async () => {
    setLoading(true)
    try {
      const data = await getAuditEvents(200)
      setEvents(data)
    } catch (err) {
      console.error("Failed to load audit events:", err)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadEvents()
  }, [loadEvents])

  const filteredEvents =
    filter === "all"
      ? events
      : events.filter((e) => e.event_type === filter)

  // Loading state
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
      {/* Header */}
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">{t("reports")}</h1>
        <button
          onClick={() => void loadEvents()}
          className="inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-sm hover:bg-muted transition-colors"
        >
          <RefreshCw className="h-4 w-4" />
          {t("refresh")}
        </button>
      </div>

      {/* Toolbar: filter + count */}
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-2">
          <Filter className="h-4 w-4 text-muted-foreground" />
          <select
            value={filter}
            onChange={(e) =>
              setFilter(e.target.value as AuditEventType | "all")
            }
            className="rounded-md border bg-background px-3 py-1.5 text-sm outline-none focus:ring-2 focus:ring-ring"
          >
            <option value="all">{t("filter_all")}</option>
            {ALL_EVENT_TYPES.map((type) => (
              <option key={type} value={type}>
                {t(`event_${type}`)}
              </option>
            ))}
          </select>
        </div>
        <span className="text-sm text-muted-foreground">
          {t("total_events", { count: filteredEvents.length })}
        </span>
      </div>

      {/* Table or empty state */}
      {filteredEvents.length === 0 ? (
        <div className="rounded-lg border p-8 text-center">
          <FileText className="mx-auto h-10 w-10 text-muted-foreground/50" />
          <p className="mt-3 text-muted-foreground">{t("no_audit_events")}</p>
        </div>
      ) : (
        <div className="overflow-x-auto rounded-lg border">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b bg-muted/40 text-left text-muted-foreground">
                <th className="px-4 py-3 font-medium">{t("event_type")}</th>
                <th className="px-4 py-3 font-medium">
                  {t("event_description")}
                </th>
                <th className="px-4 py-3 font-medium">{t("related_task")}</th>
                <th className="px-4 py-3 font-medium">{t("event_time")}</th>
              </tr>
            </thead>
            <tbody>
              {filteredEvents.map((event) => (
                <tr
                  key={event.id}
                  className="border-b last:border-b-0 hover:bg-muted/50 transition-colors"
                >
                  {/* Event type badge */}
                  <td className="px-4 py-3">
                    <span
                      className={cn(
                        "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium",
                        EVENT_BADGE_COLORS[event.event_type],
                      )}
                    >
                      {t(`event_${event.event_type}`)}
                    </span>
                  </td>

                  {/* Description */}
                  <td className="px-4 py-3 text-muted-foreground">
                    {event.description}
                  </td>

                  {/* Related task link */}
                  <td className="px-4 py-3">
                    {event.task_id ? (
                      <Link
                        to={`/tasks/${event.task_id}`}
                        className="inline-flex items-center gap-1 text-xs text-primary hover:underline"
                      >
                        {event.task_id.slice(0, 8)}
                        <ArrowRight className="h-3 w-3" />
                      </Link>
                    ) : (
                      <span className="text-xs text-muted-foreground">-</span>
                    )}
                  </td>

                  {/* Timestamp */}
                  <td className="px-4 py-3 text-muted-foreground whitespace-nowrap">
                    {formatDateTime(event.timestamp)}
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
