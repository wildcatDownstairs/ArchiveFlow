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

pub fn inspect_archive(file_path: &Path) -> Result<(ArchiveType, Option<ArchiveInfo>), String> {
    let archive_type = detect_archive_type(file_path)?;

    match archive_type {
        ArchiveType::Zip => {
            let info = inspect_zip(file_path)?;
            Ok((ArchiveType::Zip, Some(info)))
        }
        ArchiveType::SevenZ => Ok((ArchiveType::SevenZ, None)),
        ArchiveType::Rar => Ok((ArchiveType::Rar, None)),
        ArchiveType::Unknown => Err("无法识别的文件格式".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixtures_dir() -> std::path::PathBuf {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.parent().unwrap().join("fixtures").join("zip")
    }

    // ─── detect_archive_type ────────────────────────────────────────

    #[test]
    fn detect_normal_zip() {
        let path = fixtures_dir().join("normal.zip");
        assert_eq!(detect_archive_type(&path).unwrap(), ArchiveType::Zip);
    }

    #[test]
    fn detect_encrypted_aes_zip() {
        let path = fixtures_dir().join("encrypted-aes.zip");
        assert_eq!(detect_archive_type(&path).unwrap(), ArchiveType::Zip);
    }

    #[test]
    fn detect_7z_magic_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.7z");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x00])
            .unwrap();
        assert_eq!(detect_archive_type(&path).unwrap(), ArchiveType::SevenZ);
    }

    #[test]
    fn detect_rar_magic_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.rar");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00, 0x00])
            .unwrap();
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
        let path = fixtures_dir().join("normal.zip");
        let info = inspect_zip(&path).unwrap();
        assert!(!info.is_encrypted);
        assert!(info.total_entries > 0);
    }

    #[test]
    fn inspect_encrypted_aes_zip_is_encrypted() {
        let path = fixtures_dir().join("encrypted-aes.zip");
        let info = inspect_zip(&path).unwrap();
        assert!(info.is_encrypted);
    }

    #[test]
    fn inspect_empty_zip() {
        let path = fixtures_dir().join("empty.zip");
        let info = inspect_zip(&path).unwrap();
        assert_eq!(info.total_entries, 0);
        assert_eq!(info.total_size, 0);
    }

    // ─── inspect_archive ────────────────────────────────────────────

    #[test]
    fn inspect_archive_normal_zip() {
        let path = fixtures_dir().join("normal.zip");
        let (archive_type, info) = inspect_archive(&path).unwrap();
        assert_eq!(archive_type, ArchiveType::Zip);
        assert!(info.is_some());
    }

    #[test]
    fn inspect_archive_7z_returns_none_info() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.7z");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x00])
            .unwrap();
        let (archive_type, info) = inspect_archive(&path).unwrap();
        assert_eq!(archive_type, ArchiveType::SevenZ);
        assert!(info.is_none());
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
