import type { AuditEventType, Task } from "@/types"

const BASE_BADGE_CLASS =
  "inline-flex items-center justify-center rounded-full px-2.5 py-1 text-sm font-medium whitespace-nowrap"

export const TASK_TYPE_BADGE_CLASSES: Record<Task["archive_type"], string> = {
  zip: `${BASE_BADGE_CLASS} bg-primary/15 text-primary`,
  sevenz: `${BASE_BADGE_CLASS} bg-emerald-400/15 text-emerald-300`,
  rar: `${BASE_BADGE_CLASS} bg-amber-400/15 text-amber-300`,
  unknown: `${BASE_BADGE_CLASS} bg-secondary text-muted-foreground`,
}

export const TASK_STATUS_BADGE_CLASSES: Record<Task["status"], string> = {
  ready: `${BASE_BADGE_CLASS} bg-sky-400/15 text-sky-300`,
  processing: `${BASE_BADGE_CLASS} bg-indigo-400/15 text-indigo-300`,
  succeeded: `${BASE_BADGE_CLASS} bg-emerald-400/15 text-emerald-300`,
  exhausted: `${BASE_BADGE_CLASS} bg-amber-400/15 text-amber-300`,
  cancelled: `${BASE_BADGE_CLASS} bg-slate-400/15 text-slate-300`,
  failed: `${BASE_BADGE_CLASS} bg-rose-400/15 text-rose-300`,
  unsupported: `${BASE_BADGE_CLASS} bg-zinc-400/15 text-zinc-300`,
  interrupted: `${BASE_BADGE_CLASS} bg-orange-400/15 text-orange-300`,
}

export const AUDIT_EVENT_BADGE_CLASSES: Record<AuditEventType, string> = {
  file_imported: `${BASE_BADGE_CLASS} bg-sky-400/15 text-sky-300`,
  task_status_updated: `${BASE_BADGE_CLASS} bg-indigo-400/15 text-indigo-300`,
  task_deleted: `${BASE_BADGE_CLASS} bg-orange-400/15 text-orange-300`,
  tasks_cleared: `${BASE_BADGE_CLASS} bg-orange-400/15 text-orange-300`,
  task_failed: `${BASE_BADGE_CLASS} bg-rose-400/15 text-rose-300`,
  task_unsupported: `${BASE_BADGE_CLASS} bg-zinc-400/15 text-zinc-300`,
  task_interrupted: `${BASE_BADGE_CLASS} bg-orange-400/15 text-orange-300`,
  recovery_queued: `${BASE_BADGE_CLASS} bg-sky-400/15 text-sky-300`,
  recovery_started: `${BASE_BADGE_CLASS} bg-primary/15 text-primary`,
  recovery_paused: `${BASE_BADGE_CLASS} bg-amber-400/15 text-amber-300`,
  recovery_resumed: `${BASE_BADGE_CLASS} bg-cyan-400/15 text-cyan-300`,
  recovery_succeeded: `${BASE_BADGE_CLASS} bg-emerald-400/15 text-emerald-300`,
  recovery_exhausted: `${BASE_BADGE_CLASS} bg-amber-400/15 text-amber-300`,
  recovery_cancelled: `${BASE_BADGE_CLASS} bg-slate-400/15 text-slate-300`,
  recovery_failed: `${BASE_BADGE_CLASS} bg-rose-400/15 text-rose-300`,
  audit_logs_cleared: `${BASE_BADGE_CLASS} bg-zinc-400/15 text-zinc-300`,
  setting_changed: `${BASE_BADGE_CLASS} bg-cyan-400/15 text-cyan-300`,
  authorization_granted: `${BASE_BADGE_CLASS} bg-fuchsia-400/15 text-fuchsia-300`,
  result_exported: `${BASE_BADGE_CLASS} bg-teal-400/15 text-teal-300`,
  cache_cleared: `${BASE_BADGE_CLASS} bg-zinc-400/15 text-zinc-300`,
}

export const INFO_VALUE_CLASS =
  "text-right text-sm font-medium text-foreground"

export const GHOST_BUTTON_CLASS =
  "af-button-ghost px-3.5 py-2"

export const PRIMARY_BUTTON_CLASS =
  "af-button-primary px-4 py-2.5"

export const DANGER_BUTTON_CLASS =
  "af-button-danger-soft px-3.5 py-2"
