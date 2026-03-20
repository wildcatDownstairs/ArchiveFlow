use tauri::{command, State};

use crate::db::Database;
use crate::domain::audit::AuditEventType;
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

/// 检查压缩包文件信息（独立调用，不修改任务）
#[command]
pub async fn inspect_archive(
    file_path: String,
) -> Result<crate::domain::archive::ArchiveInfo, AppError> {
    let path = Path::new(&file_path);

    if !path.exists() {
        return Err(AppError::FileError(format!("文件不存在: {}", file_path)));
    }

    let (_archive_type, info) =
        archive_service::inspect_archive(path).map_err(AppError::ArchiveError)?;
    let info = info.ok_or_else(|| AppError::ArchiveError("该格式暂不支持内容解析".to_string()))?;

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

    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(AppError::FileError(format!("文件不存在: {}", file_path)));
    }

    let resolved_file_size = resolve_file_size(&file_path, file_size);

    let (archive_type, archive_info, error_message) = match archive_service::inspect_archive(path) {
        Ok((at, info)) => (at, info, None),
        Err(e) => {
            log::warn!("归档检测失败 ({}): {}", file_path, e);
            (ArchiveType::Unknown, None, Some(e))
        }
    };

    let status = TaskStatus::for_import_result(
        &archive_type,
        archive_info.is_some(),
        error_message.as_deref(),
    );

    let task = Task {
        id: task_id,
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
            "导入归档文件: {} ({:?}, 状态: {})",
            task.file_name,
            task.archive_type,
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
                "任务导入失败: {}",
                task.error_message
                    .clone()
                    .unwrap_or_else(|| task.file_name.clone())
            ),
        );
    }

    log::info!(
        "归档已导入: {} ({}) 类型={:?} 状态={:?}",
        task.file_name,
        task.id,
        task.archive_type,
        task.status
    );

    Ok(task)
}
