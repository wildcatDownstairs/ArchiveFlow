use serde::{Deserialize, Serialize};

/// 压缩包内文件条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntry {
    pub path: String,
    pub size: u64,
    pub compressed_size: u64,
    pub is_directory: bool,
    pub is_encrypted: bool,
    pub last_modified: Option<String>,
}

/// 压缩包元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveInfo {
    pub total_entries: usize,
    pub total_size: u64,
    pub is_encrypted: bool,
    pub has_encrypted_filenames: bool,
    pub entries: Vec<ArchiveEntry>,
}
