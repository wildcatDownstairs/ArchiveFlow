// archive_service.rs —— 压缩包解析服务
//
// 这个文件提供两类功能：
//   1. 检测文件类型（通过"魔数"字节判断是 ZIP / 7z / RAR）
//   2. 检查各类压缩包内容（条目列表、加密状态等）
//
// 注意：这里只做"只读解析"，不做密码恢复。
// 密码恢复逻辑在 recovery_service.rs 中。

use crate::domain::archive::{ArchiveEntry, ArchiveInfo};
use crate::domain::task::ArchiveType;
use std::fs::File;
use std::io::{self, Read, Seek};
use std::path::Path;

// 第三方库：
//   sevenz_rust — 用于读写 7z 格式
//   unrar      — 用于读写 RAR 格式（依赖本地 unrar 共享库）
use sevenz_rust::{Error as SevenZError, Password, SevenZReader};
use unrar::Archive as RarArchive;

/// 通过读取文件头部的"魔数"字节来判断文件格式。
///
/// 为什么用"魔数"而不是文件扩展名？
///   - 文件扩展名可以被任意修改（rename .txt → .zip），不可靠。
///   - 每种二进制格式都有固定的开头字节（称为"magic bytes"或"文件签名"），
///     这是格式规范的一部分，任何符合标准的压缩包都必须有这几个字节。
///
/// ZIP  魔数: PK（0x50 0x4B），源自 Phil Katz 名字缩写。
/// 7z   魔数: 7zXAF'! (0x37 0x7A 0xBC 0xAF 0x27 0x1C)
/// RAR  魔数: Rar! (0x52 0x61 0x72 0x21)
pub fn detect_archive_type(file_path: &Path) -> Result<ArchiveType, String> {
    // map_err 将 io::Error 转换成我们自定义的 String 错误消息
    let mut file = File::open(file_path).map_err(|e| format!("无法打开文件: {}", e))?;
    // [0u8; 8] 是一个长度为 8 的字节数组，初始值全为 0
    let mut magic = [0u8; 8];
    let bytes_read = file
        .read(&mut magic)
        .map_err(|e| format!("读取文件失败: {}", e))?;

    // 文件太短（< 2 字节），无法识别
    if bytes_read < 2 {
        return Ok(ArchiveType::Unknown);
    }

    // 检查前两字节是否为 PK（ZIP 魔数）
    if magic[0] == 0x50 && magic[1] == 0x4B {
        return Ok(ArchiveType::Zip);
    }
    // magic[0..6] 是切片语法：取索引 0~5 共 6 个字节
    if bytes_read >= 6 && magic[0..6] == [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C] {
        return Ok(ArchiveType::SevenZ);
    }
    if bytes_read >= 4 && magic[0..4] == [0x52, 0x61, 0x72, 0x21] {
        return Ok(ArchiveType::Rar);
    }

    Ok(ArchiveType::Unknown)
}

/// 解析 ZIP 文件，返回条目列表、加密状态、总大小等信息。
///
/// by_index_raw 以"原始"模式读取条目（不尝试解密），
/// 因此即使 ZIP 是加密的，也能读到文件名和元数据。
pub fn inspect_zip(file_path: &Path) -> Result<ArchiveInfo, String> {
    let file = File::open(file_path).map_err(|e| format!("无法打开文件: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("无法解析 ZIP 文件: {}", e))?;

    let mut entries = Vec::new();
    let mut total_size: u64 = 0;
    let mut is_encrypted = false;

    // 0..archive.len() 是 Rust 的 Range，相当于 for i = 0; i < len; i++
    for i in 0..archive.len() {
        let entry = archive
            .by_index_raw(i)
            .map_err(|e| format!("读取条目失败: {}", e))?;
        let encrypted = entry.encrypted();
        if encrypted {
            is_encrypted = true;
        }

        let size = entry.size();
        let compressed_size = entry.compressed_size();
        total_size += size;

        // .map(|dt| ...) 只在 Option 有值时执行闭包，将时间戳格式化为字符串
        entries.push(ArchiveEntry {
            path: entry.name().to_string(),
            size,
            compressed_size,
            is_directory: entry.is_dir(),
            is_encrypted: encrypted,
            last_modified: entry.last_modified().map(|dt| {
                format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                    dt.year(),
                    dt.month(),
                    dt.day(),
                    dt.hour(),
                    dt.minute(),
                    dt.second()
                )
            }),
        });
    }

    Ok(ArchiveInfo {
        total_entries: entries.len(),
        total_size,
        is_encrypted,
        // ZIP 格式不支持加密文件名，文件名始终明文可见
        has_encrypted_filenames: false,
        entries,
    })
}

/// 判断一个 7z 错误是否属于"密码相关"错误。
///
/// matches! 宏：等价于 `error == X || error == Y`，但更简洁，
/// 且对枚举的变体能做模式匹配（不要求枚举实现 PartialEq）。
fn is_7z_password_error(error: &SevenZError) -> bool {
    matches!(
        error,
        SevenZError::PasswordRequired | SevenZError::MaybeBadPassword(_)
    )
}

/// 从已打开的 7z reader 中收集条目信息（文件列表 + 总大小）。
///
/// R: Read + Seek 是泛型约束（trait bounds）：
///   R 必须同时实现 Read（可读）和 Seek（可随机访问）traits。
///   这样这个函数就能接受任何满足条件的数据源，比如文件、内存缓冲区等。
fn collect_7z_entries<R: Read + Seek>(reader: &SevenZReader<R>) -> (Vec<ArchiveEntry>, u64) {
    let mut entries = Vec::new();
    let mut total_size: u64 = 0;

    for entry in &reader.archive().files {
        let size = entry.size();
        total_size += size;
        entries.push(ArchiveEntry {
            path: entry.name().to_string(),
            size,
            compressed_size: entry.compressed_size,
            is_directory: entry.is_directory(),
            // 7z 的加密状态通过实际解密来检测，这里先设 false，后面会修正
            is_encrypted: false,
            last_modified: if entry.has_last_modified_date {
                // to_unix_time() 返回 Unix 时间戳（秒数）
                let ts = entry.last_modified_date().to_unix_time();
                let dt = chrono::DateTime::from_timestamp(ts, 0);
                dt.map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
            } else {
                None
            },
        });
    }

    (entries, total_size)
}

/// 尝试实际读取 7z 所有文件内容以验证密码是否正确（或文件是否完整）。
///
/// io::copy(src, dst) 把数据从 src 复制到 dst。
/// io::sink() 是一个"黑洞"写入目标，所有写入都被丢弃——这里只需触发解密/校验，
/// 不需要保留数据，用 sink() 避免分配内存。
///
/// 返回 Ok(true)  → 成功验证了至少一个文件（密码正确）
/// 返回 Ok(false) → 没有可验证的文件（如空包或全是目录）
/// 返回 Err(_)    → 解密失败（密码错误或文件损坏）
fn validate_7z_payload<R: Read + Seek>(reader: &mut SevenZReader<R>) -> Result<bool, SevenZError> {
    let mut validated_any_file = false;

    reader.for_each_entries(|entry, entry_reader| {
        // 跳过目录和无数据流的条目（7z 可以存储仅元数据的条目）
        if entry.is_directory() || !entry.has_stream() {
            return Ok(true); // Ok(true) 表示继续迭代
        }

        validated_any_file = true;
        // 读取并丢弃数据，触发 CRC 校验和解密验证
        io::copy(entry_reader, &mut io::sink())?;
        Ok(true)
    })?;

    Ok(validated_any_file)
}

/// 解析 7z 文件，返回条目列表和加密状态。
///
/// 7z 有两种加密模式：
///   1. 内容加密（content-only）：文件名可见，内容加密。
///      → SevenZReader::open(空密码) 成功，但 validate_7z_payload 返回密码错误。
///   2. 头部加密（header-encrypted）：文件名也加密。
///      → SevenZReader::open(空密码) 直接失败，返回 PasswordRequired。
///
/// match 表达式和嵌套 Err 处理展示了 Rust 的模式匹配能力。
pub fn inspect_7z(file_path: &Path) -> Result<ArchiveInfo, String> {
    match SevenZReader::open(file_path, Password::empty()) {
        Ok(mut reader) => {
            // 打开成功（可能是未加密，也可能是只加密了内容）
            let (mut entries, total_size) = collect_7z_entries(&reader);

            match validate_7z_payload(&mut reader) {
                Ok(_) => Ok(ArchiveInfo {
                    total_entries: entries.len(),
                    total_size,
                    is_encrypted: false,
                    has_encrypted_filenames: false,
                    entries,
                }),
                Err(error) if is_7z_password_error(&error) => {
                    // "if 守卫"（guard）：只在条件满足时匹配这个分支。
                    // 这里表示：打开成功但读取失败 → 内容加密（文件名可见）。
                    // 把所有非目录条目标记为加密。
                    for entry in &mut entries {
                        if !entry.is_directory {
                            entry.is_encrypted = true;
                        }
                    }

                    Ok(ArchiveInfo {
                        total_entries: entries.len(),
                        total_size,
                        is_encrypted: true,
                        has_encrypted_filenames: false,
                        entries,
                    })
                }
                Err(error) => Err(format!("无法解析 7z 文件: {}", error)),
            }
        }
        Err(error) => {
            if is_7z_password_error(&error) {
                // 打开直接失败 → 头部加密（文件名也不可见），返回空条目列表
                Ok(ArchiveInfo {
                    total_entries: 0,
                    total_size: 0,
                    is_encrypted: true,
                    has_encrypted_filenames: true,
                    entries: Vec::new(),
                })
            } else {
                Err(format!("无法解析 7z 文件: {}", error))
            }
        }
    }
}

/// 解析 RAR 文件，返回条目列表和加密状态。
///
/// unrar crate 的 API 是迭代器风格：
///   - open_for_listing() 以"只列表不解压"模式打开
///   - archive.has_encrypted_headers() 检测头部是否加密
///   - for result in archive 迭代每个条目的元数据
///
/// to_string_lossy() 把 OsStr（可能含非 UTF-8 字节）转成 String，
/// 遇到无效字节时用 U+FFFD 替代（"lossy"的意思），不会 panic。
pub fn inspect_rar(file_path: &Path) -> Result<ArchiveInfo, String> {
    let file_path_str = file_path.to_string_lossy().to_string();

    // 尝试打开列表模式
    let archive = RarArchive::new(&file_path_str)
        .open_for_listing()
        .map_err(|e| format!("无法打开 RAR 文件: {}", e))?;

    let has_encrypted_headers = archive.has_encrypted_headers();

    let mut entries = Vec::new();
    let mut total_size: u64 = 0;
    let mut is_encrypted = false;

    for result in archive {
        match result {
            Ok(header) => {
                if header.is_encrypted() {
                    is_encrypted = true;
                }
                let size = header.unpacked_size;
                total_size += size;

                entries.push(ArchiveEntry {
                    path: header.filename.to_string_lossy().to_string(),
                    size,
                    compressed_size: 0, // RAR listing 不直接提供压缩大小
                    is_directory: header.is_directory(),
                    is_encrypted: header.is_encrypted(),
                    last_modified: None, // RAR 的 file_time 是 DOS 时间格式，暂不解析
                });
            }
            Err(e) => {
                // 如果是加密头导致的错误，标记为加密
                let code = e.code;
                if matches!(code, unrar::error::Code::MissingPassword) {
                    return Ok(ArchiveInfo {
                        total_entries: 0,
                        total_size: 0,
                        is_encrypted: true,
                        has_encrypted_filenames: true,
                        entries: Vec::new(),
                    });
                }
                return Err(format!("读取 RAR 条目失败: {}", e));
            }
        }
    }

    Ok(ArchiveInfo {
        total_entries: entries.len(),
        total_size,
        // 任意条目加密，或者头部加密，整体标记为加密
        is_encrypted: is_encrypted || has_encrypted_headers,
        has_encrypted_filenames: has_encrypted_headers,
        entries,
    })
}

/// 统一入口：自动检测类型并解析对应格式的压缩包。
///
/// 返回 (ArchiveType, Option<ArchiveInfo>)。
/// 对于已知类型（Zip/7z/Rar），ArchiveInfo 一定是 Some(...)。
/// 对于 Unknown，直接返回 Err（不支持的格式）。
pub fn inspect_archive(file_path: &Path) -> Result<(ArchiveType, Option<ArchiveInfo>), String> {
    let archive_type = detect_archive_type(file_path)?;

    match archive_type {
        ArchiveType::Zip => {
            let info = inspect_zip(file_path)?;
            Ok((ArchiveType::Zip, Some(info)))
        }
        ArchiveType::SevenZ => {
            let info = inspect_7z(file_path)?;
            Ok((ArchiveType::SevenZ, Some(info)))
        }
        ArchiveType::Rar => {
            let info = inspect_rar(file_path)?;
            Ok((ArchiveType::Rar, Some(info)))
        }
        ArchiveType::Unknown => Err("无法识别的文件格式".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_content_encrypted_7z(password: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("hello.txt");
        let archive = dir.path().join("content-encrypted.7z");
        std::fs::write(&source, "secret payload").unwrap();
        sevenz_rust::compress_to_path_encrypted(&source, &archive, Password::from(password))
            .unwrap();
        (dir, archive)
    }

    fn zip_fixtures_dir() -> std::path::PathBuf {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.parent().unwrap().join("fixtures").join("zip")
    }

    fn sevenz_fixtures_dir() -> std::path::PathBuf {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.parent().unwrap().join("fixtures").join("7z")
    }

    fn rar_fixtures_dir() -> std::path::PathBuf {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.parent().unwrap().join("fixtures").join("rar")
    }

    // ─── detect_archive_type ────────────────────────────────────────

    #[test]
    fn detect_normal_zip() {
        let path = zip_fixtures_dir().join("normal.zip");
        assert_eq!(detect_archive_type(&path).unwrap(), ArchiveType::Zip);
    }

    #[test]
    fn detect_encrypted_aes_zip() {
        let path = zip_fixtures_dir().join("encrypted-aes.zip");
        assert_eq!(detect_archive_type(&path).unwrap(), ArchiveType::Zip);
    }

    #[test]
    fn detect_7z_file() {
        let path = sevenz_fixtures_dir().join("normal.7z");
        assert_eq!(detect_archive_type(&path).unwrap(), ArchiveType::SevenZ);
    }

    #[test]
    fn detect_rar_file() {
        let path = rar_fixtures_dir().join("normal.rar");
        assert_eq!(detect_archive_type(&path).unwrap(), ArchiveType::Rar);
    }

    #[test]
    fn detect_random_bytes_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bin");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08])
            .unwrap();
        assert_eq!(detect_archive_type(&path).unwrap(), ArchiveType::Unknown);
    }

    #[test]
    fn detect_nonexistent_file_is_err() {
        let path = std::path::Path::new("/nonexistent/file.zip");
        assert!(detect_archive_type(path).is_err());
    }

    // ─── inspect_zip ────────────────────────────────────────────────

    #[test]
    fn inspect_normal_zip_not_encrypted() {
        let path = zip_fixtures_dir().join("normal.zip");
        let info = inspect_zip(&path).unwrap();
        assert!(!info.is_encrypted);
        assert!(info.total_entries > 0);
    }

    #[test]
    fn inspect_encrypted_aes_zip_is_encrypted() {
        let path = zip_fixtures_dir().join("encrypted-aes.zip");
        let info = inspect_zip(&path).unwrap();
        assert!(info.is_encrypted);
    }

    #[test]
    fn inspect_empty_zip() {
        let path = zip_fixtures_dir().join("empty.zip");
        let info = inspect_zip(&path).unwrap();
        assert_eq!(info.total_entries, 0);
        assert_eq!(info.total_size, 0);
    }

    // ─── inspect_7z ─────────────────────────────────────────────────

    #[test]
    fn inspect_normal_7z_not_encrypted() {
        let path = sevenz_fixtures_dir().join("normal.7z");
        let info = inspect_7z(&path).unwrap();
        assert!(!info.is_encrypted);
        assert!(info.total_entries > 0);
        assert!(info.total_size > 0);
    }

    #[test]
    fn inspect_normal_7z_has_entries() {
        let path = sevenz_fixtures_dir().join("normal.7z");
        let info = inspect_7z(&path).unwrap();
        // 验证至少有 hello.txt 和 data.bin
        let names: Vec<&str> = info.entries.iter().map(|e| e.path.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("hello.txt")));
        assert!(names.iter().any(|n| n.contains("data.bin")));
    }

    #[test]
    fn inspect_encrypted_7z_is_encrypted() {
        let path = sevenz_fixtures_dir().join("encrypted.7z");
        let info = inspect_7z(&path).unwrap();
        assert!(info.is_encrypted);
    }

    #[test]
    fn inspect_content_encrypted_7z_without_encrypted_headers() {
        let (_dir, path) = make_content_encrypted_7z("test123");
        let info = inspect_7z(&path).unwrap();

        assert!(info.is_encrypted);
        assert!(!info.has_encrypted_filenames);
        assert_eq!(info.total_entries, 1);
        assert!(info.entries.iter().all(|entry| entry.is_encrypted));
    }

    #[test]
    fn inspect_empty_7z() {
        let path = sevenz_fixtures_dir().join("empty.7z");
        let info = inspect_7z(&path).unwrap();
        // sevenz_rust 压缩空目录时会包含一个目录条目
        // 所有条目要么是目录要么大小为 0
        assert!(!info.is_encrypted);
        assert_eq!(info.total_size, 0);
        for entry in &info.entries {
            assert!(entry.is_directory || entry.size == 0);
        }
    }

    // ─── inspect_rar ────────────────────────────────────────────────

    #[test]
    fn inspect_normal_rar_not_encrypted() {
        let path = rar_fixtures_dir().join("normal.rar");
        let info = inspect_rar(&path).unwrap();
        assert!(!info.is_encrypted);
        assert!(info.total_entries > 0);
    }

    #[test]
    fn inspect_encrypted_rar_is_encrypted() {
        let path = rar_fixtures_dir().join("encrypted.rar");
        let info = inspect_rar(&path).unwrap();
        assert!(info.is_encrypted);
        // 该 RAR 不加密文件头，所以能列出条目
        assert!(info.total_entries > 0);
        assert!(info.entries.iter().any(|e| e.is_encrypted));
    }

    #[test]
    fn inspect_encrypted_headers_rar() {
        let path = rar_fixtures_dir().join("encrypted-headers.rar");
        let info = inspect_rar(&path).unwrap();
        assert!(info.is_encrypted);
        assert!(info.has_encrypted_filenames);
    }

    // ─── inspect_archive (集成) ─────────────────────────────────────

    #[test]
    fn inspect_archive_normal_zip() {
        let path = zip_fixtures_dir().join("normal.zip");
        let (archive_type, info) = inspect_archive(&path).unwrap();
        assert_eq!(archive_type, ArchiveType::Zip);
        assert!(info.is_some());
    }

    #[test]
    fn inspect_archive_7z_returns_info() {
        let path = sevenz_fixtures_dir().join("normal.7z");
        let (archive_type, info) = inspect_archive(&path).unwrap();
        assert_eq!(archive_type, ArchiveType::SevenZ);
        assert!(info.is_some());
        let info = info.unwrap();
        assert!(!info.is_encrypted);
        assert!(info.total_entries > 0);
    }

    #[test]
    fn inspect_archive_rar_returns_info() {
        let path = rar_fixtures_dir().join("normal.rar");
        let (archive_type, info) = inspect_archive(&path).unwrap();
        assert_eq!(archive_type, ArchiveType::Rar);
        assert!(info.is_some());
    }

    #[test]
    fn inspect_archive_random_bytes_is_err() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bin");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08])
            .unwrap();
        assert!(inspect_archive(&path).is_err());
    }
}
