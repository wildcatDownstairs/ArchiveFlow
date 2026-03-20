use crate::domain::archive::{ArchiveEntry, ArchiveInfo};
use crate::domain::task::ArchiveType;
use std::fs::File;
use std::io::Read;
use std::path::Path;

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

pub fn inspect_archive(file_path: &Path) -> Result<(ArchiveType, ArchiveInfo), String> {
    let archive_type = detect_archive_type(file_path)?;

    match archive_type {
        ArchiveType::Zip => {
            let info = inspect_zip(file_path)?;
            Ok((ArchiveType::Zip, info))
        }
        ArchiveType::SevenZ => {
            let metadata =
                std::fs::metadata(file_path).map_err(|e| format!("读取文件信息失败: {}", e))?;
            Ok((
                ArchiveType::SevenZ,
                ArchiveInfo {
                    total_entries: 0,
                    total_size: metadata.len(),
                    is_encrypted: false,
                    has_encrypted_filenames: false,
                    entries: vec![],
                },
            ))
        }
        ArchiveType::Rar => {
            let metadata =
                std::fs::metadata(file_path).map_err(|e| format!("读取文件信息失败: {}", e))?;
            Ok((
                ArchiveType::Rar,
                ArchiveInfo {
                    total_entries: 0,
                    total_size: metadata.len(),
                    is_encrypted: false,
                    has_encrypted_filenames: false,
                    entries: vec![],
                },
            ))
        }
        ArchiveType::Unknown => Err("无法识别的文件格式".to_string()),
    }
}
