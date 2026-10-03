// chrono 是处理日期和时间的第三方库。
// DateTime<Utc> 表示带时区信息的时间点（UTC 协调世界时）。
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 审计事件类型枚举
///
/// 枚举（enum）是 Rust 中非常强大的类型，每个变体可以携带不同的数据。
/// 这里的变体都是"单元变体"（unit variant），没有附带数据，纯粹表示类别。
///
/// #[serde(rename_all = "snake_case")] 告诉 serde 序列化时把变体名转成 snake_case：
///   FileImported → "file_imported"
/// 这样前端 JSON 收到的是 "file_imported" 而不是 "FileImported"。
///
/// PartialEq 让两个枚举值可以用 == 比较相等性。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    FileImported,
    TaskStatusUpdated,
    TaskDeleted,
    TasksCleared,
    TaskFailed,
    TaskUnsupported,
    TaskInterrupted,
    RecoveryQueued,
    RecoveryStarted,
    RecoveryPaused,
    RecoveryResumed,
    RecoverySucceeded,
    RecoveryExhausted,
    RecoveryCancelled,
    RecoveryFailed,
    AuditLogsCleared,
    SettingChanged,
    AuthorizationGranted,
    ResultExported,
    CacheCleared,
}

/// 审计事件：记录系统中发生的重要操作，用于追溯历史
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// 事件唯一 ID（UUID 格式字符串）
    pub id: String,
    /// 事件类型
    pub event_type: AuditEventType,
    /// 关联的任务 ID（有些审计事件不关联具体任务，所以是 Option）
    pub task_id: Option<String>,
    /// 事件描述文字
    pub description: String,
    /// 事件发生时间（UTC）
    pub timestamp: DateTime<Utc>,
}

// impl 块：为 AuditEventType 枚举实现方法。
// Rust 没有类（class），方法通过 impl 块附加到类型上。
impl AuditEventType {
    /// 返回事件类型对应的字符串标识（用于存储到数据库）。
    ///
    /// 返回类型 &'static str 表示：
    ///   - &str 是字符串切片（借用，不拥有字符串数据）
    ///   - 'static 生命周期表示这个引用在整个程序运行期间都有效
    ///     （因为这些字符串字面量编译进了程序的只读数据段）
    ///
    /// 使用 match 表达式穷举所有变体——如果漏掉某个变体，编译器会报错！
    /// 这正是 Rust 枚举的安全性所在。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FileImported => "file_imported",
            Self::TaskStatusUpdated => "task_status_updated",
            Self::TaskDeleted => "task_deleted",
            Self::TasksCleared => "tasks_cleared",
            Self::TaskFailed => "task_failed",
            Self::TaskUnsupported => "task_unsupported",
            Self::TaskInterrupted => "task_interrupted",
            Self::RecoveryQueued => "recovery_queued",
            Self::RecoveryStarted => "recovery_started",
            Self::RecoveryPaused => "recovery_paused",
            Self::RecoveryResumed => "recovery_resumed",
            Self::RecoverySucceeded => "recovery_succeeded",
            Self::RecoveryExhausted => "recovery_exhausted",
            Self::RecoveryCancelled => "recovery_cancelled",
            Self::RecoveryFailed => "recovery_failed",
            Self::AuditLogsCleared => "audit_logs_cleared",
            Self::SettingChanged => "setting_changed",
            Self::AuthorizationGranted => "authorization_granted",
            Self::ResultExported => "result_exported",
            Self::CacheCleared => "cache_cleared",
        }
    }

    /// 从数据库持久化的字符串解析事件类型，支持旧版本的别名。
    ///
    /// 返回 Option<Self>：解析成功返回 Some(变体)，未知字符串返回 None。
    ///
    /// 为什么需要这个方法而不直接用 serde？
    /// 因为早期版本数据库存的是 "task_created"、"task_completed" 这样的旧名，
    /// 需要向前兼容映射到新的枚举变体。
    pub fn parse_persisted(raw: &str) -> Option<Self> {
        match raw {
            // "task_created" 是旧版本的名称，现在映射到 FileImported
            "file_imported" | "task_created" => Some(Self::FileImported),
            "task_status_updated" => Some(Self::TaskStatusUpdated),
            "task_deleted" => Some(Self::TaskDeleted),
            "tasks_cleared" => Some(Self::TasksCleared),
            "task_failed" => Some(Self::RecoveryFailed),
            "task_unsupported" => Some(Self::TaskUnsupported),
            "task_interrupted" => Some(Self::TaskInterrupted),
            "recovery_queued" => Some(Self::RecoveryQueued),
            // "task_started" 是旧版本名称
            "task_started" | "recovery_started" => Some(Self::RecoveryStarted),
            "recovery_paused" => Some(Self::RecoveryPaused),
            "recovery_resumed" => Some(Self::RecoveryResumed),
            // "task_completed" 是旧版本名称
            "task_completed" | "recovery_succeeded" => Some(Self::RecoverySucceeded),
            "recovery_exhausted" => Some(Self::RecoveryExhausted),
            "recovery_cancelled" => Some(Self::RecoveryCancelled),
            "recovery_failed" => Some(Self::RecoveryFailed),
            "audit_logs_cleared" => Some(Self::AuditLogsCleared),
            "setting_changed" => Some(Self::SettingChanged),
            "authorization_granted" => Some(Self::AuthorizationGranted),
            "result_exported" => Some(Self::ResultExported),
            "cache_cleared" => Some(Self::CacheCleared),
            // _ 是通配符，匹配任何其他情况
            _ => None,
        }
    }
}

// #[cfg(test)] 表示这个模块只在运行测试时编译。
// Release 构建中这些代码完全不存在，不影响产物大小。
#[cfg(test)]
mod tests {
    // super 指向父模块（即 audit 模块），use super::X 引入父模块中的类型。
    use super::AuditEventType;

    // #[test] 标注一个函数为单元测试，cargo test 会自动发现并运行它。
    #[test]
    fn persisted_values_support_legacy_aliases() {
        // assert_eq!(left, right) 断言两个值相等，不等则测试失败并打印错误。
        assert_eq!(
            AuditEventType::parse_persisted("task_started"),
            Some(AuditEventType::RecoveryStarted)
        );
        assert_eq!(
            AuditEventType::parse_persisted("task_completed"),
            Some(AuditEventType::RecoverySucceeded)
        );
        assert_eq!(
            AuditEventType::parse_persisted("task_failed"),
            Some(AuditEventType::RecoveryFailed)
        );
        assert_eq!(
            AuditEventType::parse_persisted("task_created"),
            Some(AuditEventType::FileImported)
        );
        assert_eq!(
            AuditEventType::parse_persisted("task_status_updated"),
            Some(AuditEventType::TaskStatusUpdated)
        );
        assert_eq!(
            AuditEventType::parse_persisted("setting_changed"),
            Some(AuditEventType::SettingChanged)
        );
    }
}
