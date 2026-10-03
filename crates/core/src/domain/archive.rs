// serde 是 Rust 最流行的序列化/反序列化框架。
// Serialize 让结构体可以转成 JSON（或其他格式），
// Deserialize 让 JSON 可以反序列化成结构体。
use serde::{Deserialize, Serialize};

/// 压缩包内文件条目
///
/// 这个结构体描述压缩包里的一个文件或目录。
/// #[derive(Debug, Clone, Serialize, Deserialize)] 让编译器自动生成：
///   - Debug：允许用 {:?} 打印（方便调试）
///   - Clone：允许用 .clone() 深拷贝（复制一份新值）
///   - Serialize/Deserialize：与前端 JSON 互转
///
/// 注意：pub 关键字让这些字段可以在模块外访问。
/// 如果不加 pub，字段只在 domain 模块内可见。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntry {
    /// 文件在压缩包内的路径（例如 "docs/readme.txt"）
    pub path: String,
    /// 文件原始大小（字节）
    /// u64 是无符号 64 位整数，范围 0 到 18 EB，足够表示任意文件大小
    pub size: u64,
    /// 压缩后的大小（字节）
    pub compressed_size: u64,
    /// 是否是目录（目录本身没有内容）
    pub is_directory: bool,
    /// 文件内容是否被加密
    pub is_encrypted: bool,
    /// 最后修改时间（可能为空——某些压缩包格式不存储时间戳）
    /// Option<T> 是 Rust 表示"可能没有值"的类型，相当于其他语言的 null/None，
    /// 但 Rust 强制你处理 None 的情况，避免空指针错误。
    pub last_modified: Option<String>,
}

/// 压缩包整体元信息
///
/// 描述整个压缩包的统计数据和包含的文件列表。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveInfo {
    /// 文件条目总数（包含目录）
    pub total_entries: usize,
    /// 所有文件原始大小之和（字节）
    pub total_size: u64,
    /// 压缩包是否被加密（至少有一个文件加密）
    pub is_encrypted: bool,
    /// 是否连文件名也被加密（7Z 的 AES 头加密特性）
    pub has_encrypted_filenames: bool,
    /// 所有文件条目的列表
    /// Vec<T> 是 Rust 的动态数组（类似其他语言的 ArrayList 或 list），
    /// T 是元素类型，这里是 ArchiveEntry。
    pub entries: Vec<ArchiveEntry>,
}
