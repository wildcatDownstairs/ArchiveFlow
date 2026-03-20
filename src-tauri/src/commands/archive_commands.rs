use tauri::{command, State};
use crate::db::Database;
use crate::domain::task::{Task, TaskStatus, ArchiveType};
use crate::errors::AppError;
use crate::services::archive_service;
use crate::services::audit_service;
use crate::domain::audit::AuditEventType;
use chrono::Utc;
use std::path::Path;
use uuid::Uuid;

fn unsupported_archive_message(archive_type: &ArchiveType) -> Option<String> {
    match archive_type {
        ArchiveType::SevenZ => {
            Some("7z 格式暂不支持内容解析和密码恢复，仅识别了文件类型".to_string())
        }
        ArchiveType::Rar => {
            Some("RAR 格式暂不支持内容解析和密码恢复，仅识别了文件类型".to_string())
        }
        _ => None,
    }
}

/// 检查压缩包文件信息（独立调用，不修改任务）
#[command]
pub async fn inspect_archive(file_path: String) -> Result<crate::domain::archive::ArchiveInfo, AppError> {
    let path = Path::new(&file_path);

    if !path.exists() {
        return Err(AppError::FileError(format!("文件不存在: {}", file_path)));
    }

    let (archive_type, info) = archive_service::inspect_archive(path)
        .map_err(|e| AppError::ArchiveError(e))?;
    let info = info.ok_or_else(|| {
        AppError::ArchiveError(
            unsupported_archive_message(&archive_type)
                .unwrap_or_else(|| "该格式暂不支持内容解析".to_string()),
        )
    })?;

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

    let (archive_type, archive_info, error_message) = match archive_service::inspect_archive(path) {
        Ok((at, info)) => {
            let error_message = unsupported_archive_message(&at);
            let archive_info = match at {
                ArchiveType::Zip => info,
                ArchiveType::SevenZ | ArchiveType::Rar | ArchiveType::Unknown => None,
            };
            (at, archive_info, error_message)
        }
        Err(e) => {
            log::warn!("归档检测失败 ({}): {}", file_path, e);
            // 检测失败不阻止导入，使用 Unknown 类型
            (ArchiveType::Unknown, None, Some(e))
        }
    };

    // 根据检测结果确定初始状态
    // 只有 ZIP 类型且成功解析了归档内容时才标记为 Ready
    // 7z/RAR 目前只能做类型检测，无法解析内容，标记为 Imported 并给出提示
    let status = if matches!((&archive_type, &archive_info), (ArchiveType::Zip, Some(_))) {
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
        error_message,
        found_password: None,
        archive_info,
    };

    db.insert_task(&task)?;

    // 记录文件导入审计事件
    let _ = audit_service::log_audit_event(
        &db,
        AuditEventType::FileImported,
        Some(task.id.clone()),
        format!("导入归档文件: {} ({:?})", task.file_name, task.archive_type),
    );

    log::info!(
        "归档已导入: {} ({}) 类型={:?} 状态={:?}",
        task.file_name,
        task.id,
        task.archive_type,
        task.status
    );

    Ok(task)
}
