export interface Task {
  id: string
  file_path: string
  file_name: string
  file_size: number
  archive_type: "zip" | "sevenz" | "rar" | "unknown"
  status:
    | "imported"
    | "inspecting"
    | "waiting_authorization"
    | "ready"
    | "processing"
    | "verifying"
    | "succeeded"
    | "failed"
    | "cleaned"
  created_at: string
  updated_at: string
  error_message: string | null
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

// --- 密码恢复相关类型 ---

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
