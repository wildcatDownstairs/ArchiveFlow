// ============================================================
// task_commands.rs — 任务管理相关的 Tauri 命令
//
// 任务（Task）是本应用的核心实体，代表一个待恢复密码的压缩包文件。
// 本文件提供对任务的完整 CRUD 操作，以及应用级的统计和工具命令。
//
// 暴露的命令列表：
//   get_tasks           — 获取所有任务
//   create_task         — 创建新任务（含归档类型检测）
//   get_task            — 按 ID 获取单个任务
//   delete_task         — 删除任务（运行中的任务不可删除）
//   update_task_status  — 更新任务状态
//   get_app_data_dir    — 获取应用数据目录路径
//   clear_all_tasks     — 清除全部任务（有运行中任务时拒绝）
//   get_stats           — 获取任务数和审计事件数统计
//
// 【与 archive_commands.rs 的关系】
//   create_task 与 import_archive 功能相似，但 create_task 是更底层的接口，
//   不依赖文件必须存在（文件不存在时状态为 Failed）。
//   import_archive 则用于一次性导入并要求文件存在。
// ============================================================

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

/// 辅助函数：获取文件的实际大小，失败时使用前端传入的 fallback_size。
///
/// 【Rust 概念：链式方法调用（method chaining）】
///   `metadata(...).map(...).unwrap_or(...)` 是典型的函数式风格链式调用，
///   每一步处理上一步的结果，无需临时变量，代码简洁且意图清晰。
fn resolve_file_size(file_path: &str, fallback_size: u64) -> u64 {
    std::fs::metadata(file_path)
        .map(|metadata| metadata.len())
        .unwrap_or(fallback_size)
}

/// 辅助函数：生成任务状态变更的审计描述文本。
///
/// 根据是否有错误信息，生成不同格式的描述字符串。
///
/// 【Rust 概念：match 守卫（guard）】
///   `Some(error) if !error.is_empty()` 是带守卫的模式匹配：
///   先检查是 Some，再检查字符串非空。只有两个条件都满足时才匹配该分支。
///   这比嵌套 if-let 更简洁。
fn format_status_update_description(
    file_name: &str,
    previous_status: &TaskStatus,
    next_status: &TaskStatus,
    error_message: Option<&str>,
) -> String {
    match error_message {
        Some(error) if !error.is_empty() => format!(
            "任务状态更新: {} {} -> {} (错误: {})",
            file_name,
            previous_status.as_str(),
            next_status.as_str(),
            error
        ),
        _ => format!(
            "任务状态更新: {} {} -> {}",
            file_name,
            previous_status.as_str(),
            next_status.as_str()
        ),
    }
}

/// 获取所有任务
///
/// 返回数据库中全部任务，前端主列表页面加载时调用。
#[command]
pub async fn get_tasks(db: State<'_, Database>) -> Result<Vec<Task>, AppError> {
    db.get_all_tasks()
}

/// 创建新任务
///
/// 接收文件路径、文件名和文件大小，自动检测归档类型并持久化到数据库。
/// 文件不存在时任务状态为 Failed，归档类型未知时状态取决于 for_import_result 逻辑。
///
/// 【流程说明】
///   1. 用系统 API 重新测量文件大小
///   2. 如果文件存在，调用 archive_service 检测类型和内容
///   3. 文件不存在时跳过检测，直接标记为未知类型+错误
///   4. 根据检测结果确定初始状态
///   5. 写数据库 + 写审计日志
///
/// 【Rust 概念：if-let + else block】
///   这里用 `if path.exists() { ... } else { ... }` 的三元组赋值模式，
///   两个分支都返回同类型 `(ArchiveType, Option<ArchiveInfo>, Option<String>)`，
///   Rust 要求 if-else 的两个分支类型完全一致。
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

    // 根据文件是否存在走不同的检测路径
    let (archive_type, archive_info, error_message) = if path.exists() {
        match archive_service::inspect_archive(path) {
            Ok((at, info)) => (at, info, None),
            Err(e) => {
                log::warn!("创建任务时检测归档失败 ({}): {}", file_path, e);
                (ArchiveType::Unknown, None, Some(e))
            }
        }
    } else {
        // 文件不存在：任务仍然创建，但状态为 Failed，错误消息说明原因
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

    // 写入基础导入审计事件
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

    // 对特殊状态额外写一条专项审计事件
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
///
/// 返回 `Option<Task>`：找到返回 `Some(task)`，找不到返回 `None`。
/// Tauri 会将 None 序列化为 JSON 的 `null`。
#[command]
pub async fn get_task(task_id: String, db: State<'_, Database>) -> Result<Option<Task>, AppError> {
    db.get_task_by_id(&task_id)
}

/// 删除任务
///
/// 【安全检查】
///   正在运行恢复的任务不允许删除，否则会导致后台 worker 线程在访问
///   已不存在的任务数据时出现不一致状态。
///
/// 【Rust 概念：State<'_, RecoveryManager>】
///   Tauri 允许在同一个命令函数中注入多个状态参数，
///   每个 `State<'_, T>` 对应 lib.rs 中 `.manage(T)` 注册的一个全局状态。
///   这里同时使用 db 和 recovery_manager 两个托管状态。
///
/// 【Rust 概念：.ok_or_else(|| ...)】
///   把 `Option<Task>` 转为 `Result<Task, AppError>`：
///   `Some(task)` → `Ok(task)`，`None` → `Err(AppError::TaskNotFound(...))`。
///   `ok_or_else` 用闭包惰性构造错误值，避免 None 时的无谓分配。
#[command]
pub async fn delete_task(
    task_id: String,
    db: State<'_, Database>,
    recovery_manager: State<'_, RecoveryManager>,
) -> Result<(), AppError> {
    // 防止删除正在运行恢复的任务
    if recovery_manager.is_running(&task_id) {
        return Err(AppError::InvalidArgument(format!(
            "任务正在恢复中，无法删除: {}",
            task_id
        )));
    }

    // 先查出任务（用于审计日志中的文件名），再删除
    let task = db
        .get_task_by_id(&task_id)?
        .ok_or_else(|| AppError::TaskNotFound(task_id.clone()))?;
    db.delete_task(&task_id)?;

    let _ = audit_service::log_audit_event(
        &db,
        AuditEventType::TaskDeleted,
        Some(task_id.clone()),
        format!(
            "删除任务: {} (状态: {})",
            task.file_name,
            task.status.as_str()
        ),
    );

    log::info!("任务已删除: {}", task_id);
    Ok(())
}

/// 更新任务状态
///
/// 前端在某些场景（如手动重置、标记取消）下需要直接修改任务状态。
///
/// 【Rust 概念：Option<String> 参数】
///   `error_message: Option<String>` 允许前端不传错误信息（置为 null/undefined）。
///   Tauri 将 JSON null 反序列化为 None，将字符串反序列化为 Some(String)。
///
/// 【幂等性优化】
///   如果状态和错误信息都没有变化，则跳过审计日志写入，
///   避免产生无意义的审计记录（例如前端重复调用同一状态）。
#[command]
pub async fn update_task_status(
    task_id: String,
    status: String,
    error_message: Option<String>,
    db: State<'_, Database>,
) -> Result<(), AppError> {
    // 先验证 status 字符串是合法的 TaskStatus 枚举值
    let next_status = TaskStatus::parse_canonical(&status)
        .ok_or_else(|| AppError::InvalidArgument(format!("无效的状态值: {}", status)))?;

    // 查出当前任务的现有状态，用于对比和审计
    let task = db
        .get_task_by_id(&task_id)?
        .ok_or_else(|| AppError::TaskNotFound(task_id.clone()))?;
    let previous_status = task.status.clone();
    let previous_error = task.error_message.clone();

    db.update_task_status(&task_id, &status, error_message.as_deref())?;

    // 只有状态或错误信息真正改变时才写审计日志
    if previous_status != next_status || previous_error.as_deref() != error_message.as_deref() {
        let _ = audit_service::log_audit_event(
            &db,
            AuditEventType::TaskStatusUpdated,
            Some(task_id.clone()),
            format_status_update_description(
                &task.file_name,
                &previous_status,
                &next_status,
                error_message.as_deref(),
            ),
        );
    }
    log::info!("任务状态已更新: {} -> {}", task_id, status);
    Ok(())
}

/// 获取应用数据目录
///
/// 返回 Tauri 分配的应用专属数据目录路径（如 AppData/Roaming/ArchiveFlow）。
/// 前端可以用这个路径显示数据库文件位置，或引导用户打开文件夹。
///
/// 【Rust 概念：AppHandle】
///   `AppHandle` 是 Tauri 应用实例的句柄（handle），相当于对应用的弱引用。
///   通过 `.path()` 可以访问各类平台标准目录（数据目录、缓存目录、配置目录等）。
///
/// 【Rust 概念：to_string_lossy().to_string()】
///   `to_string_lossy()` 将 `Path` 转为 `Cow<str>`（可能借用或拥有），
///   对于无效 UTF-8 的路径（某些 Windows 路径可能出现），用 U+FFFD 替换。
///   `.to_string()` 最终产出拥有所有权的 `String`。
#[command]
pub async fn get_app_data_dir(app_handle: AppHandle) -> Result<String, AppError> {
    let dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileError(format!("无法获取数据目录: {}", e)))?;
    Ok(dir.to_string_lossy().to_string())
}

/// 清除所有任务
///
/// 一次性删除数据库中的所有任务记录（包括关联的审计事件）。
///
/// 【安全检查】
///   如果有任何任务正在运行恢复，则拒绝清除操作。
///   `recovery_manager.has_running_tasks()` 原子地检查是否有活跃 worker。
///
/// 返回值 `u64` 是实际删除的任务数量。
#[command]
pub async fn clear_all_tasks(
    db: State<'_, Database>,
    recovery_manager: State<'_, RecoveryManager>,
) -> Result<u64, AppError> {
    // 有运行中任务时拒绝清除，防止孤儿 worker 访问已删除的任务
    if recovery_manager.has_running_tasks() {
        return Err(AppError::InvalidArgument(
            "存在运行中的恢复任务，请先取消后再清除全部任务".to_string(),
        ));
    }

    let cleared = db.clear_all_tasks()?;
    // 清除后写一条汇总审计日志
    let _ = audit_service::log_audit_event(
        &db,
        AuditEventType::TasksCleared,
        None, // 不关联到特定任务
        format!("批量清空任务: {} 条", cleared),
    );
    Ok(cleared)
}

/// 获取统计信息（任务总数、审计事件总数）
///
/// 前端在状态栏或统计面板显示这两个数字。
///
/// 【Rust 概念：元组返回值 (u64, u64)】
///   Rust 函数可以通过元组（tuple）返回多个值，无需专门定义结构体。
///   Tauri 会将元组序列化为 JSON 数组 `[task_count, audit_count]`。
#[command]
pub async fn get_stats(db: State<'_, Database>) -> Result<(u64, u64), AppError> {
    let task_count = db.get_task_count()?;
    let audit_count = db.get_audit_event_count()?;
    Ok((task_count, audit_count))
}

// ============================================================
// 单元测试
// ============================================================
#[cfg(test)]
mod tests {
    use super::format_status_update_description;
    use crate::domain::task::TaskStatus;

    /// 有错误信息时，描述应包含错误内容
    #[test]
    fn format_status_update_description_includes_error_when_present() {
        let description = format_status_update_description(
            "demo.zip",
            &TaskStatus::Ready,
            &TaskStatus::Failed,
            Some("CRC mismatch"),
        );

        assert!(description.contains("demo.zip"));
        assert!(description.contains("ready -> failed"));
        assert!(description.contains("CRC mismatch"));
    }

    /// 无错误信息时，描述只包含文件名和状态转换
    #[test]
    fn format_status_update_description_omits_error_when_absent() {
        let description = format_status_update_description(
            "demo.zip",
            &TaskStatus::Ready,
            &TaskStatus::Processing,
            None,
        );

        assert_eq!(description, "任务状态更新: demo.zip ready -> processing");
    }
}
