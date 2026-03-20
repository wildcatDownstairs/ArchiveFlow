use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// super 指向父模块（domain 模块），引入 archive 子模块中的 ArchiveInfo 类型。
use super::archive::ArchiveInfo;

/// 任务状态枚举
///
/// 描述一个密码恢复任务在生命周期中可能处于的状态。
/// #[serde(rename_all = "snake_case")] 使序列化时使用 snake_case：
///   Ready → "ready"，Processing → "processing"，依此类推。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// 就绪：已导入文件，等待开始恢复
    Ready,
    /// 处理中：正在尝试密码
    Processing,
    /// 成功：找到了正确密码
    Succeeded,
    /// 已穷尽候选密码：所有候选密码都试过了，没找到
    Exhausted,
    /// 已取消：用户手动停止
    Cancelled,
    /// 失败：发生了技术性错误（如文件损坏）
    Failed,
    /// 当前任务不受支持：压缩包格式无法处理
    Unsupported,
    /// 上一次处理中断：程序意外退出导致任务未完成
    Interrupted,
}

/// 压缩包类型枚举
///
/// #[serde(rename_all = "lowercase")] 让 SevenZ 序列化为 "sevenz"，
/// 而不是 "SevenZ"。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveType {
    Zip,
    SevenZ,
    Rar,
    Unknown,
}

/// 任务：代表一个压缩包密码恢复任务的完整数据
///
/// 这是应用的核心领域对象（Domain Object），存储在 SQLite 数据库中。
/// id、created_at、updated_at 等字段由数据库层自动管理。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// 任务唯一 ID（UUID v4 字符串格式）
    pub id: String,
    /// 压缩包文件的完整路径（文件系统路径）
    pub file_path: String,
    /// 压缩包文件名（不含路径，方便显示）
    pub file_name: String,
    /// 文件大小（字节）
    pub file_size: u64,
    /// 压缩包格式类型
    pub archive_type: ArchiveType,
    /// 当前任务状态
    pub status: TaskStatus,
    /// 任务创建时间
    pub created_at: DateTime<Utc>,
    /// 任务最后更新时间
    pub updated_at: DateTime<Utc>,
    /// 错误信息（仅在状态为 Failed 时有值）
    pub error_message: Option<String>,
    /// 恢复成功后找到的密码（仅在状态为 Succeeded 时有值）
    pub found_password: Option<String>,
    /// 压缩包检测结果（JSON 序列化后存储在数据库 TEXT 列中）
    /// 检测失败时为 None
    pub archive_info: Option<ArchiveInfo>,
}

impl TaskStatus {
    /// 返回状态的字符串表示（用于存入数据库）
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Processing => "processing",
            Self::Succeeded => "succeeded",
            Self::Exhausted => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Unsupported => "unsupported",
            Self::Interrupted => "interrupted",
        }
    }

    /// 从规范字符串解析状态（只接受当前版本的标准值）。
    /// 返回 Option<Self>：未知字符串返回 None。
    pub fn parse_canonical(raw: &str) -> Option<Self> {
        match raw {
            "ready" => Some(Self::Ready),
            "processing" => Some(Self::Processing),
            "succeeded" => Some(Self::Succeeded),
            "exhausted" => Some(Self::Exhausted),
            "cancelled" => Some(Self::Cancelled),
            "failed" => Some(Self::Failed),
            "unsupported" => Some(Self::Unsupported),
            "interrupted" => Some(Self::Interrupted),
            _ => None,
        }
    }

    /// 根据导入结果决定任务初始状态。
    ///
    /// 参数说明：
    ///   - archive_type: 检测到的压缩包类型
    ///   - has_archive_info: 是否成功解析出压缩包元信息
    ///   - error_message: 解析时是否遇到错误
    ///
    /// 逻辑：
    ///   - 已知格式（ZIP/7Z/RAR）且成功解析 → Ready
    ///   - 已知格式但解析失败 → Failed
    ///   - 未知格式且有错误 → Failed
    ///   - 未知格式且无错误 → Unsupported（无法处理但不是错误）
    pub fn for_import_result(
        archive_type: &ArchiveType,
        has_archive_info: bool,
        error_message: Option<&str>,
    ) -> Self {
        match archive_type {
            // Rust 的模式匹配支持用 | 同时匹配多个变体
            ArchiveType::Zip | ArchiveType::SevenZ | ArchiveType::Rar => {
                if has_archive_info {
                    Self::Ready
                } else {
                    Self::Failed
                }
            }
            ArchiveType::Unknown => {
                // .is_some() 检查 Option 是否有值
                if error_message.is_some() {
                    Self::Failed
                } else {
                    Self::Unsupported
                }
            }
        }
    }

    /// 从数据库读取时规范化状态字符串，支持旧版本数据库的历史值。
    ///
    /// 先尝试标准解析（parse_canonical），失败时再映射旧版本名称。
    /// 这种"渐进增强"策略保证了数据库升级后旧数据依然可用。
    ///
    /// if let Some(status) = ... 是 Rust 的"模式匹配 if"：
    ///   如果解析成功（得到 Some），就绑定变量 status 并执行 return。
    pub fn normalize_persisted(
        raw: &str,
        archive_type: &ArchiveType,
        error_message: Option<&str>,
        has_archive_info: bool,
    ) -> Self {
        if let Some(status) = Self::parse_canonical(raw) {
            return status;
        }

        match raw {
            // "verifying" 是旧版本中"处理中"的名称
            "verifying" => Self::Processing,
            // "cleaned" 是旧版本中"已取消"的名称
            "cleaned" => Self::Cancelled,
            // 其他旧版本状态：根据实际文件情况重新推断
            "imported" | "inspecting" | "waiting_authorization" => {
                Self::for_import_result(archive_type, has_archive_info, error_message)
            }
            _ => Self::for_import_result(archive_type, has_archive_info, error_message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ArchiveType, TaskStatus};

    #[test]
    fn canonical_statuses_parse_roundtrip() {
        // 测试所有状态的双向转换：as_str() 和 parse_canonical() 互为逆操作
        for (raw, expected) in [
            ("ready", TaskStatus::Ready),
            ("processing", TaskStatus::Processing),
            ("succeeded", TaskStatus::Succeeded),
            ("exhausted", TaskStatus::Exhausted),
            ("cancelled", TaskStatus::Cancelled),
            ("failed", TaskStatus::Failed),
            ("unsupported", TaskStatus::Unsupported),
            ("interrupted", TaskStatus::Interrupted),
        ] {
            assert_eq!(TaskStatus::parse_canonical(raw), Some(expected.clone()));
            assert_eq!(expected.as_str(), raw);
        }
    }

    #[test]
    fn import_result_uses_archive_capability_boundaries() {
        assert_eq!(
            TaskStatus::for_import_result(&ArchiveType::Zip, true, None),
            TaskStatus::Ready
        );
        assert_eq!(
            TaskStatus::for_import_result(&ArchiveType::Zip, false, Some("boom")),
            TaskStatus::Failed
        );
        // 7Z 和 RAR 现在也完全支持
        assert_eq!(
            TaskStatus::for_import_result(&ArchiveType::SevenZ, true, None),
            TaskStatus::Ready
        );
        assert_eq!(
            TaskStatus::for_import_result(&ArchiveType::SevenZ, false, None),
            TaskStatus::Failed
        );
        assert_eq!(
            TaskStatus::for_import_result(&ArchiveType::Rar, true, None),
            TaskStatus::Ready
        );
        assert_eq!(
            TaskStatus::for_import_result(&ArchiveType::Unknown, false, Some("bad file")),
            TaskStatus::Failed
        );
    }

    #[test]
    fn legacy_statuses_are_normalized() {
        assert_eq!(
            TaskStatus::normalize_persisted("imported", &ArchiveType::Zip, None, true),
            TaskStatus::Ready
        );
        assert_eq!(
            TaskStatus::normalize_persisted("imported", &ArchiveType::Rar, None, false),
            TaskStatus::Failed
        );
        assert_eq!(
            TaskStatus::normalize_persisted("verifying", &ArchiveType::Zip, None, true),
            TaskStatus::Processing
        );
        assert_eq!(
            TaskStatus::normalize_persisted("cleaned", &ArchiveType::Zip, None, true),
            TaskStatus::Cancelled
        );
    }
}
