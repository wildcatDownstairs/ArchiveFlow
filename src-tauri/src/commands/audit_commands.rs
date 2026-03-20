use tauri::{command, State};

use crate::db::Database;
use crate::domain::audit::AuditEvent;
use crate::errors::AppError;

/// 获取审计事件列表
#[command]
pub async fn get_audit_events(
    limit: Option<usize>,
    db: State<'_, Database>,
) -> Result<Vec<AuditEvent>, AppError> {
    let limit = limit.unwrap_or(100);
    db.get_audit_events(limit)
}

/// 获取指定任务的审计事件
#[command]
pub async fn get_task_audit_events(
    task_id: String,
    db: State<'_, Database>,
) -> Result<Vec<AuditEvent>, AppError> {
    db.get_audit_events_for_task(&task_id)
}
