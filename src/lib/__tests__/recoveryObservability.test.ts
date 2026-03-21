import { describe, expect, it } from "vitest"
import {
  describeObservedMode,
  estimateEtaSeconds,
  getRecoveryStageKey,
} from "@/lib/recoveryObservability"
import type { RecoveryCheckpoint, RecoveryProgress, ScheduledRecovery, Task } from "@/types"
import type { TFunction } from "i18next"

const mockT: TFunction = ((key: string) => key) as unknown as TFunction

const baseTask: Task = {
  id: "task-1",
  file_path: "/tmp/demo.zip",
  file_name: "demo.zip",
  file_size: 100,
  archive_type: "zip",
  status: "ready",
  created_at: "2026-03-21T00:00:00Z",
  updated_at: "2026-03-21T00:00:00Z",
  error_message: null,
  found_password: null,
  archive_info: null,
}

describe("recoveryObservability", () => {
  it("describes the scheduled or checkpoint mode", () => {
    const checkpoint: RecoveryCheckpoint = {
      task_id: "task-1",
      mode: { type: "mask", mask: "?d?d?d?d" },
      archive_type: "zip",
      priority: 3,
      tried: 100,
      total: 10_000,
      updated_at: "2026-03-21T00:00:00Z",
    }

    expect(describeObservedMode(null, checkpoint, mockT)).toBe("mask_attack · ?d?d?d?d")
  })

  it("estimates eta only for running progress with positive speed", () => {
    const progress: RecoveryProgress = {
      task_id: "task-1",
      tried: 25,
      total: 125,
      speed: 10,
      status: "running",
      found_password: null,
      elapsed_seconds: 2.5,
      worker_count: 4,
      last_checkpoint_at: "2026-03-21T00:00:00Z",
    }

    expect(estimateEtaSeconds(progress)).toBe(10)
    expect(estimateEtaSeconds({ ...progress, speed: 0 })).toBeNull()
    expect(estimateEtaSeconds({ ...progress, status: "found" })).toBeNull()
  })

  it("prefers queued and terminal states when deriving the stage key", () => {
    const queued: ScheduledRecovery = {
      task_id: "task-1",
      mode: { type: "dictionary", wordlist: ["secret"] },
      priority: 0,
      state: "queued",
      requested_at: "2026-03-21T00:00:00Z",
      started_at: null,
    }

    expect(getRecoveryStageKey(baseTask, null, queued)).toBe("stage_queued")
    expect(
      getRecoveryStageKey(
        { ...baseTask, status: "succeeded", found_password: "secret" },
        null,
        queued,
      ),
    ).toBe("stage_found")
  })
})
