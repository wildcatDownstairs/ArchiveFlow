use chrono::Utc;
use uuid::Uuid;

use crate::db::Database;
use crate::domain::audit::{AuditEvent, AuditEventType};
use crate::errors::AppError;

/// 记录审计事件的辅助函数
pub fn log_audit_event(
    db: &Database,
    event_type: AuditEventType,
    task_id: Option<String>,
    description: String,
) -> Result<(), AppError> {
    let event = AuditEvent {
        id: Uuid::new_v4().to_string(),
        event_type,
        task_id,
        description,
        timestamp: Utc::now(),
    };
    db.insert_audit_event(&event)?;
    log::info!("审计事件已记录: {} ({})", event.description, event.id);
    Ok(())
}
