// ============================================================
// archive_commands.rs — 压缩包相关的 Tauri 命令
//
// 这个文件只暴露两个命令给前端 JS 调用：
//   1. inspect_archive  — 单独检测一个压缩包（不创建任务）
//   2. import_archive   — 导入压缩包并在数据库中创建任务
//
// 【Rust 概念：#[command] 宏】
//   `#[command]` 是 Tauri 提供的属性宏，它会自动把函数包装成
//   可以从前端 JavaScript 通过 invoke() 调用的处理器。
//   对应 lib.rs 中 invoke_handler!(tauri_generate_handler![...]) 的注册。
// ============================================================

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

/// 辅助函数：获取文件的实际大小，如果读取失败则退回到前端传来的 fallback_size。
///
/// 【Rust 概念：std::fs::metadata】
///   `std::fs::metadata(path)` 返回 `Result<Metadata>`，包含文件大小、权限等信息。
///   `.map(|metadata| metadata.len())` 在 Ok 时提取字节数，Err 时保持 Err。
///   `.unwrap_or(fallback_size)` 在 Err 时使用回退值，避免 panic。
///
/// 【为什么需要这个函数？】
///   前端在拖放文件时已经知道大小，但用系统 API 重新测量更可靠（跨平台一致性）。
fn resolve_file_size(file_path: &str, fallback_size: u64) -> u64 {
    std::fs::metadata(file_path)
        .map(|metadata| metadata.len())
        .unwrap_or(fallback_size)
}

/// 检查压缩包文件信息（独立调用，不修改任务）
///
/// 前端可以在导入前先调用此命令，预览压缩包内容（条目数、是否加密等）。
///
/// 【Rust 概念：async fn + Result<T, E>】
///   Tauri 命令支持 async fn，内部会在 Tokio 运行时执行。
///   返回 `Result<ArchiveInfo, AppError>`：Ok 时 Tauri 自动序列化成 JSON 发回前端，
///   Err 时序列化成错误对象。
///
/// 【Rust 概念：?  操作符 vs .map_err()】
///   `.map_err(AppError::ArchiveError)?`：先把 ArchiveService 的 String 错误
///   包装成 AppError::ArchiveError，然后 `?` 在出错时提前 return Err(...)。
///   `.ok_or_else(|| ...)` 把 Option<T> 转为 Result<T, E>，None 变成 Err。
#[command]
pub async fn inspect_archive(
    file_path: String,
) -> Result<crate::domain::archive::ArchiveInfo, AppError> {
    let path = Path::new(&file_path);

    // 文件不存在时提前返回，避免后续 IO 操作出现更难理解的错误
    if !path.exists() {
        return Err(AppError::FileError(format!("文件不存在: {}", file_path)));
    }

    // inspect_archive 返回 (ArchiveType, Option<ArchiveInfo>)
    // `_archive_type` 前缀的下划线告诉编译器：这个变量刻意不使用，无需警告
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
///
/// 【流程说明】
///   1. 生成 UUID 作为任务 ID
///   2. 用系统 API 重新测量文件大小（更可靠）
///   3. 调用 archive_service 检测压缩包类型和内容
///   4. 根据检测结果决定任务初始状态（Ready / Failed / Unsupported）
///   5. 写入数据库
///   6. 写入审计日志（不影响主流程，失败时静默忽略 `let _ = ...`）
///
/// 【Rust 概念：State<'_, Database>】
///   Tauri 的托管状态（managed state）。`'_` 是生命周期省略符，
///   表示 `db` 的借用与函数调用期间存活。Database 被存储在 Tauri 的全局状态中，
///   这里只是借用引用，不获得所有权。
///
/// 【Rust 概念：Uuid::new_v4().to_string()】
///   生成随机 v4 UUID（如 "550e8400-e29b-41d4-a716-446655440000"），
///   作为任务的全局唯一标识符，存入数据库。
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

    // 用系统 API 重新获取文件大小，前端传入的 file_size 仅作备用
    let resolved_file_size = resolve_file_size(&file_path, file_size);

    // 解构 match 的三元组：(类型, 内容信息, 错误消息)
    // 检测失败时不中断流程，而是记录错误并使用 Unknown 类型继续
    let (archive_type, archive_info, error_message) = match archive_service::inspect_archive(path) {
        Ok((at, info)) => (at, info, None),
        Err(e) => {
            log::warn!("归档检测失败 ({}): {}", file_path, e);
            // 检测失败时仍然创建任务，状态会被设为 Failed
            (ArchiveType::Unknown, None, Some(e))
        }
    };

    // 根据归档类型、是否有内容信息、是否有错误，推断任务的初始状态
    // 逻辑封装在 TaskStatus::for_import_result 中，避免这里出现复杂的 if-else 嵌套
    let status = TaskStatus::for_import_result(
        &archive_type,
        archive_info.is_some(),
        error_message.as_deref(), // Option<String> → Option<&str>，不克隆字符串
    );

    // 【Rust 概念：结构体初始化语法】
    // 所有字段都必须显式赋值（无默认值机制），编译器会在遗漏字段时报错
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

    // 写入数据库，? 在出错时提前返回
    db.insert_task(&task)?;

    // 【Rust 概念：let _ = ...】
    // 审计日志写入失败不应影响主操作的成功返回，所以用 `let _ =` 丢弃 Result。
    // 如果直接用 `?`，审计失败会导致整个命令返回错误，这不是我们想要的行为。
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

    // 对特殊状态额外记录一条专项审计事件，方便前端过滤
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
                // 有错误消息用错误消息，没有则用文件名兜底
                // clone() 是因为 task.error_message 是 Option<String>，
                // 而 task 在本函数末尾会被 move 到 Ok(task) 中
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

    // task 的所有权在这里 move 进 Ok(...)，调用方获得 Task 的完整所有权
    Ok(task)
}
