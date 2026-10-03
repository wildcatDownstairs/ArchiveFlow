// 这个文件只负责"验证一个密码对不对"。
// 它不关心候选空间怎么生成，也不关心多线程调度。
// 这样做的好处是：当某种压缩格式的验证逻辑需要修复时，
// 只需要看这一小块文件，而不用在 1000+ 行的大文件里来回跳。

use std::io::{Read, Seek};
use std::path::Path;

use sevenz_rust::{Error as SevenZError, Password, SevenZReader};

use crate::domain::task::ArchiveType;
use crate::services::archive_service;

/// 在已打开的 ZipArchive 上，用给定密码尝试解密指定索引的条目。
///
/// # 为什么复用 archive？
/// 每次打开文件都需要解析文件头（有 IO 开销），而 ZipArchive 可以复用。
/// 多线程场景下每个 worker 线程拥有自己的 ZipArchive 实例，避免锁竞争。
///
/// # 泛型参数 `R: Read + Seek`
/// 这是 trait bounds（特征约束）。意思是：R 可以是任何同时实现了
/// `Read`（可读取字节）和 `Seek`（可随机定位）的类型，
/// 例如 `std::fs::File`、`std::io::Cursor<Vec<u8>>` 等。
/// 这让函数既能用于真实文件，也能用于内存缓冲区（方便测试）。
pub fn try_password_on_archive<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    index: usize,
    password: &str,
) -> bool {
    let result = archive.by_index_decrypt(index, password.as_bytes());
    let mut zip_file = match result {
        Ok(f) => f,
        Err(_) => return false,
    };

    let mut buf = Vec::new();
    zip_file.read_to_end(&mut buf).is_ok()
}

/// 独立版本：每次都重新打开文件，尝试解密第一个加密条目。
#[allow(dead_code)]
pub fn try_password_zip(file_path: &Path, password: &str) -> bool {
    let file = match std::fs::File::open(file_path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(_) => return false,
    };

    let encrypted_index = (0..archive.len()).find(|&i| {
        archive
            .by_index_raw(i)
            .map(|entry| entry.encrypted() && !entry.is_dir())
            .unwrap_or(false)
    });

    let index = match encrypted_index {
        Some(i) => i,
        None => return false,
    };

    try_password_on_archive(&mut archive, index, password)
}

/// 尝试用给定密码打开 7z 文件。
///
/// 这里必须实际读取 payload，不能只依赖 `open()` 是否成功。
pub fn try_password_7z(file_path: &Path, password: &str) -> bool {
    match SevenZReader::open(file_path, Password::from(password)) {
        Ok(mut reader) => match validate_7z_payload(&mut reader) {
            Ok(true) => {
                if let Ok(mut empty_reader) = SevenZReader::open(file_path, Password::empty()) {
                    if let Ok(true) = validate_7z_payload(&mut empty_reader) {
                        return false;
                    }
                }
                true
            }
            _ => false,
        },
        Err(_) => false,
    }
}

/// 尝试用给定密码解密 RAR 文件中第一个加密条目。
pub fn try_password_rar(file_path: &Path, password: &str) -> bool {
    use unrar::Archive as RarArchive;

    let file_path_str = file_path.to_string_lossy().to_string();
    let open_result = RarArchive::with_password(&file_path_str, password).open_for_processing();

    let archive = match open_result {
        Ok(a) => a,
        Err(_) => return false,
    };

    let mut archive = archive;
    loop {
        let entry = match archive.read_header() {
            Ok(Some(entry)) => entry,
            Ok(None) => return false,
            Err(_) => return false,
        };

        let should_test = {
            let header = entry.entry();
            header.is_encrypted() && !header.is_directory()
        };

        if should_test {
            return entry.test().is_ok();
        }

        archive = match entry.skip() {
            Ok(next) => next,
            Err(_) => return false,
        };
    }
}

#[allow(dead_code)]
fn is_7z_password_error(error: &SevenZError) -> bool {
    matches!(
        error,
        SevenZError::PasswordRequired | SevenZError::MaybeBadPassword(_)
    )
}

fn validate_7z_payload<R: Read + Seek>(reader: &mut SevenZReader<R>) -> Result<bool, SevenZError> {
    let mut validated_any_file = false;

    reader.for_each_entries(|entry, entry_reader| {
        if entry.is_directory() || !entry.has_stream() {
            return Ok(true);
        }

        validated_any_file = true;
        std::io::copy(entry_reader, &mut std::io::sink())?;
        Ok(true)
    })?;

    Ok(validated_any_file)
}

/// 在恢复开始前，验证目标文件确实是加密的。
pub(crate) fn validate_recovery_target(
    path: &Path,
    archive_type: &ArchiveType,
) -> Result<(), String> {
    let is_encrypted = match archive_type {
        ArchiveType::Zip => archive_service::inspect_zip(path)?.is_encrypted,
        ArchiveType::SevenZ => archive_service::inspect_7z(path)?.is_encrypted,
        ArchiveType::Rar => archive_service::inspect_rar(path)?.is_encrypted,
        ArchiveType::Unknown => {
            return Err("未知的归档类型，无法进行密码恢复".to_string());
        }
    };

    if is_encrypted {
        Ok(())
    } else {
        Err("当前归档没有可恢复的加密内容".to_string())
    }
}
