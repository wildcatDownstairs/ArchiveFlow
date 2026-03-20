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
  | "task_deleted"
  | "tasks_cleared"
  | "task_failed"
  | "task_unsupported"
  | "task_interrupted"
  | "recovery_started"
  | "recovery_succeeded"
  | "recovery_exhausted"
  | "recovery_cancelled"
  | "recovery_failed"
  | "audit_logs_cleared"
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
}
