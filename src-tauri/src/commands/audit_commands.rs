use tauri::{command, State};

use crate::db::Database;
use crate::domain::audit::AuditEventType;
use crate::domain::audit::AuditEvent;
use crate::errors::AppError;
use crate::services::audit_service;

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

/// 清除所有审计事件
#[command]
pub async fn clear_audit_events(db: State<'_, Database>) -> Result<u64, AppError> {
    let cleared = db.clear_audit_events()?;
    let _ = audit_service::log_audit_event(
        &db,
        AuditEventType::AuditLogsCleared,
        None,
        format!("清除审计日志: {} 条", cleared),
    );
    Ok(cleared)
}
