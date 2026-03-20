use tauri::{command, AppHandle, Manager, State};

use crate::db::Database;
use crate::domain::audit::AuditEventType;
use crate::domain::recovery::RecoveryManager;
use crate::domain::task::{ArchiveType, Task, TaskStatus};
use crate::errors::AppError;
use crate::services::archive_service;
use crate::services::audit_service;
use chrono::Utc;
use std::path::Path;
use uuid::Uuid;

fn resolve_file_size(file_path: &str, fallback_size: u64) -> u64 {
    std::fs::metadata(file_path)
        .map(|metadata| metadata.len())
        .unwrap_or(fallback_size)
}

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
    let resolved_file_size = resolve_file_size(&file_path, file_size);
    let path = Path::new(&file_path);

    let (archive_type, archive_info, error_message) = if path.exists() {
        match archive_service::inspect_archive(path) {
            Ok((at, info)) => (at, info, None),
            Err(e) => {
                log::warn!("创建任务时检测归档失败 ({}): {}", file_path, e);
                (ArchiveType::Unknown, None, Some(e))
            }
        }
    } else {
        (
            ArchiveType::Unknown,
            None,
            Some(format!("文件不存在: {}", file_path)),
        )
    };

    let status = TaskStatus::for_import_result(
        &archive_type,
        archive_info.is_some(),
        error_message.as_deref(),
    );

    let task = Task {
        id: Uuid::new_v4().to_string(),
        file_path,
        file_name,
        file_size: resolved_file_size,
        archive_type,
        status,
        created_at: now,
        updated_at: now,
        error_message,
        found_password: None,
        archive_info,
    };
    db.insert_task(&task)?;
    let _ = audit_service::log_audit_event(
        &db,
        AuditEventType::FileImported,
        Some(task.id.clone()),
        format!(
            "创建任务: {} (状态: {})",
            task.file_name,
            task.status.as_str()
        ),
    );
    if task.status == TaskStatus::Unsupported {
        let _ = audit_service::log_audit_event(
            &db,
            AuditEventType::TaskUnsupported,
            Some(task.id.clone()),
            format!("任务不受支持: {} ({:?})", task.file_name, task.archive_type),
        );
    } else if task.status == TaskStatus::Failed {
        let _ = audit_service::log_audit_event(
            &db,
            AuditEventType::TaskFailed,
            Some(task.id.clone()),
            format!(
                "任务创建失败: {}",
                task.error_message
                    .clone()
                    .unwrap_or_else(|| task.file_name.clone())
            ),
        );
    }
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
    TaskStatus::parse_canonical(&status)
        .ok_or_else(|| AppError::InvalidArgument(format!("无效的状态值: {}", status)))?;

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
