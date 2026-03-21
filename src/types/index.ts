export interface Task {
  id: string
  file_path: string
  file_name: string
  file_size: number
  archive_type: "zip" | "sevenz" | "rar" | "unknown"
  status:
    | "ready"
    | "processing"
    | "succeeded"
    | "exhausted"
    | "cancelled"
    | "failed"
    | "unsupported"
    | "interrupted"
  created_at: string
  updated_at: string
  error_message: string | null
  found_password: string | null
  archive_info: ArchiveInfo | null
}

export interface ArchiveEntry {
  path: string
  size: number
  compressed_size: number
  is_directory: boolean
  is_encrypted: boolean
  last_modified: string | null
}

export interface ArchiveInfo {
  total_entries: number
  total_size: number
  is_encrypted: boolean
  has_encrypted_filenames: boolean
  entries: ArchiveEntry[]
}

// --- Audit event types ---

export type AuditEventType =
  | "file_imported"
  | "task_status_updated"
  | "task_deleted"
  | "tasks_cleared"
  | "task_failed"
  | "task_unsupported"
  | "task_interrupted"
  | "recovery_queued"
  | "recovery_started"
  | "recovery_paused"
  | "recovery_resumed"
  | "recovery_succeeded"
  | "recovery_exhausted"
  | "recovery_cancelled"
  | "recovery_failed"
  | "audit_logs_cleared"
  | "setting_changed"
  | "authorization_granted"
  | "result_exported"
  | "cache_cleared"

export interface AuditEvent {
  id: string
  event_type: AuditEventType
  task_id: string | null
  description: string
  timestamp: string
}

// --- Password recovery types ---

export type RecoveryStatus =
  | "running"
  | "found"
  | "exhausted"
  | "cancelled"
  | "error"

export interface RecoveryProgress {
  task_id: string
  tried: number
  total: number
  speed: number
  status: RecoveryStatus
  found_password: string | null
  elapsed_seconds: number
  worker_count: number
  last_checkpoint_at: string | null
}

export type ExportFormat = "csv" | "json"

export interface ExportOptions {
  maskPasswords?: boolean
  includeAuditEvents?: boolean
}

export type AttackMode =
  | { type: "dictionary"; wordlist: string[] }
  | { type: "brute_force"; charset: string; min_length: number; max_length: number }
  | { type: "mask"; mask: string }

export type RecoveryBackend = "cpu" | "gpu"

export interface RecoveryCheckpoint {
  task_id: string
  mode: AttackMode
  archive_type: Task["archive_type"]
  priority: number
  tried: number
  total: number
  updated_at: string
}

export type ScheduledRecoveryState = "queued" | "running" | "paused"

export interface ScheduledRecovery {
  task_id: string
  mode: AttackMode
  priority: number
  backend: RecoveryBackend
  hashcat_path: string | null
  state: ScheduledRecoveryState
  requested_at: string
  started_at: string | null
}

export interface RecoverySchedulerSnapshot {
  max_concurrent: number
  running_count: number
  queued_count: number
  paused_count: number
  tasks: ScheduledRecovery[]
}

export interface HashcatDeviceInfo {
  id: number
  name: string
  device_type: string
}

export interface HashcatDetectionResult {
  available: boolean
  path: string | null
  version: string | null
  devices: HashcatDeviceInfo[]
  error: string | null
}
