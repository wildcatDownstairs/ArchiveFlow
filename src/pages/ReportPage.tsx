import { useState, useEffect, useCallback, useRef } from "react"
import { Link } from "react-router-dom"
import { useTranslation } from "react-i18next"
import {
  FileText,
  RefreshCw,
  Filter,
  Loader2,
  ArrowRight,
  ChevronDown,
} from "lucide-react"
import { AUDIT_EVENT_BADGE_CLASSES, GHOST_BUTTON_CLASS } from "@/lib/ui"
import { formatDateTime } from "@/lib/format"
import { getAuditEvents } from "@/services/api"
import { cn } from "@/lib/utils"
import type { AuditEvent, AuditEventType } from "@/types"

const EVENT_FILTER_GROUPS = [
  {
    labelKey: "filter_group_tasks",
    types: [
      "file_imported",
      "task_status_updated",
      "task_deleted",
      "tasks_cleared",
      "task_failed",
      "task_unsupported",
      "task_interrupted",
    ],
  },
  {
    labelKey: "filter_group_recovery",
    types: [
      "recovery_queued",
      "recovery_started",
      "recovery_paused",
      "recovery_resumed",
      "recovery_succeeded",
      "recovery_exhausted",
      "recovery_cancelled",
      "recovery_failed",
    ],
  },
  {
    labelKey: "filter_group_system",
    types: [
      "audit_logs_cleared",
      "setting_changed",
      "authorization_granted",
      "result_exported",
      "cache_cleared",
    ],
  },
] as const

export default function ReportPage() {
  const { t } = useTranslation()
  const [events, setEvents] = useState<AuditEvent[]>([])
  const [loading, setLoading] = useState(true)
  const [filter, setFilter] = useState<AuditEventType | "all">("all")
  const [isFilterOpen, setIsFilterOpen] = useState(false)
  const filterPanelRef = useRef<HTMLDivElement | null>(null)

  const loadEvents = useCallback(async () => {
    setLoading(true)
    try {
      const data = await getAuditEvents(200)
      setEvents(data)
    } catch (error) {
      console.error("Failed to load audit events:", error)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadEvents()
  }, [loadEvents])

  useEffect(() => {
    const handlePointerDown = (event: MouseEvent) => {
      if (!filterPanelRef.current?.contains(event.target as Node)) {
        setIsFilterOpen(false)
      }
    }

    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setIsFilterOpen(false)
      }
    }

    document.addEventListener("mousedown", handlePointerDown)
    document.addEventListener("keydown", handleEscape)

    return () => {
      document.removeEventListener("mousedown", handlePointerDown)
      document.removeEventListener("keydown", handleEscape)
    }
  }, [])

  const filteredEvents =
    filter === "all"
      ? events
      : events.filter((event) => event.event_type === filter)
  const currentFilterLabel =
    filter === "all" ? t("filter_all") : t(`event_${filter}`)
  const selectFilter = (nextFilter: AuditEventType | "all") => {
    setFilter(nextFilter)
    setIsFilterOpen(false)
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
      <div className="mx-auto max-w-[1180px]">
        <div className="flex flex-col gap-4 border-b border-white/6 pb-5 sm:flex-row sm:items-center sm:justify-between">
          <h1 className="af-page-title">{t("reports")}</h1>

          <button
            onClick={() => void loadEvents()}
            className={`${GHOST_BUTTON_CLASS} w-fit`}
          >
            <RefreshCw className="h-4 w-4" />
            {t("refresh")}
          </button>
        </div>

        <div className="mt-6 flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
          <div className="flex items-center gap-3">
            <Filter className="h-4 w-4 text-muted-foreground" />
            <div className="relative" ref={filterPanelRef}>
              <button
                type="button"
                onClick={() => setIsFilterOpen((open) => !open)}
                className="af-input flex min-w-[220px] items-center justify-between gap-3 py-3 text-left text-sm"
                aria-haspopup="dialog"
                aria-expanded={isFilterOpen}
              >
                <span className="truncate">{currentFilterLabel}</span>
                <ChevronDown
                  className={cn(
                    "h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-300",
                    isFilterOpen && "rotate-180",
                  )}
                />
              </button>

              {isFilterOpen && (
                <div className="af-panel absolute left-0 top-[calc(100%+0.75rem)] z-30 w-[min(760px,calc(100vw-3rem))] max-w-[calc(100vw-3rem)] min-w-[320px] p-3">
                  <div className="mb-3 flex flex-wrap items-center justify-between gap-2 border-b border-white/6 px-1 pb-3">
                    <EventFilterButton
                      label={t("filter_all")}
                      active={filter === "all"}
                      onClick={() => selectFilter("all")}
                    />
                    <span className="text-sm text-muted-foreground">
                      {t("filter_by_type")}
                    </span>
                  </div>

                  <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
                    {EVENT_FILTER_GROUPS.map((group) => (
                      <section key={group.labelKey} className="af-panel-soft rounded-[18px] p-3">
                        <div className="af-kicker mb-3 text-[12px]">
                          {t(group.labelKey)}
                        </div>
                        <div className="grid gap-1.5">
                          {group.types.map((type) => (
                            <EventFilterButton
                              key={type}
                              label={t(`event_${type}`)}
                              active={filter === type}
                              onClick={() => selectFilter(type)}
                            />
                          ))}
                        </div>
                      </section>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>

          <span className="text-sm text-muted-foreground">
            {t("total_events", { count: filteredEvents.length })}
          </span>
        </div>

        {filteredEvents.length === 0 ? (
          <div className="af-panel mt-6 px-6 py-12 text-center">
            <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-2xl bg-secondary text-muted-foreground">
              <FileText className="h-6 w-6" />
            </div>
            <p className="mt-4 text-sm text-muted-foreground">{t("no_audit_events")}</p>
          </div>
        ) : (
          <div className="af-panel mt-6 overflow-hidden">
            <div className="hidden grid-cols-[180px_minmax(0,1fr)_170px_160px] gap-4 border-b border-white/6 px-5 py-3 text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground lg:grid">
              <span>{t("event_type")}</span>
              <span>{t("event_description")}</span>
              <span>{t("related_task")}</span>
              <span>{t("event_time")}</span>
            </div>

            <div>
              {filteredEvents.map((event) => (
                <div
                  key={event.id}
                  className="grid gap-3 border-b border-white/5 px-5 py-4 transition-colors last:border-b-0 hover:bg-white/[0.02] lg:grid-cols-[180px_minmax(0,1fr)_170px_160px] lg:gap-4"
                >
                  <div className="flex items-start">
                    <span className={AUDIT_EVENT_BADGE_CLASSES[event.event_type]}>
                      {t(`event_${event.event_type}`)}
                    </span>
                  </div>

                  <div className="text-sm leading-6 text-muted-foreground">
                    {event.description}
                  </div>

                  <div>
                    {event.task_id ? (
                      <Link
                        to={`/tasks/${event.task_id}`}
                        className="inline-flex items-center gap-1 text-xs text-primary transition-colors hover:text-primary/80"
                      >
                        {event.task_id.slice(0, 8)}
                        <ArrowRight className="h-3 w-3" />
                      </Link>
                    ) : (
                      <span className="text-xs text-muted-foreground">-</span>
                    )}
                  </div>

                  <div className="font-mono text-xs text-muted-foreground">
                    {formatDateTime(event.timestamp)}
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

function EventFilterButton({
  label,
  active,
  onClick,
}: {
  label: string
  active: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex min-h-[40px] items-center rounded-[12px] border px-3 py-2 text-left text-sm transition-colors",
        active
          ? "af-filter-chip-active"
          : "border-transparent text-muted-foreground hover:border-white/6 hover:bg-accent/55 hover:text-foreground",
      )}
    >
      {label}
    </button>
  )
}
