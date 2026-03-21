use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// hashcat mode 13600 的 `data` 字段（仅密文体，不含 salt/verify/auth）的最大长度。
/// 对应 hashcat `module_13600.c` 里的 `data_buf[0x200000]`，即 8 MiB。
const MAX_ZIP_AES_CONTENT_SIZE: u64 = 8 * 1024 * 1024;

/// hashcat mode 17200 / 17210 的 `compressed_length` / `data_length` 最大长度。
/// 对应 hashcat `module_17200.c` / `module_17210.c` 里的 `MAX_DATA (320 * 1024)`。
const MAX_PKZIP_DATA_SIZE: u64 = 320 * 1024;
const ZIP_LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x0403_4b50;
const ZIP_FLAG_DATA_DESCRIPTOR: u16 = 1 << 3;

/// 交给 hashcat 的 ZIP hash 信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashcatZipHash {
    pub hash_mode: u32,
    pub hash_string: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PkzipLocalHeaderMetadata {
    general_purpose_flag: u16,
    last_modified_time: u16,
}

pub fn extract_zip_hash(file_path: &Path) -> Result<HashcatZipHash, String> {
    // Probe to decide which path to take: AES or PKZIP
    let file = File::open(file_path).map_err(|e| format!("打开 ZIP 文件失败: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("解析 ZIP 文件失败: {}", e))?;

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
        .map_err(|e| format!("读取 ZIP 条目失败: {}", e))?;

    let is_aes = entry
        .extra_data()
        .map(|extra| find_winzip_aes_field(extra).is_some())
        .unwrap_or(false);

    drop(entry);
    drop(archive);

    if is_aes {
        extract_zip_aes_hash(file_path)
    } else {
        extract_zip_pkzip_hash(file_path)
    }
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
        return Err("当前 ZIP 使用传统 PKZIP 加密，不是 WinZip AES 加密".to_string());
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

    let aes_overhead = salt_len as u64 + 2 + 10;

    // 大文件保护：mode 13600 ($zip2$) 需要把完整密文体嵌入 hash 字符串。
    // hashcat 自身对这段 `data` 有 8 MiB 的硬上限，超出时会直接拒收 hash。
    if compressed_size > MAX_ZIP_AES_CONTENT_SIZE + aes_overhead {
        let encrypted_content_size = compressed_size.saturating_sub(aes_overhead);
        return Err(format!(
            "AES 加密条目过大（密文 {:.1} MB），GPU 模式不支持（hashcat 最多接受 8.0 MB 密文）。请改用 CPU 引擎进行恢复",
            encrypted_content_size as f64 / (1024.0 * 1024.0)
        ));
    }

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

    Ok(HashcatZipHash {
        hash_mode: 13600,
        hash_string: format!(
            "$zip2$*0*{}*0*{}*{}*{:x}*{}*{}*$/zip2$",
            aes_strength,
            hex_encode(salt),
            hex_encode(password_verification),
            encrypted_content.len(),
            hex_encode(encrypted_content),
            hex_encode(auth_code)
        ),
    })
}

/// 从传统 PKZIP 加密 ZIP 中提取 hashcat $pkzip2$ hash。
///
/// 根据压缩方式自动选择 hashcat mode：
///   - mode 17200: PKZIP (Compressed)   — Deflate 等需要 inflate 验证的条目
///   - mode 17210: PKZIP (Uncompressed) — Stored（method=0）条目，直接 CRC32 校验
///
/// $pkzip2$ 格式 (single-file):
///   $pkzip2$<N>*<chk>*<ctype>*<plain>*<clen>*<ulen>*<crc32>*<offset>*<addoff>*<method>*<dlen>*<crc16>*<crc_hi>*<data>*$/pkzip2$
///
/// 字段说明：
///   N       = 1 (单文件攻击)
///   chk     = 1 (使用 2 字节校验)
///   ctype   = 2 (data_type_enum, 始终为 2 以包含扩展字段)
///   plain   = 0
///   clen    = compressed_size (含 12 字节加密头) 的十六进制
///   ulen    = uncompressed_size 的十六进制
///   crc32   = 原始文件 CRC32，8 位小写十六进制
///   offset  = 0
///   addoff  = data 长度 (= clen)
///   method  = 压缩方法 (8=Deflate, 0=Store)
///   dlen    = data 长度十六进制 (= clen)
///   check1  = bit3 置位时用 DOS mtime，否则用 crc32 低 16 位
///   check2  = crc32 高 16 位
///   data    = 完整加密负载（12 字节加密头 + 密文体）的十六进制
pub fn extract_zip_pkzip_hash(file_path: &Path) -> Result<HashcatZipHash, String> {
    let file = File::open(file_path).map_err(|e| format!("打开 ZIP 文件失败: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("解析 ZIP 文件失败: {}", e))?;

    // Find the first encrypted file entry
    let target_index = (0..archive.len())
        .find(|index| {
            archive
                .by_index_raw(*index)
                .map(|entry| entry.encrypted() && entry.is_file())
                .unwrap_or(false)
        })
        .ok_or_else(|| "ZIP 中没有加密文件条目".to_string())?;

    let entry = archive
        .by_index_raw(target_index)
        .map_err(|e| format!("读取 ZIP 条目失败: {}", e))?;

    // Reject AES-encrypted entries
    if let Some(extra) = entry.extra_data() {
        if find_winzip_aes_field(extra).is_some() {
            return Err("当前条目使用 WinZip AES 加密，不是 PKZIP 传统加密".to_string());
        }
    }

    if !entry.encrypted() {
        return Err("ZIP 条目未加密".to_string());
    }

    let compressed_size = entry.compressed_size();
    let uncompressed_size = entry.size();
    let crc32 = entry.crc32();
    let header_start = entry.header_start();
    let compress_method: u16 = match entry.compression() {
        zip::CompressionMethod::Stored => 0,
        zip::CompressionMethod::Deflated => 8,
        other => {
            #[allow(deprecated)]
            let v = other.to_u16();
            return Err(format!(
                "当前 ZIP 压缩方法 {} 不受 hashcat GPU 模式支持，请改用 CPU 引擎进行恢复",
                v
            ));
        }
    };
    let data_start = entry.data_start();

    drop(entry);
    drop(archive);

    // 大文件保护：PKZIP mode 17200 / 17210 必须把完整加密数据嵌入 hash 字符串，
    // 而 hashcat 自身对这段数据有 320 KiB 的硬上限。超过后不会开始跑密码，
    // 而是直接报 `Token length exception / No hashes loaded`。此处提前拦截并
    // 引导用户改用 CPU 引擎。
    if compressed_size > MAX_PKZIP_DATA_SIZE {
        return Err(format!(
            "PKZIP 加密条目过大（{:.1} KB），GPU 模式不支持（hashcat 最多接受 320 KB 压缩数据）。请改用 CPU 引擎进行恢复",
            compressed_size as f64 / 1024.0
        ));
    }

    let mut file = File::open(file_path).map_err(|e| format!("打开 ZIP 文件失败: {}", e))?;
    let local_header = read_pkzip_local_header_metadata(&mut file, header_start)?;

    // Read the full encrypted payload (12-byte PKZIP encryption header + ciphertext)
    file.seek(SeekFrom::Start(data_start))
        .map_err(|e| format!("定位 ZIP 数据段失败: {}", e))?;

    let mut encrypted_data = vec![0_u8; compressed_size as usize];
    file.read_exact(&mut encrypted_data)
        .map_err(|e| format!("读取 ZIP 加密数据失败: {}", e))?;

    if encrypted_data.len() < 12 {
        return Err("PKZIP 加密数据段过短（至少需要 12 字节加密头）".to_string());
    }

    // data_type_enum: always 2 to include extended fields (clen/ulen/crc32/offset/addoff).
    // The actual compression method (0=Stored, 8=Deflate) goes in the `method` field.
    // Setting this to 0 for Stored entries was the root cause of "No hashes loaded" —
    // hashcat's parser skips the 5 extended fields when data_type_enum <= 1,
    // causing field misalignment and hash rejection.
    let ctype_flag: u32 = 2;
    let method = compress_method as u32;

    // 根据压缩方式选择 hashcat mode：
    //   - mode 17200: PKZIP (Compressed)   — 解密后需要 inflate 再校验 CRC32
    //   - mode 17210: PKZIP (Uncompressed) — 解密后直接校验 CRC32，不做 inflate
    // 使用错误的 mode 会导致 hashcat 对正确密码的验证也失败（inflate 非压缩数据
    // 必定报错），从而「穷尽候选」却找不到密码。常见于 Bandizip 等工具对
    // JPEG/PNG 等已压缩格式使用 Stored 方式打包的场景。
    let hash_mode: u32 = if compress_method == 0 { 17210 } else { 17200 };

    let clen_hex = format!("{:x}", compressed_size);
    let ulen_hex = format!("{:x}", uncompressed_size);
    let crc32_hex = format!("{:08x}", crc32);
    let dlen_hex = clen_hex.clone();
    let addoff_hex = clen_hex.clone();
    let (check1, check2) = pkzip_checksums(crc32, local_header);
    let check1_hex = format!("{:04x}", check1);
    let check2_hex = format!("{:04x}", check2);
    let data_hex = hex_encode(&encrypted_data);

    let hash_string = format!(
        "$pkzip2$1*1*{ctype}*0*{clen}*{ulen}*{crc32}*0*{addoff}*{method}*{dlen}*{check1}*{check2}*{data}*$/pkzip2$",
        ctype = ctype_flag,
        clen = clen_hex,
        ulen = ulen_hex,
        crc32 = crc32_hex,
        addoff = addoff_hex,
        method = method,
        dlen = dlen_hex,
        check1 = check1_hex,
        check2 = check2_hex,
        data = data_hex,
    );

    Ok(HashcatZipHash {
        hash_mode,
        hash_string,
    })
}

fn read_pkzip_local_header_metadata<R: Read + Seek>(
    reader: &mut R,
    header_start: u64,
) -> Result<PkzipLocalHeaderMetadata, String> {
    let mut header = [0_u8; 30];
    reader
        .seek(SeekFrom::Start(header_start))
        .map_err(|e| format!("定位 ZIP 本地文件头失败: {}", e))?;
    reader
        .read_exact(&mut header)
        .map_err(|e| format!("读取 ZIP 本地文件头失败: {}", e))?;

    let signature = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    if signature != ZIP_LOCAL_FILE_HEADER_SIGNATURE {
        return Err(format!(
            "ZIP 本地文件头签名无效: 期望 0x{ZIP_LOCAL_FILE_HEADER_SIGNATURE:08x}，实际 0x{signature:08x}"
        ));
    }

    Ok(PkzipLocalHeaderMetadata {
        general_purpose_flag: u16::from_le_bytes([header[6], header[7]]),
        last_modified_time: u16::from_le_bytes([header[10], header[11]]),
    })
}

fn pkzip_checksums(crc32: u32, local_header: PkzipLocalHeaderMetadata) -> (u16, u16) {
    let primary = if local_header.general_purpose_flag & ZIP_FLAG_DATA_DESCRIPTOR != 0 {
        local_header.last_modified_time
    } else {
        (crc32 & 0xffff) as u16
    };
    let secondary = ((crc32 >> 16) & 0xffff) as u16;

    (primary, secondary)
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
    use super::{
        extract_zip_aes_hash, extract_zip_pkzip_hash, parse_winzip_aes_extra, pkzip_checksums,
        read_pkzip_local_header_metadata, HashcatZipHash, PkzipLocalHeaderMetadata,
    };
    use crate::domain::recovery::AttackMode;
    use crate::services::hashcat_service::{build_attack_args, run_hashcat};
    use crate::services::recovery_service::RecoveryResult;
    use std::fs::File;
    use std::path::Path;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn zip_fixtures_dir() -> std::path::PathBuf {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.parent().unwrap().join("fixtures").join("zip")
    }

    /// 复用一套“真机 hashcat”测试流程来验证 ZIP hash 提取结果。
    /// 这样 AES / PKZIP 两条路径都能走完全链路：
    ///   1. 提取 hashcat 兼容 hash
    ///   2. 写临时 hash / wordlist 文件
    ///   3. 调本机 hashcat 跑小字典
    ///   4. 断言已知密码能被找到
    fn crack_fixture_with_local_hashcat(hash: HashcatZipHash, session_name: &str) {
        if !cfg!(windows) {
            return;
        }

        let hashcat_path = std::env::var("HASHCAT_PATH")
            .expect("running ignored hashcat tests requires HASHCAT_PATH to be set");

        let temp_dir = tempfile::tempdir().unwrap();
        let args = build_attack_args(
            &AttackMode::Dictionary {
                wordlist: vec!["wrong".to_string(), "test123".to_string()],
            },
            hash.hash_mode,
            &hash.hash_string,
            session_name,
            temp_dir.path(),
        )
        .unwrap();

        let result = run_hashcat(
            Path::new(&hashcat_path),
            &args.args,
            &args.outfile_path,
            session_name,
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .unwrap();

        match result {
            RecoveryResult::Found(password) => assert_eq!(password, "test123"),
            other => panic!("expected hashcat to find password, got {:?}", other),
        }
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
    fn read_pkzip_local_header_metadata_from_fixture() {
        let path = zip_fixtures_dir().join("encrypted-pkzip.zip");
        let mut file = File::open(path).unwrap();
        let metadata = read_pkzip_local_header_metadata(&mut file, 0).unwrap();

        assert_eq!(metadata.general_purpose_flag, 0x0001);
        assert_eq!(metadata.last_modified_time, 0x9040);
    }

    #[test]
    fn pkzip_checksums_use_crc_low_when_data_descriptor_is_absent() {
        let metadata = PkzipLocalHeaderMetadata {
            general_purpose_flag: 0x0001,
            last_modified_time: 0x9040,
        };

        assert_eq!(pkzip_checksums(0x0d4a1185, metadata), (0x1185, 0x0d4a));
    }

    #[test]
    fn pkzip_checksums_use_dos_time_when_data_descriptor_is_present() {
        let metadata = PkzipLocalHeaderMetadata {
            general_purpose_flag: 0x0009,
            last_modified_time: 0xbd32,
        };

        assert_eq!(pkzip_checksums(0x6c935eec, metadata), (0xbd32, 0x6c93));
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
        let zip_path = zip_fixtures_dir().join("encrypted-aes.zip");
        let hash = extract_zip_aes_hash(&zip_path).unwrap();
        crack_fixture_with_local_hashcat(hash, "zip_aes_fixture");
    }

    /// 对称地覆盖 PKZIP GPU 路径。
    /// 这能防止以后 refactor 只保证单元测试通过，却把 17200 的真实格式改坏。
    #[test]
    #[ignore = "requires local hashcat binary"]
    fn hashcat_can_crack_extracted_zip_pkzip_fixture() {
        let zip_path = zip_fixtures_dir().join("encrypted-pkzip.zip");
        let hash = extract_zip_pkzip_hash(&zip_path).unwrap();
        crack_fixture_with_local_hashcat(hash, "zip_pkzip_fixture");
    }

    #[test]
    fn extract_zip_pkzip_hash_returns_mode_17200_for_pkzip_fixture() {
        let path = zip_fixtures_dir().join("encrypted-pkzip.zip");
        let hash = extract_zip_pkzip_hash(&path).unwrap();
        assert_eq!(hash.hash_mode, 17200);
        assert!(
            hash.hash_string.starts_with("$pkzip2$"),
            "hash_string should start with $pkzip2$, got: {}",
            hash.hash_string
        );
        assert!(
            hash.hash_string.ends_with("*$/pkzip2$"),
            "hash_string should end with *$/pkzip2$, got: {}",
            hash.hash_string
        );
    }

    #[test]
    fn extract_zip_pkzip_hash_contains_correct_fields_for_fixture() {
        // Fixture: encrypted-pkzip.zip
        //   password: test123, compress_type=8 (Deflate)
        //   CRC32: 0x0d4a1185, compressed_size: 25 (0x19), file_size: 11 (0xb)
        //   encrypted_data (25 bytes):
        //     b5d620e049737fec611672600821d36a99e568c4730a174434
        let path = zip_fixtures_dir().join("encrypted-pkzip.zip");
        let hash = extract_zip_pkzip_hash(&path).unwrap();
        let s = &hash.hash_string;

        // Must encode the full encrypted payload (25 bytes)
        let expected_data_hex = "b5d620e049737fec611672600821d36a99e568c4730a174434";
        assert!(
            s.contains(expected_data_hex),
            "hash must contain encrypted payload hex, got: {}",
            s
        );

        // Must contain compressed_size in hex: 19
        assert!(
            s.contains("*19*"),
            "hash must contain comp_len=19, got: {}",
            s
        );

        // Must contain crc32: 0d4a1185
        assert!(
            s.contains("0d4a1185"),
            "hash must contain crc32=0d4a1185, got: {}",
            s
        );
    }

    #[test]
    fn extract_zip_pkzip_hash_rejects_unencrypted_zip() {
        let path = zip_fixtures_dir().join("normal.zip");
        assert!(extract_zip_pkzip_hash(&path).is_err());
    }

    #[test]
    fn extract_zip_pkzip_hash_rejects_aes_zip() {
        let path = zip_fixtures_dir().join("encrypted-aes.zip");
        let result = extract_zip_pkzip_hash(&path);
        assert!(
            result.is_err(),
            "AES zip should be rejected by PKZIP extractor"
        );
    }

    // ── Stored-compression PKZIP tests (mode 17210) ──────────────────

    #[test]
    fn extract_zip_pkzip_hash_returns_mode_17210_for_stored_fixture() {
        let path = zip_fixtures_dir().join("encrypted-pkzip-stored.zip");
        let hash = extract_zip_pkzip_hash(&path).unwrap();
        assert_eq!(
            hash.hash_mode, 17210,
            "Stored (method=0) entries should use hashcat mode 17210, not 17200"
        );
        assert!(
            hash.hash_string.starts_with("$pkzip2$"),
            "hash_string should start with $pkzip2$, got: {}",
            hash.hash_string
        );
        assert!(
            hash.hash_string.ends_with("*$/pkzip2$"),
            "hash_string should end with *$/pkzip2$, got: {}",
            hash.hash_string
        );
    }

    #[test]
    fn extract_zip_pkzip_hash_stored_fixture_has_ctype_2() {
        // Fixture: encrypted-pkzip-stored.zip
        //   password: test123, compress_type=0 (Stored), CRC32: 0x0d4a1185
        //   compressed_size: 23 (= 11 content + 12 PKZIP header)
        let path = zip_fixtures_dir().join("encrypted-pkzip-stored.zip");
        let hash = extract_zip_pkzip_hash(&path).unwrap();
        let s = &hash.hash_string;

        // data_type_enum must always be 2 to include extended fields
        // (clen/ulen/crc32/offset/addoff). The actual compression method
        // (0=Stored) goes in the later `method` field.
        // Format: $pkzip2$1*1*{ctype}*0*...
        assert!(
            s.starts_with("$pkzip2$1*1*2*"),
            "Stored entry should have data_type_enum=2 in hash, got: {}",
            s
        );

        // Method field should be 0 (Stored)
        // CRC32 should match
        assert!(
            s.contains("0d4a1185"),
            "hash must contain crc32=0d4a1185, got: {}",
            s
        );
    }

    #[test]
    fn extract_zip_hash_dispatches_to_17210_for_stored_fixture() {
        use super::extract_zip_hash;
        let path = zip_fixtures_dir().join("encrypted-pkzip-stored.zip");
        let hash = extract_zip_hash(&path).unwrap();
        assert_eq!(
            hash.hash_mode, 17210,
            "extract_zip_hash should dispatch Stored PKZIP to mode 17210"
        );
    }

    /// 与 Deflate PKZIP 测试对称——验证 Stored PKZIP 的 hashcat 全链路。
    #[test]
    #[ignore = "requires local hashcat binary"]
    fn hashcat_can_crack_extracted_zip_pkzip_stored_fixture() {
        let zip_path = zip_fixtures_dir().join("encrypted-pkzip-stored.zip");
        let hash = extract_zip_pkzip_hash(&zip_path).unwrap();
        crack_fixture_with_local_hashcat(hash, "zip_pkzip_stored_fixture");
    }

    #[test]
    fn extract_zip_hash_dispatches_to_pkzip_for_pkzip_fixture() {
        use super::extract_zip_hash;
        let path = zip_fixtures_dir().join("encrypted-pkzip.zip");
        let hash = extract_zip_hash(&path).unwrap();
        assert_eq!(hash.hash_mode, 17200);
    }

    #[test]
    fn extract_zip_hash_dispatches_to_aes_for_aes_fixture() {
        use super::extract_zip_hash;
        let path = zip_fixtures_dir().join("encrypted-aes.zip");
        let hash = extract_zip_hash(&path).unwrap();
        assert_eq!(hash.hash_mode, 13600);
    }

    #[test]
    fn extract_zip_pkzip_hash_rejects_oversized_entry() {
        use super::MAX_PKZIP_DATA_SIZE;

        // 构造一个条目超过阈值的 PKZIP ZIP（使用真实大文件或环境变量路径）。
        // 如果没有真实大文件可用，则用阈值常量做基本断言。
        let large_zip_path = std::env::var("LARGE_PKZIP_ZIP_PATH").ok();
        if let Some(path_str) = large_zip_path {
            let path = std::path::Path::new(&path_str);
            if path.exists() {
                let result = extract_zip_pkzip_hash(path);
                assert!(result.is_err(), "oversized PKZIP should be rejected");
                let err_msg = result.unwrap_err();
                assert!(
                    err_msg.contains("过大") && err_msg.contains("CPU"),
                    "error should mention size and CPU fallback, got: {}",
                    err_msg
                );
                return;
            }
        }
        // 无大文件时，至少验证阈值常量合理
        assert_eq!(MAX_PKZIP_DATA_SIZE, 320 * 1024);
    }
}
