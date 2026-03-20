use tauri::{command, State};

use crate::db::Database;
use crate::domain::audit::{AuditEvent, AuditEventType};
use crate::errors::AppError;
use chrono::Utc;
use uuid::Uuid;

fn format_setting_change_description(
    setting_key: &str,
    old_value: Option<&str>,
    new_value: &str,
) -> String {
    let before = old_value.unwrap_or("未设置");
    format!("设置变更: {} 从 {} -> {}", setting_key, before, new_value)
}

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
    let marker = AuditEvent {
        id: Uuid::new_v4().to_string(),
        event_type: AuditEventType::AuditLogsCleared,
        task_id: None,
        description: "清除审计日志并保留操作记录".to_string(),
        timestamp: Utc::now(),
    };
    let cleared = db.clear_audit_events_and_record(&marker)?;
    Ok(cleared)
}

/// 记录设置变更
#[command]
pub async fn record_setting_change(
    setting_key: String,
    old_value: Option<String>,
    new_value: String,
    db: State<'_, Database>,
) -> Result<(), AppError> {
    if old_value.as_deref() == Some(new_value.as_str()) {
        return Ok(());
    }

    let event = AuditEvent {
        id: Uuid::new_v4().to_string(),
        event_type: AuditEventType::SettingChanged,
        task_id: None,
        description: format_setting_change_description(
            &setting_key,
            old_value.as_deref(),
            &new_value,
        ),
        timestamp: Utc::now(),
    };

    db.insert_audit_event(&event)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_setting_change_description_includes_old_and_new_values() {
        assert_eq!(
            format_setting_change_description("language", Some("zh"), "en"),
            "设置变更: language 从 zh -> en"
        );
    }

    #[test]
    fn format_setting_change_description_handles_missing_old_value() {
        assert_eq!(
            format_setting_change_description("language", None, "zh"),
            "设置变更: language 从 未设置 -> zh"
        );
    }
}
