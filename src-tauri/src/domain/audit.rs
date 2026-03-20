use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 审计事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    FileImported,
    TaskCreated,
    TaskStarted,
    TaskCompleted,
    TaskFailed,
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
