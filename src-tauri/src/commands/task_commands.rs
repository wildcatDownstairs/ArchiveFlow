use tauri::{command, AppHandle, Manager, State};
use crate::db::Database;
use crate::domain::recovery::RecoveryManager;
use crate::domain::task::{Task, TaskStatus, ArchiveType};
use crate::errors::AppError;
use crate::services::audit_service;
use crate::domain::audit::AuditEventType;
use chrono::Utc;
use uuid::Uuid;

/// 获取所有任务
#[command]
pub async fn get_tasks(db: State<'_, Database>) -> Result<Vec<Task>, AppError> {
    db.get_all_tasks()
}

/// 创建新任务
#[command]
pub async fn create_task(
    file_path: String,
    file_name: String,
    file_size: u64,
    db: State<'_, Database>,
) -> Result<Task, AppError> {
    let now = Utc::now();
    let task = Task {
        id: Uuid::new_v4().to_string(),
        file_path,
        file_name,
        file_size,
        archive_type: ArchiveType::Unknown,
        status: TaskStatus::Imported,
        created_at: now,
        updated_at: now,
        error_message: None,
        found_password: None,
        archive_info: None,
    };
    db.insert_task(&task)?;
    let _ = audit_service::log_audit_event(
        &db,
        AuditEventType::TaskCreated,
        Some(task.id.clone()),
        format!("创建任务: {}", task.file_name),
    );
    log::info!("任务已创建: {} ({})", task.file_name, task.id);
    Ok(task)
}

/// 获取单个任务
#[command]
pub async fn get_task(task_id: String, db: State<'_, Database>) -> Result<Option<Task>, AppError> {
    db.get_task_by_id(&task_id)
}

/// 删除任务
#[command]
pub async fn delete_task(
    task_id: String,
    db: State<'_, Database>,
    recovery_manager: State<'_, RecoveryManager>,
) -> Result<(), AppError> {
    if recovery_manager.is_running(&task_id) {
        return Err(AppError::InvalidArgument(format!(
            "任务正在恢复中，无法删除: {}",
            task_id
        )));
    }

    db.delete_task(&task_id)?;

    // 记录任务删除审计事件
    let _ = audit_service::log_audit_event(
        &db,
        AuditEventType::TaskDeleted,
        Some(task_id.clone()),
        format!("删除任务: {}", task_id),
    );

    log::info!("任务已删除: {}", task_id);
    Ok(())
}

/// 更新任务状态
#[command]
pub async fn update_task_status(
    task_id: String,
    status: String,
    error_message: Option<String>,
    db: State<'_, Database>,
) -> Result<(), AppError> {
    // 校验 status 字符串是否是有效的 TaskStatus
    let _: TaskStatus = serde_json::from_value(serde_json::Value::String(status.clone()))
        .map_err(|_| AppError::InvalidArgument(format!("无效的状态值: {}", status)))?;

    db.update_task_status(&task_id, &status, error_message.as_deref())?;
    log::info!("任务状态已更新: {} -> {}", task_id, status);
    Ok(())
}

/// 获取应用数据目录
#[command]
pub async fn get_app_data_dir(app_handle: AppHandle) -> Result<String, AppError> {
    let dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileError(format!("无法获取数据目录: {}", e)))?;
    Ok(dir.to_string_lossy().to_string())
}

/// 清除所有任务
#[command]
pub async fn clear_all_tasks(
    db: State<'_, Database>,
    recovery_manager: State<'_, RecoveryManager>,
) -> Result<u64, AppError> {
    if recovery_manager.has_running_tasks() {
        return Err(AppError::InvalidArgument(
            "存在运行中的恢复任务，请先取消后再清除全部任务".to_string(),
        ));
    }

    let cleared = db.clear_all_tasks()?;
    let _ = audit_service::log_audit_event(
        &db,
        AuditEventType::TasksCleared,
        None,
        format!("清除全部任务: {} 条", cleared),
    );
    Ok(cleared)
}

/// 获取统计信息（任务数、审计事件数）
#[command]
pub async fn get_stats(db: State<'_, Database>) -> Result<(u64, u64), AppError> {
    let task_count = db.get_task_count()?;
    let audit_count = db.get_audit_event_count()?;
    Ok((task_count, audit_count))
}
