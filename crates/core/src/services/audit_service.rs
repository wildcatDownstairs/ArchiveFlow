// chrono::Utc 提供 UTC 时间戳，Uuid 用于生成全局唯一 ID
use chrono::Utc;
use uuid::Uuid;

use crate::db::Database;
use crate::domain::audit::{AuditEvent, AuditEventType};
use crate::errors::AppError;

/// 记录审计事件的辅助函数
///
/// # 参数说明
/// - `db`: 对 Database 的不可变引用（`&Database`），这里只需要读/写权限，
///   不需要获取所有权，所以用引用即可。
/// - `event_type`: 枚举值，描述事件类型（如"文件导入"、"恢复开始"等）
/// - `task_id`: `Option<String>` —— 可能有关联的任务 ID，也可能没有。
///   Rust 用 `Option<T>` 表示"可能为空"的值，而不是用 null。
/// - `description`: 事件的文字描述，调用方传入所有权（不是引用），
///   因为我们需要将它存入结构体中。
///
/// # 返回值
/// `Result<(), AppError>` —— 成功时返回空值 `()`，失败时返回自定义错误。
/// `?` 操作符会在出错时自动提前返回 Err，无需手写 match。
pub fn log_audit_event(
    db: &Database,
    event_type: AuditEventType,
    task_id: Option<String>,
    description: String,
) -> Result<(), AppError> {
    // 构造一个新的审计事件结构体
    // Uuid::new_v4() 生成一个随机的 v4 UUID，.to_string() 将其转为字符串
    // Utc::now() 获取当前 UTC 时间戳
    let event = AuditEvent {
        id: Uuid::new_v4().to_string(),
        event_type,
        task_id,
        description,
        timestamp: Utc::now(),
    };
    // 将事件持久化到数据库，`?` 在出错时传播错误
    db.insert_audit_event(&event)?;
    // log::info! 是一个宏，输出 INFO 级别的日志
    // {} 是格式化占位符，对应后面的参数（类似 println!）
    log::info!("审计事件已记录: {} ({})", event.description, event.id);
    Ok(())
}

// ─── 单元测试 ────────────────────────────────────────────────────────────────
// `#[cfg(test)]` 表示这个模块只在运行测试时编译，不会出现在生产二进制文件中。
// `mod tests { use super::*; }` 是 Rust 的惯用写法：
//   - `mod tests` 声明一个内联子模块
//   - `use super::*` 将父模块（audit_service）的所有公开项导入，方便测试使用
#[cfg(test)]
mod tests {
    use super::*;
    // tempfile::tempdir() 创建一个系统临时目录，测试结束后自动清理（RAII 模式）
    use tempfile::tempdir;

    // `#[test]` 标注一个函数为测试用例，`cargo test` 会自动发现并运行它
    #[test]
    fn log_audit_event_persists_to_db() {
        // 创建临时目录作为数据库存储路径
        // .unwrap() 会在 Result 为 Err 或 Option 为 None 时 panic，测试中常用
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();

        log_audit_event(
            &db,
            AuditEventType::FileImported,
            Some("task-1".to_string()),
            "File imported".to_string(),
        )
        .unwrap();

        let events = db.get_audit_events(100).unwrap();
        // assert_eq! 宏：断言两个值相等，不等则 panic 并打印差异
        assert_eq!(events.len(), 1);
        // .as_deref() 将 Option<String> 转换为 Option<&str>，便于与字符串字面量比较
        assert_eq!(events[0].task_id.as_deref(), Some("task-1"));
        assert_eq!(events[0].description, "File imported");
    }

    #[test]
    fn log_audit_event_without_task_id() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();

        // task_id 传入 None，表示这条审计事件不关联任何任务
        log_audit_event(
            &db,
            AuditEventType::AuditLogsCleared,
            None,
            "Logs cleared".to_string(),
        )
        .unwrap();

        let events = db.get_audit_events(100).unwrap();
        assert_eq!(events.len(), 1);
        // .is_none() 检查 Option 是否为 None
        assert!(events[0].task_id.is_none());
    }

    #[test]
    fn log_audit_event_with_task_id() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();

        // .clone() 克隆字符串，因为后面还需要用 task_id 做查询
        // Rust 的所有权规则：传入 Some(task_id) 会移动所有权，
        // 所以用 clone 保留一份副本供后续使用
        let task_id = "specific-task-42".to_string();
        log_audit_event(
            &db,
            AuditEventType::RecoveryStarted,
            Some(task_id.clone()),
            "Task started".to_string(),
        )
        .unwrap();

        // 按任务 ID 查询，验证只有该任务的审计事件被返回
        let events = db.get_audit_events_for_task(&task_id).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].task_id.as_deref(), Some("specific-task-42"));
    }
}
