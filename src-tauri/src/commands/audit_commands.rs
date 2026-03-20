// ============================================================
// audit_commands.rs — 审计日志相关的 Tauri 命令
//
// 审计日志（Audit Log）记录了应用中的关键操作，例如文件导入、
// 密码恢复成功/失败、设置变更等。这些记录存储在 SQLite 数据库中，
// 用于追踪历史操作、合规审查或调试。
//
// 本文件提供四个命令：
//   1. get_audit_events        — 获取最近 N 条全局审计事件
//   2. get_task_audit_events   — 获取某个任务的全部审计事件
//   3. clear_audit_events      — 清除所有审计事件（保留一条"已清除"记录）
//   4. record_setting_change   — 记录一次设置变更
// ============================================================

use tauri::{command, State};

use crate::db::Database;
use crate::domain::audit::{AuditEvent, AuditEventType};
use crate::errors::AppError;
use chrono::Utc;
use uuid::Uuid;

/// 内部辅助函数：生成"设置变更"事件的描述文本。
///
/// 【为什么提取成独立函数？】
///   这使得描述格式可以在测试中独立验证，而不需要搭建完整的数据库环境。
///   参见文件末尾的 `#[cfg(test)]` 测试块。
///
/// 【Rust 概念：Option<&str>】
///   `old_value: Option<&str>` 表示"可能有旧值，也可能没有"。
///   使用 `&str`（字符串切片引用）而非 `String`（堆分配），
///   避免不必要的内存分配，因为这里只需要读取内容，不需要所有权。
///   `.unwrap_or("未设置")` 在 None 时提供默认文字。
fn format_setting_change_description(
    setting_key: &str,
    old_value: Option<&str>,
    new_value: &str,
) -> String {
    let before = old_value.unwrap_or("未设置");
    format!("设置变更: {} 从 {} -> {}", setting_key, before, new_value)
}

/// 获取审计事件列表
///
/// `limit` 为 None 时默认取最近 100 条。
///
/// 【Rust 概念：Option<usize> 参数】
///   前端可以不传 limit（JavaScript 中省略该字段），
///   Tauri 会将其反序列化为 `None`；传了则为 `Some(n)`。
///   `limit.unwrap_or(100)` 提供默认值，使 API 更友好。
#[command]
pub async fn get_audit_events(
    limit: Option<usize>,
    db: State<'_, Database>,
) -> Result<Vec<AuditEvent>, AppError> {
    let limit = limit.unwrap_or(100);
    db.get_audit_events(limit)
}

/// 获取指定任务的审计事件
///
/// 返回与某个 task_id 关联的所有审计事件，按时间倒序排列。
/// 前端在展示任务详情时调用此命令，以显示操作历史时间线。
#[command]
pub async fn get_task_audit_events(
    task_id: String,
    db: State<'_, Database>,
) -> Result<Vec<AuditEvent>, AppError> {
    db.get_audit_events_for_task(&task_id)
}

/// 清除所有审计事件
///
/// 【设计说明：为什么清除后要保留一条记录？】
///   彻底清空日志会导致"无法知道日志曾被清除过"的问题。
///   为此，先插入一条 AuditLogsCleared 类型的事件作为"清除标记"，
///   然后删除所有其他事件，最终只保留这一条。
///   这样审计日志始终能回答"日志是否被清除过，何时清除的"。
///
/// 【Rust 概念：Uuid::new_v4().to_string()】
///   为新建的标记事件分配一个唯一 ID，防止主键冲突。
///
/// 返回值 `u64` 是实际删除的事件数量。
#[command]
pub async fn clear_audit_events(db: State<'_, Database>) -> Result<u64, AppError> {
    let marker = AuditEvent {
        id: Uuid::new_v4().to_string(),
        event_type: AuditEventType::AuditLogsCleared,
        task_id: None, // 清除操作不关联到具体任务
        description: "清除审计日志并保留操作记录".to_string(),
        timestamp: Utc::now(),
    };
    let cleared = db.clear_audit_events_and_record(&marker)?;
    Ok(cleared)
}

/// 记录设置变更
///
/// 前端在用户修改设置（如语言、主题、并发限制等）时调用此命令，
/// 将变更写入审计日志，便于日后追踪"谁在什么时候改了什么设置"。
///
/// 【设计说明：幂等性优化】
///   如果新旧值相同，则跳过写入，避免产生无意义的审计记录。
///   `old_value.as_deref() == Some(new_value.as_str())`：
///     - `.as_deref()` 将 `Option<String>` 转为 `Option<&str>`（零拷贝）
///     - `Some(new_value.as_str())` 把 &String 转为 &str 再包一层 Some
///   两者类型相同后才能用 `==` 比较。
///
/// 【Rust 概念：为什么不直接比较 old_value == Some(&new_value)？】
///   `old_value` 是 `Option<String>`，`new_value` 是 `String`，类型不同，
///   需要先统一成 `Option<&str>` 才能进行内容比较，而不是比较堆地址。
#[command]
pub async fn record_setting_change(
    setting_key: String,
    old_value: Option<String>,
    new_value: String,
    db: State<'_, Database>,
) -> Result<(), AppError> {
    // 新旧值相同时提前返回，不写入日志
    if old_value.as_deref() == Some(new_value.as_str()) {
        return Ok(());
    }

    let event = AuditEvent {
        id: Uuid::new_v4().to_string(),
        event_type: AuditEventType::SettingChanged,
        task_id: None, // 设置变更不关联到具体任务
        description: format_setting_change_description(
            &setting_key,
            old_value.as_deref(), // Option<String> → Option<&str>，避免 clone
            &new_value,
        ),
        timestamp: Utc::now(),
    };

    db.insert_audit_event(&event)?;
    Ok(())
}

// ============================================================
// 单元测试
//
// 【Rust 概念：#[cfg(test)]】
//   `#[cfg(test)]` 表示此模块只在运行测试时编译（cargo test），
//   正式构建时不会包含这些代码，不占用二进制体积。
//
// 【Rust 概念：use super::*】
//   `super` 指向父模块（即本文件的顶层），`*` 导入所有公开项。
//   这样测试代码可以直接调用 format_setting_change_description 等私有辅助函数。
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    /// 验证有旧值时描述格式正确
    #[test]
    fn format_setting_change_description_includes_old_and_new_values() {
        assert_eq!(
            format_setting_change_description("language", Some("zh"), "en"),
            "设置变更: language 从 zh -> en"
        );
    }

    /// 验证旧值为 None 时使用"未设置"占位
    #[test]
    fn format_setting_change_description_handles_missing_old_value() {
        assert_eq!(
            format_setting_change_description("language", None, "zh"),
            "设置变更: language 从 未设置 -> zh"
        );
    }
}
