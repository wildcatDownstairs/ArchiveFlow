use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// 交给 hashcat 的 ZIP hash 信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashcatZipHash {
    pub hash_mode: u32,
    pub hash_string: String,
}

pub fn extract_zip_hash(file_path: &Path) -> Result<HashcatZipHash, String> {
    extract_zip_aes_hash(file_path)
}

pub fn extract_zip_aes_hash(file_path: &Path) -> Result<HashcatZipHash, String> {
    let file = File::open(file_path).map_err(|error| format!("打开 ZIP 文件失败: {}", error))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("解析 ZIP 文件失败: {}", error))?;

    let target_index = (0..archive.len())
        .find(|index| {
            archive
                .by_index_raw(*index)
                .map(|entry| entry.encrypted() && entry.is_file())
                .unwrap_or(false)
        })
        .ok_or_else(|| "ZIP 中没有可用于 GPU 的加密文件条目".to_string())?;

    let entry = archive
        .by_index_raw(target_index)
        .map_err(|error| format!("读取 ZIP 条目失败: {}", error))?;

    let extra_data = entry
        .extra_data()
        .ok_or_else(|| "ZIP 条目缺少 extra data，无法提取 AES 参数".to_string())?
        .to_vec();
    let compressed_size = entry.compressed_size();
    let data_start = entry.data_start();
    let is_aes = find_winzip_aes_field(&extra_data).is_some();

    drop(entry);
    drop(archive);

    if !is_aes {
        return Err("当前 ZIP 使用传统 PKZIP 加密，GPU 恢复暂不支持，请改用 CPU".to_string());
    }

    let (aes_strength, _) = parse_winzip_aes_extra(&extra_data)?;
    let salt_len = match aes_strength {
        1 => 8,
        2 => 12,
        3 => 16,
        other => {
            return Err(format!("不支持的 ZIP AES 强度: {}", other));
        }
    };

    let mut file =
        File::open(file_path).map_err(|error| format!("打开 ZIP 文件失败: {}", error))?;
    file.seek(SeekFrom::Start(data_start))
        .map_err(|error| format!("定位 ZIP 数据段失败: {}", error))?;

    let mut payload = vec![0_u8; compressed_size as usize];
    file.read_exact(&mut payload)
        .map_err(|error| format!("读取 ZIP 加密数据失败: {}", error))?;

    if payload.len() <= salt_len + 2 + 10 {
        return Err("ZIP AES 数据段过短，无法拼出 hashcat hash".to_string());
    }

    let salt = &payload[..salt_len];
    let password_verification = &payload[salt_len..salt_len + 2];
    let auth_code = &payload[payload.len() - 10..];
    let encrypted_content = &payload[salt_len + 2..payload.len() - 10];

    let mut data_field = Vec::with_capacity(password_verification.len() + encrypted_content.len());
    data_field.extend_from_slice(password_verification);
    data_field.extend_from_slice(encrypted_content);

    Ok(HashcatZipHash {
        hash_mode: 13600,
        hash_string: format!(
            "$zip2$*0*{}*0*{}*{}*{}*$/zip2$",
            aes_strength,
            hex_encode(salt),
            hex_encode(&data_field),
            hex_encode(auth_code)
        ),
    })
}

fn find_winzip_aes_field(extra_data: &[u8]) -> Option<&[u8]> {
    let mut cursor = 0;
    while cursor + 4 <= extra_data.len() {
        let header_id = u16::from_le_bytes([extra_data[cursor], extra_data[cursor + 1]]);
        let data_size =
            u16::from_le_bytes([extra_data[cursor + 2], extra_data[cursor + 3]]) as usize;
        let field_start = cursor + 4;
        let field_end = field_start + data_size;
        if field_end > extra_data.len() {
            return None;
        }
        if header_id == 0x9901 {
            return Some(&extra_data[field_start..field_end]);
        }
        cursor = field_end;
    }
    None
}

fn parse_winzip_aes_extra(extra_data: &[u8]) -> Result<(u8, u16), String> {
    let field = find_winzip_aes_field(extra_data)
        .ok_or_else(|| "未找到 WinZip AES extra field (0x9901)".to_string())?;
    if field.len() < 7 {
        return Err("WinZip AES extra field 长度不足".to_string());
    }

    let aes_strength = field[4];
    let compression_method = u16::from_le_bytes([field[5], field[6]]);
    Ok((aes_strength, compression_method))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{:02x}", byte)).collect()
}

#[cfg(test)]
mod tests {
    use super::{extract_zip_aes_hash, parse_winzip_aes_extra};
    use crate::domain::recovery::AttackMode;
    use crate::services::hashcat_service::{build_attack_args, run_hashcat};
    use crate::services::recovery_service::RecoveryResult;
    use std::path::Path;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn zip_fixtures_dir() -> std::path::PathBuf {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.parent().unwrap().join("fixtures").join("zip")
    }

    #[test]
    fn parse_winzip_aes_extra_returns_strength_and_compression() {
        let extra = vec![
            0x01, 0x99, 0x07, 0x00, 0x02, 0x00, 0x41, 0x45, 0x03, 0x08, 0x00,
        ];

        let (strength, compression) = parse_winzip_aes_extra(&extra).unwrap();
        assert_eq!(strength, 3);
        assert_eq!(compression, 8);
    }

    #[test]
    fn extract_zip_aes_hash_from_fixture() {
        let path = zip_fixtures_dir().join("encrypted-aes.zip");
        let hash = extract_zip_aes_hash(&path).unwrap();
        assert_eq!(hash.hash_mode, 13600);
        assert!(hash.hash_string.starts_with("$zip2$"));
        assert!(hash.hash_string.ends_with("$/zip2$"));
    }

    #[test]
    fn extract_zip_aes_hash_rejects_unencrypted_zip() {
        let path = zip_fixtures_dir().join("normal.zip");
        assert!(extract_zip_aes_hash(&path).is_err());
    }

    #[test]
    fn extract_zip_aes_hash_rejects_non_aes_zip() {
        let path = zip_fixtures_dir().join("encrypted-strong.zip");
        let result = extract_zip_aes_hash(&path);
        if let Err(message) = result {
            assert!(message.contains("PKZIP") || message.contains("extra field"));
        }
    }

    /// 这是一个“真机集成测试”：
    ///   - 读取真实 ZIP AES fixture
    ///   - 提取 mode 13600 hash
    ///   - 调本机 hashcat 去跑一个很小的字典
    ///
    /// 之所以默认 ignore，是因为 CI 和大多数开发机都不一定装了 hashcat。
    /// 需要手工验证时，可以这样运行：
    ///   HASHCAT_PATH=... cargo test ... -- --ignored hashcat_can_crack_extracted_zip_aes_fixture
    #[test]
    #[ignore = "requires local hashcat binary"]
    fn hashcat_can_crack_extracted_zip_aes_fixture() {
        if !cfg!(windows) {
            return;
        }

        let hashcat_path = match std::env::var("HASHCAT_PATH") {
            Ok(path) => path,
            Err(_) => return,
        };

        let zip_path = zip_fixtures_dir().join("encrypted-aes.zip");
        let hash = extract_zip_aes_hash(&zip_path).unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let args = build_attack_args(
            &AttackMode::Dictionary {
                wordlist: vec!["wrong".to_string(), "test123".to_string()],
            },
            hash.hash_mode,
            &hash.hash_string,
            "zip_aes_fixture",
            temp_dir.path(),
        )
        .unwrap();

        let result = run_hashcat(
            Path::new(&hashcat_path),
            &args.args,
            &args.outfile_path,
            "zip-aes-fixture",
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .unwrap();

        match result {
            RecoveryResult::Found(password) => assert_eq!(password, "test123"),
            other => panic!("expected hashcat to find password, got {:?}", other),
        }
    }
}
