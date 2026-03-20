use tauri::{command, State};
use crate::db::Database;
use crate::domain::task::{Task, TaskStatus, ArchiveType};
use crate::errors::AppError;
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
        archive_info: None,
    };
    db.insert_task(&task)?;
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
pub async fn delete_task(task_id: String, db: State<'_, Database>) -> Result<(), AppError> {
    db.delete_task(&task_id)?;
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
