use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::archive::ArchiveInfo;

/// 任务状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// 就绪
    Ready,
    /// 处理中
    Processing,
    /// 成功
    Succeeded,
    /// 已穷尽候选密码
    Exhausted,
    /// 已取消
    Cancelled,
    /// 失败
    Failed,
    /// 当前任务不受支持
    Unsupported,
    /// 上一次处理中断
    Interrupted,
}

/// 压缩包类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveType {
    Zip,
    SevenZ,
    Rar,
    Unknown,
}

/// 任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub file_path: String,
    pub file_name: String,
    pub file_size: u64,
    pub archive_type: ArchiveType,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub error_message: Option<String>,
    /// 恢复成功后找到的密码
    pub found_password: Option<String>,
    /// 压缩包检测结果 (JSON 序列化存储)
    pub archive_info: Option<ArchiveInfo>,
}

impl TaskStatus {
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

    pub fn for_import_result(
        archive_type: &ArchiveType,
        has_archive_info: bool,
        error_message: Option<&str>,
    ) -> Self {
        match archive_type {
            ArchiveType::Zip | ArchiveType::SevenZ | ArchiveType::Rar => {
                if has_archive_info {
                    Self::Ready
                } else {
                    Self::Failed
                }
            }
            ArchiveType::Unknown => {
                if error_message.is_some() {
                    Self::Failed
                } else {
                    Self::Unsupported
                }
            }
        }
    }

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
            "verifying" => Self::Processing,
            "cleaned" => Self::Cancelled,
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
