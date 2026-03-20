use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 审计事件类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    FileImported,
    TaskStatusUpdated,
    TaskDeleted,
    TasksCleared,
    TaskFailed,
    TaskUnsupported,
    TaskInterrupted,
    RecoveryStarted,
    RecoverySucceeded,
    RecoveryExhausted,
    RecoveryCancelled,
    RecoveryFailed,
    AuditLogsCleared,
    SettingChanged,
    AuthorizationGranted,
    ResultExported,
    CacheCleared,
}

/// 审计事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub event_type: AuditEventType,
    pub task_id: Option<String>,
    pub description: String,
    pub timestamp: DateTime<Utc>,
}

impl AuditEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FileImported => "file_imported",
            Self::TaskStatusUpdated => "task_status_updated",
            Self::TaskDeleted => "task_deleted",
            Self::TasksCleared => "tasks_cleared",
            Self::TaskFailed => "task_failed",
            Self::TaskUnsupported => "task_unsupported",
            Self::TaskInterrupted => "task_interrupted",
            Self::RecoveryStarted => "recovery_started",
            Self::RecoverySucceeded => "recovery_succeeded",
            Self::RecoveryExhausted => "recovery_exhausted",
            Self::RecoveryCancelled => "recovery_cancelled",
            Self::RecoveryFailed => "recovery_failed",
            Self::AuditLogsCleared => "audit_logs_cleared",
            Self::SettingChanged => "setting_changed",
            Self::AuthorizationGranted => "authorization_granted",
            Self::ResultExported => "result_exported",
            Self::CacheCleared => "cache_cleared",
        }
    }

    pub fn parse_persisted(raw: &str) -> Option<Self> {
        match raw {
            "file_imported" | "task_created" => Some(Self::FileImported),
            "task_status_updated" => Some(Self::TaskStatusUpdated),
            "task_deleted" => Some(Self::TaskDeleted),
            "tasks_cleared" => Some(Self::TasksCleared),
            "task_failed" => Some(Self::RecoveryFailed),
            "task_unsupported" => Some(Self::TaskUnsupported),
            "task_interrupted" => Some(Self::TaskInterrupted),
            "task_started" | "recovery_started" => Some(Self::RecoveryStarted),
            "task_completed" | "recovery_succeeded" => Some(Self::RecoverySucceeded),
            "recovery_exhausted" => Some(Self::RecoveryExhausted),
            "recovery_cancelled" => Some(Self::RecoveryCancelled),
            "recovery_failed" => Some(Self::RecoveryFailed),
            "audit_logs_cleared" => Some(Self::AuditLogsCleared),
            "setting_changed" => Some(Self::SettingChanged),
            "authorization_granted" => Some(Self::AuthorizationGranted),
            "result_exported" => Some(Self::ResultExported),
            "cache_cleared" => Some(Self::CacheCleared),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AuditEventType;

    #[test]
    fn persisted_values_support_legacy_aliases() {
        assert_eq!(
            AuditEventType::parse_persisted("task_started"),
            Some(AuditEventType::RecoveryStarted)
        );
        assert_eq!(
            AuditEventType::parse_persisted("task_completed"),
            Some(AuditEventType::RecoverySucceeded)
        );
        assert_eq!(
            AuditEventType::parse_persisted("task_failed"),
            Some(AuditEventType::RecoveryFailed)
        );
        assert_eq!(
            AuditEventType::parse_persisted("task_created"),
            Some(AuditEventType::FileImported)
        );
        assert_eq!(
            AuditEventType::parse_persisted("task_status_updated"),
            Some(AuditEventType::TaskStatusUpdated)
        );
        assert_eq!(
            AuditEventType::parse_persisted("setting_changed"),
            Some(AuditEventType::SettingChanged)
        );
    }
}
