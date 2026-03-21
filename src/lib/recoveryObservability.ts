/**
 * @fileoverview 文件功能：提供 recoveryObservability 基础库和工具函数
 * @author ArchiveFlow Team
 * @created 2026-03-21
 * @modified 2026-03-21
 * @dependencies i18next
 */

import type {
  RecoveryCheckpoint,
  RecoveryProgress,
  ScheduledRecovery,
  Task,
} from "@/types"
import type { TFunction } from "i18next"

/**
 *
 * @param scheduledRecovery
 * @param checkpoint
 * @param t
  * @returns {any} 执行结果
 */
export function describeObservedMode(
  scheduledRecovery: ScheduledRecovery | null,
  checkpoint: RecoveryCheckpoint | null,
  t: TFunction,
): string | null {
  const mode = scheduledRecovery?.mode ?? checkpoint?.mode
  if (!mode) return null

  switch (mode.type) {
    case "dictionary":
      return `${t("dictionary_attack")} · ${mode.wordlist.length}`
    case "brute_force":
      return `${t("bruteforce_attack")} · ${t("charset")} ${mode.charset.length} · ${mode.min_length}-${mode.max_length}`
    case "mask":
      return `${t("mask_attack")} · ${mode.mask}`
    default:
      return null
  }
}

/**
 *
 * @param progress
  * @returns {any} 执行结果
 */
export function estimateEtaSeconds(progress: RecoveryProgress | null): number | null {
  if (!progress || progress.status !== "running") return null
  if (progress.speed <= 0 || progress.total <= progress.tried) return null
  return (progress.total - progress.tried) / progress.speed
}

/**
 *
 * @param task
 * @param progress
 * @param scheduledRecovery
  * @returns {any} 执行结果
 */
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
