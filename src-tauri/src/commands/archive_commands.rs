use tauri::{command, State};
use crate::db::Database;
use crate::domain::task::{Task, TaskStatus, ArchiveType};
use crate::errors::AppError;
use crate::services::archive_service;
use chrono::Utc;
use std::path::Path;
use uuid::Uuid;

/// 检查压缩包文件信息（独立调用，不修改任务）
#[command]
pub async fn inspect_archive(file_path: String) -> Result<crate::domain::archive::ArchiveInfo, AppError> {
    let path = Path::new(&file_path);

    if !path.exists() {
        return Err(AppError::FileError(format!("文件不存在: {}", file_path)));
    }

    let (_archive_type, info) = archive_service::inspect_archive(path)
        .map_err(|e| AppError::ArchiveError(e))?;

    log::info!(
        "检查完成: {} (条目数: {}, 加密: {})",
        file_path,
        info.total_entries,
        info.is_encrypted
    );

    Ok(info)
}

/// 导入压缩包：创建任务 + 检测归档类型和内容，一站式操作
#[command]
pub async fn import_archive(
    file_path: String,
    file_name: String,
    file_size: u64,
    db: State<'_, Database>,
) -> Result<Task, AppError> {
    let now = Utc::now();
    let task_id = Uuid::new_v4().to_string();

    // 检测归档类型和内容
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(AppError::FileError(format!("文件不存在: {}", file_path)));
    }

    let (archive_type, archive_info) = match archive_service::inspect_archive(path) {
        Ok((at, info)) => (at, Some(info)),
        Err(e) => {
            log::warn!("归档检测失败 ({}): {}", file_path, e);
            // 检测失败不阻止导入，使用 Unknown 类型
            (ArchiveType::Unknown, None)
        }
    };

    // 根据检测结果确定初始状态
    let status = if archive_info.is_some() {
        TaskStatus::Ready
    } else {
        TaskStatus::Imported
    };

    let task = Task {
        id: task_id,
        file_path,
        file_name,
        file_size,
        archive_type,
        status,
        created_at: now,
        updated_at: now,
        error_message: None,
        archive_info,
    };

    db.insert_task(&task)?;

    log::info!(
        "归档已导入: {} ({}) 类型={:?} 状态={:?}",
        task.file_name,
        task.id,
        task.archive_type,
        task.status
    );

    Ok(task)
}
