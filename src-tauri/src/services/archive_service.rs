use crate::domain::archive::{ArchiveEntry, ArchiveInfo};
use crate::domain::task::ArchiveType;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use sevenz_rust::{Password, SevenZReader};
use unrar::Archive as RarArchive;

pub fn detect_archive_type(file_path: &Path) -> Result<ArchiveType, String> {
    let mut file = File::open(file_path).map_err(|e| format!("无法打开文件: {}", e))?;
    let mut magic = [0u8; 8];
    let bytes_read = file
        .read(&mut magic)
        .map_err(|e| format!("读取文件失败: {}", e))?;

    if bytes_read < 2 {
        return Ok(ArchiveType::Unknown);
    }

    if magic[0] == 0x50 && magic[1] == 0x4B {
        return Ok(ArchiveType::Zip);
    }
    if bytes_read >= 6 && magic[0..6] == [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C] {
        return Ok(ArchiveType::SevenZ);
    }
    if bytes_read >= 4 && magic[0..4] == [0x52, 0x61, 0x72, 0x21] {
        return Ok(ArchiveType::Rar);
    }

    Ok(ArchiveType::Unknown)
}

pub fn inspect_zip(file_path: &Path) -> Result<ArchiveInfo, String> {
    let file = File::open(file_path).map_err(|e| format!("无法打开文件: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("无法解析 ZIP 文件: {}", e))?;

    let mut entries = Vec::new();
    let mut total_size: u64 = 0;
    let mut is_encrypted = false;

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
        has_encrypted_filenames: false,
        entries,
    })
}

pub fn inspect_7z(file_path: &Path) -> Result<ArchiveInfo, String> {
    // 先尝试无密码打开
    match SevenZReader::open(file_path, Password::empty()) {
        Ok(reader) => {
            // 无密码打开成功 → 非加密（或仅内容加密但文件头可读）
            let archive = reader.archive();
            let mut entries = Vec::new();
            let mut total_size: u64 = 0;

            for entry in &archive.files {
                let size = entry.size();
                total_size += size;
                entries.push(ArchiveEntry {
                    path: entry.name().to_string(),
                    size,
                    compressed_size: entry.compressed_size,
                    is_directory: entry.is_directory(),
                    is_encrypted: false,
                    last_modified: if entry.has_last_modified_date {
                        let ts = entry.last_modified_date().to_unix_time();
                        let dt = chrono::DateTime::from_timestamp(ts, 0);
                        dt.map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
                    } else {
                        None
                    },
                });
            }

            Ok(ArchiveInfo {
                total_entries: entries.len(),
                total_size,
                is_encrypted: false,
                has_encrypted_filenames: false,
                entries,
            })
        }
        Err(e) => {
            let err_str = format!("{}", e);
            // 检查是否是密码错误（加密的归档）
            if err_str.contains("PasswordRequired")
                || err_str.contains("password")
                || err_str.contains("Password")
            {
                // 加密的 7z — 如果文件名也加密了，我们无法获取条目列表
                Ok(ArchiveInfo {
                    total_entries: 0,
                    total_size: 0,
                    is_encrypted: true,
                    has_encrypted_filenames: true,
                    entries: Vec::new(),
                })
            } else {
                Err(format!("无法解析 7z 文件: {}", e))
            }
        }
    }
}

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
        is_encrypted: is_encrypted || has_encrypted_headers,
        has_encrypted_filenames: has_encrypted_headers,
        entries,
    })
}

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
