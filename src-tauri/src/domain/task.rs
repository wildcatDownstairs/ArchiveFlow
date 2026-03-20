use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::archive::ArchiveInfo;

/// 任务状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// 已导入
    Imported,
    /// 检查中
    Inspecting,
    /// 等待授权
    WaitingAuthorization,
    /// 就绪
    Ready,
    /// 处理中
    Processing,
    /// 校验中
    Verifying,
    /// 成功
    Succeeded,
    /// 已穷尽候选密码
    Exhausted,
    /// 已取消
    Cancelled,
    /// 失败
    Failed,
    /// 已清理
    Cleaned,
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
