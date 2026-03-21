import type {
  RecoveryCheckpoint,
  RecoveryProgress,
  ScheduledRecovery,
  Task,
} from "@/types"

export function describeObservedMode(
  scheduledRecovery: ScheduledRecovery | null,
  checkpoint: RecoveryCheckpoint | null,
): string | null {
  const mode = scheduledRecovery?.mode ?? checkpoint?.mode
  if (!mode) return null

  switch (mode.type) {
    case "dictionary":
      return `字典攻击 · ${mode.wordlist.length} 个候选`
    case "brute_force":
      return `暴力破解 · 字符集 ${mode.charset.length} · 长度 ${mode.min_length}-${mode.max_length}`
    case "mask":
      return `掩码攻击 · ${mode.mask}`
    default:
      return null
  }
}

export function estimateEtaSeconds(progress: RecoveryProgress | null): number | null {
  if (!progress || progress.status !== "running") return null
  if (progress.speed <= 0 || progress.total <= progress.tried) return null
  return (progress.total - progress.tried) / progress.speed
}

export function getRecoveryStageKey(
  task: Task,
  progress: RecoveryProgress | null,
  scheduledRecovery: ScheduledRecovery | null,
): string {
  if (progress?.status === "found" || task.status === "succeeded") {
    return "stage_found"
  }
  if (progress?.status === "exhausted" || task.status === "exhausted") {
    return "stage_exhausted"
  }
  if (progress?.status === "cancelled" || task.status === "cancelled") {
    return scheduledRecovery?.state === "paused" ? "stage_paused" : "stage_cancelled"
  }
  if (progress?.status === "error" || task.status === "failed") {
    return "stage_failed"
  }
  if (task.status === "interrupted") {
    return "stage_interrupted"
  }
  if (scheduledRecovery?.state === "paused") {
    return "stage_paused"
  }
  if (scheduledRecovery?.state === "queued") {
    return "stage_queued"
  }
  if (progress?.status === "running" || scheduledRecovery?.state === "running" || task.status === "processing") {
    return "stage_running"
  }
  return "stage_ready"
}
