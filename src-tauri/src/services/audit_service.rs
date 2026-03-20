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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn log_audit_event_persists_to_db() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();

        log_audit_event(
            &db,
            AuditEventType::FileImported,
            Some("task-1".to_string()),
            "File imported".to_string(),
        )
        .unwrap();

        let events = db.get_audit_events(100).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].task_id.as_deref(), Some("task-1"));
        assert_eq!(events[0].description, "File imported");
    }

    #[test]
    fn log_audit_event_without_task_id() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();

        log_audit_event(
            &db,
            AuditEventType::AuditLogsCleared,
            None,
            "Logs cleared".to_string(),
        )
        .unwrap();

        let events = db.get_audit_events(100).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].task_id.is_none());
    }

    #[test]
    fn log_audit_event_with_task_id() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();

        let task_id = "specific-task-42".to_string();
        log_audit_event(
            &db,
            AuditEventType::TaskStarted,
            Some(task_id.clone()),
            "Task started".to_string(),
        )
        .unwrap();

        let events = db.get_audit_events_for_task(&task_id).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].task_id.as_deref(), Some("specific-task-42"));
    }
}
