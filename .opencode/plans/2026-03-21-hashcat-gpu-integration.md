# Hashcat GPU Integration Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Integrate hashcat as an external GPU-accelerated password recovery backend, starting with ZIP (AES + PKZIP), so ArchiveFlow can offload brute-force/mask/dictionary attacks to the user's GPU for 10-100x speedup.

**Architecture:** ArchiveFlow keeps its existing CPU recovery engine unchanged. A new parallel code path extracts hashcat-compatible hash strings from archive files in pure Rust, launches hashcat as a subprocess, parses its `--status-json` output into the existing `RecoveryProgress` event stream, and maps its exit codes to `RecoveryResult`. The frontend sees no difference — same progress events, same result states.

**Tech Stack:** Rust (hash extraction + subprocess management), hashcat CLI (`--status-json`), existing Tauri event system, existing `zip` crate for binary header parsing.

---

## Architecture Overview

```
+------------------- recovery_commands.rs -------------------+
|  dispatch_scheduled_recoveries()                           |
|       |                                                    |
|       v                                                    |
|  spawn_recovery_worker()                                   |
|       |                                                    |
|  +----+------------------------------------+               |
|  |  backend == Cpu?                        |               |
|  |  YES -> run_recovery()                  | <- existing   |
|  |  NO  -> run_gpu_recovery()              | <- NEW        |
|  +-----------------------------------------+               |
|       | (GPU path)                                         |
|       +-- extract_hash()          <- Task 2-3 (Rust)       |
|       +-- build_hashcat_args()    <- Task 4                |
|       +-- spawn hashcat process   <- Task 5                |
|       +-- poll --status-json      <- Task 5                |
|       |    +-> emit "recovery-progress"                    |
|       +-- map exit code           <- Task 5                |
|            +-> RecoveryResult                              |
+------------------------------------------------------------+
```

## Key Design Decisions

1. **CPU engine stays untouched** — GPU is an additive backend, not a replacement.
2. **Hash extraction in Rust** — No external `zip2john`/`7z2hashcat` dependency. Read binary headers directly.
3. **Hashcat as subprocess** — hashcat has no library API. Use `--status --status-json --status-timer=1`.
4. **Same RecoveryProgress event** — Frontend doesn't know/care if it's CPU or GPU. Zero frontend changes for V1.
5. **User installs hashcat** — V1 detects PATH or accepts manual path in settings. No bundling.
6. **Backend selection** — Auto-detect hashcat availability + GPU support. User can override per-task.
7. **Cancellation** — Monitoring thread checks existing `AtomicBool` cancel flag, kills hashcat process when set.

---

## Hashcat Reference

### Status JSON fields (from `src/terminal.c:2877-3051`)

```json
{
  "session": "session_name",
  "status": 3,
  "target": "$zip2$*0*3*0*...",
  "progress": [current, total],
  "recovered_hashes": [cracked, total],
  "devices": [
    { "device_id": 1, "device_name": "NVIDIA RTX 4080", "device_type": "GPU", "speed": 123456789, "temp": 65, "util": 98 }
  ],
  "time_start": 1711000000,
  "estimated_stop": 1711000300
}
```

### Status codes (`types.h`)
- 0=Init, 1=Autotune, 2=Selftest, **3=Running**, 4=Paused, **5=Exhausted**, **6=Cracked**, 7=Aborted

### Exit codes
- **0** = Cracked (password found)
- **1** = Exhausted (all tried, not found)
- **2** = Aborted by user
- **-1/255** = Error

### Relevant hash modes

| Mode | Format | Algorithm |
|------|--------|-----------|
| 13600 | `$zip2$*type*mode*magic*salt*data*auth*$/zip2$` | WinZip AES |
| 17200 | `$pkzip2$N*...` | PKZIP Compressed |
| 17210 | `$pkzip2$N*...` | PKZIP Uncompressed |
| 17220 | `$pkzip2$N*...` | PKZIP Compressed Multi-File |
| 17225 | `$pkzip2$N*...` | PKZIP Mixed Multi-File |
| 17230 | `$pkzip2$N*...` | PKZIP Uncompressed Multi-File |
| 11600 | `$7z$type$cost$salt_len$salt$iv_len$iv$crc$data_len$dec_len$data` | 7-Zip AES-256 |

### Attack mode mapping

| ArchiveFlow AttackMode | hashcat `-a` | Notes |
|---|---|---|
| `Dictionary { wordlist }` | `-a 0 wordlist_file` | Write `Vec<String>` to temp file |
| `BruteForce { charset, min, max }` | `-a 3 -1 <charset> -i --increment-min=N --increment-max=M ?1?1...` | Custom charset via `-1`, mask of `?1` repeated `max_length` times |
| `Mask { mask }` | `-a 3 <mask>` | Translate `??` -> literal `?`, `?l/?u/?d/?s/?a` -> same |

---

## Task Breakdown

### Task 1: Hashcat Detection & Configuration

**Files:**
- Create: `src-tauri/src/services/hashcat_service.rs`
- Modify: `src-tauri/src/services/mod.rs` (add `pub mod hashcat_service;`)
- Modify: `src-tauri/src/commands/recovery_commands.rs` (add settings commands)
- Test: inline `#[cfg(test)]` module in `hashcat_service.rs`

**What this does:**
Detect hashcat installation, verify it works, expose GPU device info. This is the foundation -- every subsequent task depends on being able to find and call hashcat.

**Step 1: Create `hashcat_service.rs` with detection logic**

```rust
// src-tauri/src/services/hashcat_service.rs

use std::path::{Path, PathBuf};
use std::process::Command;
use serde::{Deserialize, Serialize};

/// hashcat executable detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashcatInfo {
    pub path: PathBuf,
    pub version: String,
    pub devices: Vec<GpuDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDevice {
    pub id: u32,
    pub name: String,
    pub device_type: String, // "GPU", "CPU", "Accelerator"
}

/// Detect hashcat in system PATH or at a user-specified path
pub fn detect_hashcat(custom_path: Option<&Path>) -> Result<HashcatInfo, String> {
    let hashcat_path = match custom_path {
        Some(p) if p.exists() => p.to_path_buf(),
        Some(p) => return Err(format!("hashcat path does not exist: {}", p.display())),
        None => find_in_path()?,
    };

    let version = get_version(&hashcat_path)?;
    let devices = get_devices(&hashcat_path)?;

    Ok(HashcatInfo {
        path: hashcat_path,
        version,
        devices,
    })
}

fn find_in_path() -> Result<PathBuf, String> {
    let candidates = if cfg!(windows) {
        vec!["hashcat.exe", "hashcat"]
    } else {
        vec!["hashcat"]
    };
    for name in candidates {
        // Windows: use `where`
        if cfg!(windows) {
            if let Ok(output) = Command::new("where").arg(name).output() {
                if output.status.success() {
                    let path_str = String::from_utf8_lossy(&output.stdout);
                    if let Some(first_line) = path_str.lines().next() {
                        let p = PathBuf::from(first_line.trim());
                        if p.exists() {
                            return Ok(p);
                        }
                    }
                }
            }
        }
        // Unix: use `which`
        if !cfg!(windows) {
            if let Ok(output) = Command::new("which").arg(name).output() {
                if output.status.success() {
                    let path_str = String::from_utf8_lossy(&output.stdout);
                    if let Some(first_line) = path_str.lines().next() {
                        return Ok(PathBuf::from(first_line.trim()));
                    }
                }
            }
        }
    }
    Err("hashcat not found in PATH. Install hashcat or specify path in settings.".to_string())
}

fn get_version(hashcat_path: &Path) -> Result<String, String> {
    let output = Command::new(hashcat_path)
        .arg("--version")
        .output()
        .map_err(|e| format!("Failed to run hashcat: {}", e))?;

    if !output.status.success() {
        return Err("hashcat --version failed".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn get_devices(hashcat_path: &Path) -> Result<Vec<GpuDevice>, String> {
    let output = Command::new(hashcat_path)
        .args(["--backend-info", "--quiet"])
        .output()
        .map_err(|e| format!("Failed to get GPU device list: {}", e))?;

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_backend_info(&text))
}

fn parse_backend_info(text: &str) -> Vec<GpuDevice> {
    let mut devices = Vec::new();
    let mut current_id: Option<u32> = None;
    let mut current_name: Option<String> = None;
    let mut current_type: Option<String> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Backend Device ID #") {
            if let (Some(id), Some(name), Some(dtype)) =
                (current_id, current_name.take(), current_type.take())
            {
                devices.push(GpuDevice { id, name, device_type: dtype });
            }
            current_id = rest.split_whitespace().next()
                .and_then(|s| s.parse().ok());
        } else if let Some(rest) = trimmed.strip_prefix("Name") {
            if let Some(name) = rest.split(':').nth(1) {
                current_name = Some(name.trim().to_string());
            }
        } else if let Some(rest) = trimmed.strip_prefix("Type") {
            if let Some(t) = rest.split(':').nth(1) {
                current_type = Some(t.trim().to_string());
            }
        }
    }
    if let (Some(id), Some(name), Some(dtype)) =
        (current_id, current_name, current_type)
    {
        devices.push(GpuDevice { id, name, device_type: dtype });
    }
    devices
}
```

**Step 2: Register module**

In `src-tauri/src/services/mod.rs`, add:
```rust
pub mod hashcat_service;
```

**Step 3: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_backend_info_extracts_devices() {
        let sample = "\nBackend Device ID #1 (Alias: #1)\n  Name...........: NVIDIA GeForce RTX 4080\n  Type...........: GPU\n\nBackend Device ID #2 (Alias: #2)\n  Name...........: Intel(R) UHD Graphics 770\n  Type...........: GPU\n";
        let devices = parse_backend_info(sample);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].id, 1);
        assert!(devices[0].name.contains("RTX 4080"));
        assert_eq!(devices[0].device_type, "GPU");
        assert_eq!(devices[1].id, 2);
    }

    #[test]
    fn parse_backend_info_empty_input() {
        let devices = parse_backend_info("");
        assert!(devices.is_empty());
    }

    #[test]
    fn find_in_path_returns_error_when_not_installed() {
        let result = detect_hashcat(Some(Path::new("/nonexistent/hashcat")));
        assert!(result.is_err());
    }
}
```

**Step 4: Run tests**

Run: `cargo test hashcat -- --nocapture` (from `src-tauri`)
Expected: All 3 tests pass.

**Step 5: Commit**

```
feat(gpu): add hashcat detection and GPU device discovery
```

---

### Task 2: ZIP AES Hash Extraction (Mode 13600)

**Files:**
- Create: `src-tauri/src/services/hash_extraction.rs`
- Modify: `src-tauri/src/services/mod.rs` (add `pub mod hash_extraction;`)
- Test: inline `#[cfg(test)]` module in `hash_extraction.rs`

**What this does:**
Read a ZIP file's binary headers to extract the WinZip AES encryption parameters and format them as a hashcat-compatible `$zip2$` string. This is pure binary parsing -- no decryption, no external tools.

**Background: WinZip AES ZIP structure**

A WinZip AES-encrypted ZIP entry has:
- Local file header with extra field ID `0x9901` (WinZip AES extra data)
- Extra field contains: vendor version (2 bytes), vendor ID "AE" (2 bytes), AES strength (1 byte: 1=128, 2=192, 3=256), compression method (2 bytes)
- Encrypted data block: salt (8/12/16 bytes) + password verification value (2 bytes) + encrypted content + authentication code (10 bytes HMAC-SHA1)

Hashcat `$zip2$` format:
```
$zip2$*type*mode*magic*salt_hex*data_hex*auth_hex*$/zip2$
```
Where:
- `type` = 0 (always)
- `mode` = AES strength (1=128bit, 2=192bit, 3=256bit)
- `magic` = 0 (reserved)
- `salt_hex` = hex-encoded salt bytes
- `data_hex` = hex-encoded compressed+encrypted data (including 2-byte password verification value at start)
- `auth_hex` = hex-encoded 10-byte authentication code

**Step 1: Create `hash_extraction.rs`**

```rust
// src-tauri/src/services/hash_extraction.rs

use std::path::Path;

/// Extract a hashcat-compatible hash string from a ZIP file.
/// Returns (hash_string, hashcat_mode).
pub fn extract_zip_hash(file_path: &Path) -> Result<(String, u32), String> {
    let file = std::fs::File::open(file_path)
        .map_err(|e| format!("Cannot open file: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Cannot parse ZIP: {}", e))?;

    // Find first encrypted non-directory entry
    let encrypted_index = (0..archive.len())
        .find(|&i| {
            archive.by_index_raw(i)
                .map(|entry| entry.encrypted() && !entry.is_dir())
                .unwrap_or(false)
        })
        .ok_or_else(|| "No encrypted entries found".to_string())?;

    let entry = archive.by_index_raw(encrypted_index)
        .map_err(|e| format!("Cannot read entry: {}", e))?;

    // AES-encrypted entries use compression method 99 (WinZip AES marker)
    let is_aes = entry.compression() == zip::CompressionMethod::Unsupported(99);

    drop(entry);
    drop(archive);

    if is_aes {
        extract_zip_aes_hash(file_path)
    } else {
        extract_zip_pkzip_hash(file_path)
    }
}

fn extract_zip_aes_hash(file_path: &Path) -> Result<(String, u32), String> {
    let data = std::fs::read(file_path)
        .map_err(|e| format!("Cannot read file: {}", e))?;

    let mut offset = 0usize;
    while offset + 30 <= data.len() {
        if &data[offset..offset + 4] != b"\x50\x4b\x03\x04" {
            offset += 1;
            continue;
        }

        let compressed_method = u16::from_le_bytes([data[offset + 8], data[offset + 9]]);
        let compressed_size = u32::from_le_bytes([
            data[offset + 18], data[offset + 19],
            data[offset + 20], data[offset + 21],
        ]) as usize;
        let filename_len = u16::from_le_bytes([data[offset + 26], data[offset + 27]]) as usize;
        let extra_len = u16::from_le_bytes([data[offset + 28], data[offset + 29]]) as usize;

        let header_end = offset + 30 + filename_len + extra_len;
        if header_end > data.len() { break; }

        if compressed_method == 99 {
            let extra_start = offset + 30 + filename_len;
            let (aes_strength, _actual_compression) =
                parse_winzip_aes_extra(&data[extra_start..extra_start + extra_len])?;

            let salt_len = match aes_strength {
                1 => 8,
                2 => 12,
                3 => 16,
                _ => return Err(format!("Unknown AES strength: {}", aes_strength)),
            };

            let enc_data_start = header_end;
            let enc_data_end = enc_data_start + compressed_size;
            if enc_data_end > data.len() {
                return Err("Incomplete file data".to_string());
            }

            let salt = &data[enc_data_start..enc_data_start + salt_len];
            let pvv = &data[enc_data_start + salt_len..enc_data_start + salt_len + 2];
            let auth_code = &data[enc_data_end - 10..enc_data_end];
            let encrypted_content = &data[enc_data_start + salt_len + 2..enc_data_end - 10];

            let mut data_bytes = Vec::with_capacity(2 + encrypted_content.len());
            data_bytes.extend_from_slice(pvv);
            data_bytes.extend_from_slice(encrypted_content);

            let hash = format!(
                "$zip2$*0*{}*0*{}*{}*{}*$/zip2$",
                aes_strength,
                hex_encode(salt),
                hex_encode(&data_bytes),
                hex_encode(auth_code),
            );

            return Ok((hash, 13600));
        }

        offset = header_end + compressed_size;
    }

    Err("No WinZip AES encrypted entry found".to_string())
}

// Placeholder -- implemented in Task 3
fn extract_zip_pkzip_hash(_file_path: &Path) -> Result<(String, u32), String> {
    Err("PKZIP hash extraction not yet implemented".to_string())
}

fn parse_winzip_aes_extra(extra: &[u8]) -> Result<(u8, u16), String> {
    let mut pos = 0;
    while pos + 4 <= extra.len() {
        let header_id = u16::from_le_bytes([extra[pos], extra[pos + 1]]);
        let data_size = u16::from_le_bytes([extra[pos + 2], extra[pos + 3]]) as usize;
        if header_id == 0x9901 && pos + 4 + data_size <= extra.len() {
            let field = &extra[pos + 4..pos + 4 + data_size];
            if field.len() >= 7 {
                let aes_strength = field[4];
                let actual_compression = u16::from_le_bytes([field[5], field[6]]);
                return Ok((aes_strength, actual_compression));
            }
        }
        pos += 4 + data_size;
    }
    Err("WinZip AES extra field (0x9901) not found".to_string())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
```

**Step 2: Register module**

In `src-tauri/src/services/mod.rs`, add: `pub mod hash_extraction;`

**Step 3: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn zip_fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap()
            .join("fixtures").join("zip")
    }

    #[test]
    fn extract_aes_zip_hash_from_fixture() {
        let path = zip_fixtures_dir().join("encrypted-aes.zip");
        let result = extract_zip_hash(&path);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
        let (hash, mode) = result.unwrap();
        assert_eq!(mode, 13600);
        assert!(hash.starts_with("$zip2$"));
        assert!(hash.ends_with("$/zip2$"));
    }

    #[test]
    fn extract_hash_unencrypted_zip_returns_error() {
        let path = zip_fixtures_dir().join("normal.zip");
        let result = extract_zip_hash(&path);
        assert!(result.is_err());
    }

    #[test]
    fn parse_winzip_aes_extra_valid() {
        let extra: Vec<u8> = vec![
            0x01, 0x99, 0x07, 0x00,
            0x02, 0x00, 0x41, 0x45, 0x03, 0x08, 0x00,
        ];
        let (strength, compression) = parse_winzip_aes_extra(&extra).unwrap();
        assert_eq!(strength, 3);
        assert_eq!(compression, 8);
    }

    #[test]
    fn hex_encode_works() {
        assert_eq!(hex_encode(&[0xde, 0xad, 0xbe, 0xef]), "deadbeef");
        assert_eq!(hex_encode(&[]), "");
    }
}
```

**Step 4: Run tests**

Run: `cargo test hash_extraction -- --nocapture` (from `src-tauri`)
Expected: All tests pass.

**Step 5: Validate extracted hash against hashcat**

```bash
# Print hash from test
cargo test extract_aes_zip -- --nocapture
# Test with hashcat
echo "<hash>" | hashcat -m 13600 -a 3 "test123"
```

**Step 6: Commit**

```
feat(gpu): add ZIP AES hash extraction for hashcat mode 13600
```

---

### Task 3: ZIP PKZIP Hash Extraction (Modes 17200-17230)

**Files:**
- Modify: `src-tauri/src/services/hash_extraction.rs` (implement `extract_zip_pkzip_hash`)
- Test: add to existing `#[cfg(test)]` module

**What this does:**
Extract PKZIP (traditional ZipCrypto) encryption parameters for hashcat modes 17200-17230.

**Background: PKZIP encryption**

PKZIP uses a stream cipher based on CRC32. The encrypted data starts with a 12-byte encryption header.

**Important:** The PKZIP `$pkzip2$` hash format is complex and version-dependent. The exact format must be validated by comparing against `zip2john` output on the same test fixture. Create a PKZIP-encrypted fixture first.

**Step 1: Create PKZIP test fixture**

```bash
# Create a PKZIP-encrypted ZIP (ZipCrypto, not AES)
7z a -p"test123" -mem=ZipCrypto fixtures/zip/encrypted-pkzip.zip fixtures/zip/test-content.txt
```

**Step 2: Implement extraction**

Follow the same pattern as Task 2 but for PKZIP entries (compression method != 99, encrypted flag set). The hash format should match hashcat's `--example-hashes -m 17200` output.

**Step 3: Validate and iterate**

Compare output against `zip2john` and `hashcat --example-hashes`. Iterate on the format until hashcat accepts the hash.

**Step 4: Commit**

```
feat(gpu): add ZIP PKZIP hash extraction for hashcat modes 17200-17210
```

---

### Task 4: Attack Mode Translation

**Files:**
- Refactor: `src-tauri/src/services/hashcat_service.rs` -> `src-tauri/src/services/hashcat_service/mod.rs` (promote to directory module)
- Create: `src-tauri/src/services/hashcat_service/detection.rs` (move detection from Task 1)
- Create: `src-tauri/src/services/hashcat_service/args.rs` (NEW)
- Test: inline `#[cfg(test)]`

**What this does:**
Translate ArchiveFlow's `AttackMode` enum into hashcat CLI arguments.

**Step 1: Restructure module**

```
src-tauri/src/services/hashcat_service/
  mod.rs          (re-exports)
  detection.rs    (moved from hashcat_service.rs)
  args.rs         (NEW)
```

**Step 2: Implement args builder**

```rust
// src-tauri/src/services/hashcat_service/args.rs

use std::path::{Path, PathBuf};
use std::io::Write;
use crate::domain::recovery::AttackMode;

pub struct HashcatArgs {
    pub args: Vec<String>,
    pub temp_files: Vec<PathBuf>,
}

pub fn build_attack_args(
    mode: &AttackMode,
    hash_mode: u32,
    hash_string: &str,
    session_name: &str,
    temp_dir: &Path,
) -> Result<HashcatArgs, String> {
    let mut args = Vec::new();
    let mut temp_files = Vec::new();

    // Common args
    args.extend([
        "-m".to_string(), hash_mode.to_string(),
        "--status".to_string(),
        "--status-json".to_string(),
        "--status-timer=1".to_string(),
        "--session".to_string(), session_name.to_string(),
        "--potfile-disable".to_string(),
        "-o".to_string(),
    ]);

    let outfile = temp_dir.join(format!("{}.out", session_name));
    args.push(outfile.to_string_lossy().to_string());
    temp_files.push(outfile);

    // Write hash to temp file
    let hash_file = temp_dir.join(format!("{}.hash", session_name));
    std::fs::write(&hash_file, hash_string)
        .map_err(|e| format!("Cannot write hash file: {}", e))?;
    temp_files.push(hash_file.clone());
    args.push(hash_file.to_string_lossy().to_string());

    match mode {
        AttackMode::Dictionary { wordlist } => {
            args.extend(["-a".to_string(), "0".to_string()]);
            let wordlist_file = temp_dir.join(format!("{}.wordlist", session_name));
            let mut f = std::fs::File::create(&wordlist_file)
                .map_err(|e| format!("Cannot create wordlist file: {}", e))?;
            for word in wordlist {
                writeln!(f, "{}", word)
                    .map_err(|e| format!("Cannot write wordlist: {}", e))?;
            }
            temp_files.push(wordlist_file.clone());
            args.push(wordlist_file.to_string_lossy().to_string());
        }
        AttackMode::BruteForce { charset, min_length, max_length } => {
            args.extend(["-a".to_string(), "3".to_string()]);
            args.extend(["-1".to_string(), charset.clone()]);
            args.extend([
                "-i".to_string(),
                format!("--increment-min={}", min_length),
                format!("--increment-max={}", max_length),
            ]);
            let mask: String = "?1".repeat(*max_length);
            args.push(mask);
        }
        AttackMode::Mask { mask } => {
            args.extend(["-a".to_string(), "3".to_string()]);
            let hashcat_mask = translate_mask(mask)?;
            args.push(hashcat_mask);
        }
    }

    Ok(HashcatArgs { args, temp_files })
}

fn translate_mask(mask: &str) -> Result<String, String> {
    let mut result = String::new();
    let chars: Vec<char> = mask.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '?' && i + 1 < chars.len() {
            match chars[i + 1] {
                'l' | 'u' | 'd' | 's' | 'a' => {
                    result.push('?');
                    result.push(chars[i + 1]);
                    i += 2;
                }
                '?' => {
                    return Err("hashcat masks do not support literal '?'".to_string());
                }
                c => {
                    return Err(format!("Unknown mask token: ?{}", c));
                }
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    Ok(result)
}
```

**Step 3: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_mask_basic() {
        assert_eq!(translate_mask("?d?d?d?d").unwrap(), "?d?d?d?d");
        assert_eq!(translate_mask("?l?u?d").unwrap(), "?l?u?d");
        assert_eq!(translate_mask("abc?d").unwrap(), "abc?d");
    }

    #[test]
    fn translate_mask_literal_question_mark_errors() {
        assert!(translate_mask("??").is_err());
    }

    #[test]
    fn brute_force_builds_increment_args() {
        let mode = AttackMode::BruteForce {
            charset: "0123456789".to_string(),
            min_length: 1,
            max_length: 6,
        };
        let temp = tempfile::tempdir().unwrap();
        let result = build_attack_args(
            &mode, 13600, "$zip2$...", "test_session", temp.path(),
        ).unwrap();

        assert!(result.args.contains(&"-a".to_string()));
        assert!(result.args.contains(&"3".to_string()));
        assert!(result.args.contains(&"-1".to_string()));
        assert!(result.args.contains(&"0123456789".to_string()));
        assert!(result.args.iter().any(|a| a.contains("--increment-min=1")));
        assert!(result.args.iter().any(|a| a.contains("--increment-max=6")));
        assert!(result.args.iter().any(|a| a == "?1?1?1?1?1?1"));
    }

    #[test]
    fn dictionary_creates_wordlist_file() {
        let mode = AttackMode::Dictionary {
            wordlist: vec!["password".to_string(), "123456".to_string()],
        };
        let temp = tempfile::tempdir().unwrap();
        let result = build_attack_args(
            &mode, 13600, "$zip2$...", "test_session", temp.path(),
        ).unwrap();

        assert!(result.args.contains(&"-a".to_string()));
        assert!(result.args.contains(&"0".to_string()));
        let wl_file = result.temp_files.iter()
            .find(|p| p.extension().map(|e| e == "wordlist").unwrap_or(false))
            .expect("should have wordlist temp file");
        let content = std::fs::read_to_string(wl_file).unwrap();
        assert!(content.contains("password"));
        assert!(content.contains("123456"));
    }
}
```

**Step 4: Run tests and commit**

```
feat(gpu): add attack mode to hashcat CLI args translation
```

---

### Task 5: Hashcat Process Manager (Core GPU Engine)

**Files:**
- Create: `src-tauri/src/services/hashcat_service/runner.rs`
- Modify: `src-tauri/src/services/hashcat_service/mod.rs` (add `pub mod runner;`)
- Test: inline `#[cfg(test)]`

**What this does:**
Launch hashcat as a subprocess, parse its `--status-json` stdout output into `RecoveryProgress` events, handle cancellation via process kill, and map exit codes to `RecoveryResult`.

**Step 1: Implement runner**

```rust
// src-tauri/src/services/hashcat_service/runner.rs

use std::io::{BufRead, BufReader, Read as _};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::domain::recovery::{RecoveryProgress, RecoveryStatus};
use crate::services::recovery_service::RecoveryResult;

#[derive(Debug, serde::Deserialize)]
struct HashcatStatus {
    status: i32,
    #[serde(default)]
    progress: Vec<u64>,
    #[serde(default)]
    devices: Vec<DeviceStatus>,
    #[serde(default)]
    recovered_hashes: Vec<u64>,
}

#[derive(Debug, serde::Deserialize)]
struct DeviceStatus {
    speed: u64,
    #[serde(default)]
    temp: i32,
}

pub fn run_hashcat(
    hashcat_path: &Path,
    args: &[String],
    outfile_path: &Path,
    task_id: &str,
    cancel_flag: Arc<AtomicBool>,
    mut on_progress: impl FnMut(RecoveryProgress),
) -> Result<RecoveryResult, String> {
    let start_time = Instant::now();

    let mut cmd = build_command(hashcat_path);
    let mut child = cmd
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("Cannot start hashcat: {}", e))?;

    let stdout = child.stdout.take()
        .ok_or("Cannot get hashcat stdout")?;
    let reader = BufReader::new(stdout);

    for line in reader.lines() {
        if cancel_flag.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(RecoveryResult::Cancelled);
        }

        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if let Ok(status) = serde_json::from_str::<HashcatStatus>(&line) {
            let (tried, total) = if status.progress.len() == 2 {
                (status.progress[0], status.progress[1])
            } else {
                (0, 0)
            };

            let total_speed: f64 = status.devices.iter()
                .map(|d| d.speed as f64)
                .sum();

            let elapsed = start_time.elapsed().as_secs_f64();

            let recovery_status = match status.status {
                6 => RecoveryStatus::Found,
                5 => RecoveryStatus::Exhausted,
                7 | 8 | 10 | 11 | 14 => RecoveryStatus::Cancelled,
                13 => RecoveryStatus::Error,
                _ => RecoveryStatus::Running,
            };

            on_progress(RecoveryProgress {
                task_id: task_id.to_string(),
                tried,
                total,
                speed: total_speed,
                status: recovery_status,
                found_password: None,
                elapsed_seconds: elapsed,
                worker_count: status.devices.len() as u64,
                last_checkpoint_at: None,
            });
        }
    }

    let exit_status = child.wait()
        .map_err(|e| format!("Failed to wait for hashcat: {}", e))?;

    let exit_code = exit_status.code().unwrap_or(-1);

    match exit_code {
        0 => {
            let password = read_cracked_password(outfile_path)?;
            Ok(RecoveryResult::Found(password))
        }
        1 => Ok(RecoveryResult::Exhausted),
        2 | 3 | 4 | 5 => Ok(RecoveryResult::Cancelled),
        _ => {
            let stderr_output = child.stderr
                .map(|mut s| {
                    let mut buf = String::new();
                    let _ = s.read_to_string(&mut buf);
                    buf
                })
                .unwrap_or_default();
            Err(format!("hashcat exit code {}: {}", exit_code, stderr_output))
        }
    }
}

fn read_cracked_password(outfile_path: &Path) -> Result<String, String> {
    let content = std::fs::read_to_string(outfile_path)
        .map_err(|e| format!("Cannot read hashcat output file: {}", e))?;

    if let Some(line) = content.lines().next() {
        if let Some(colon_pos) = line.rfind(':') {
            return Ok(line[colon_pos + 1..].to_string());
        }
    }
    Err("Cannot extract password from hashcat output".to_string())
}

#[cfg(windows)]
fn build_command(hashcat_path: &Path) -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new(hashcat_path);
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    cmd
}

#[cfg(not(windows))]
fn build_command(hashcat_path: &Path) -> Command {
    Command::new(hashcat_path)
}
```

**Step 2: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hashcat_status_json() {
        let json = r#"{"session":"test","status":3,"target":"$zip2$...","progress":[500000,2000000],"recovered_hashes":[0,1],"recovered_salts":[0,1],"rejected":0,"devices":[{"device_id":1,"device_name":"NVIDIA GeForce RTX 4080","device_type":"GPU","speed":1500000,"temp":65,"util":98}],"time_start":1711000000,"estimated_stop":1711000300}"#;
        let status: HashcatStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.status, 3);
        assert_eq!(status.progress, vec![500000, 2000000]);
        assert_eq!(status.devices.len(), 1);
        assert_eq!(status.devices[0].speed, 1500000);
    }

    #[test]
    fn parse_cracked_status() {
        let json = r#"{"session":"test","status":6,"progress":[1234,2000000],"recovered_hashes":[1,1],"devices":[{"device_id":1,"device_name":"GPU","device_type":"GPU","speed":0,"temp":60}]}"#;
        let status: HashcatStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.status, 6);
        assert_eq!(status.recovered_hashes, vec![1, 1]);
    }

    #[test]
    fn read_cracked_password_from_outfile() {
        let temp = tempfile::tempdir().unwrap();
        let outfile = temp.path().join("test.out");
        std::fs::write(&outfile, "$zip2$*0*3*0*abcd*1234*ef01*$/zip2$:test123\n").unwrap();
        let password = read_cracked_password(&outfile).unwrap();
        assert_eq!(password, "test123");
    }

    #[test]
    fn read_cracked_password_empty_file_errors() {
        let temp = tempfile::tempdir().unwrap();
        let outfile = temp.path().join("empty.out");
        std::fs::write(&outfile, "").unwrap();
        assert!(read_cracked_password(&outfile).is_err());
    }
}
```

**Step 3: Run tests and commit**

```
feat(gpu): add hashcat process runner with status JSON parsing
```

---

### Task 6: GPU Recovery Entry Point + Backend Selection

**Files:**
- Create: `src-tauri/src/services/hashcat_service/gpu_engine.rs`
- Modify: `src-tauri/src/services/hashcat_service/mod.rs` (add `pub mod gpu_engine;`)
- Modify: `src-tauri/src/commands/recovery_commands.rs:196-210` (add GPU backend dispatch)

**What this does:**
Wire everything together: hash extraction -> build args -> launch hashcat -> emit progress -> return result. This is the GPU equivalent of `run_recovery()`.

**Step 1: Create `gpu_engine.rs`**

```rust
// src-tauri/src/services/hashcat_service/gpu_engine.rs

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::Emitter;

use crate::domain::recovery::{RecoveryConfig, RecoveryProgress, RecoveryStatus};
use crate::domain::task::ArchiveType;
use crate::services::recovery_service::RecoveryResult;
use crate::services::hash_extraction;

use super::args::build_attack_args;
use super::detection;
use super::runner::run_hashcat;

pub fn run_gpu_recovery(
    config: RecoveryConfig,
    file_path: String,
    archive_type: ArchiveType,
    app_handle: tauri::AppHandle,
    cancel_flag: Arc<AtomicBool>,
) -> Result<RecoveryResult, String> {
    let task_id = config.task_id.clone();

    // 1. Detect hashcat
    let hashcat_info = detection::detect_hashcat(None)?;

    // 2. Extract hash
    let (hash_string, hash_mode) = match archive_type {
        ArchiveType::Zip => hash_extraction::extract_zip_hash(
            std::path::Path::new(&file_path)
        )?,
        ArchiveType::SevenZ => return Err("7z GPU recovery not yet implemented".to_string()),
        ArchiveType::Rar => return Err("RAR GPU recovery not yet implemented".to_string()),
        ArchiveType::Unknown => return Err("Unknown archive type".to_string()),
    };

    log::info!(
        "GPU recovery: task={}, hash_mode={}, hashcat={}",
        task_id, hash_mode, hashcat_info.version
    );

    // 3. Build CLI args
    let temp_dir = std::env::temp_dir().join("archiveflow_hashcat");
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Cannot create temp dir: {}", e))?;

    let session_name = format!("af_{}", task_id.replace('-', "_"));
    let hashcat_args = build_attack_args(
        &config.mode, hash_mode, &hash_string,
        &session_name, &temp_dir,
    )?;

    let outfile_path = temp_dir.join(format!("{}.out", session_name));

    // 4. Emit initial progress
    let _ = app_handle.emit("recovery-progress", RecoveryProgress {
        task_id: task_id.clone(),
        tried: 0, total: 0,
        speed: 0.0,
        status: RecoveryStatus::Running,
        found_password: None,
        elapsed_seconds: 0.0,
        worker_count: hashcat_info.devices.len() as u64,
        last_checkpoint_at: None,
    });

    // 5. Run hashcat
    let app_clone = app_handle.clone();
    let result = run_hashcat(
        &hashcat_info.path,
        &hashcat_args.args,
        &outfile_path,
        &task_id,
        cancel_flag,
        move |progress| {
            let _ = app_clone.emit("recovery-progress", progress);
        },
    );

    // 6. Cleanup
    for path in &hashcat_args.temp_files {
        let _ = std::fs::remove_file(path);
    }

    result
}
```

**Step 2: Modify `recovery_commands.rs` dispatch**

In `spawn_recovery_worker()` around line 205, change:

```rust
// BEFORE:
let result = recovery_service::run_recovery(
    config, file_path, archive_type, app_handle.clone(), cancel_flag,
);

// AFTER:
let result = if should_use_gpu(&archive_type) {
    crate::services::hashcat_service::gpu_engine::run_gpu_recovery(
        config, file_path, archive_type, app_handle.clone(), cancel_flag,
    )
} else {
    recovery_service::run_recovery(
        config, file_path, archive_type, app_handle.clone(), cancel_flag,
    )
};
```

Add helper:
```rust
fn should_use_gpu(archive_type: &ArchiveType) -> bool {
    if !matches!(archive_type, ArchiveType::Zip) {
        return false;
    }
    crate::services::hashcat_service::detect_hashcat(None).is_ok()
}
```

**Step 3: Integration test (manual, requires hashcat)**

```rust
#[test]
#[ignore = "requires hashcat installed and GPU available"]
fn gpu_recovery_finds_password_on_aes_zip() {
    // Test with encrypted-aes.zip fixture (password: test123)
}
```

**Step 4: Commit**

```
feat(gpu): wire GPU recovery engine with backend selection
```

---

### Task 7: 7z Hash Extraction (Mode 11600) -- DEFERRED TO V2

**Files:**
- Modify: `src-tauri/src/services/hash_extraction.rs` (add `extract_7z_hash`)

**What this does:**
Parse 7z file headers to extract AES-256 encryption parameters for hashcat mode 11600.

**Note:** 7z header parsing is significantly more complex than ZIP because:
1. The main header at the end of the file may itself be compressed (LZMA)
2. Header encryption hides the metadata
3. The `sevenz-rust` crate doesn't expose raw crypto parameters

This task should reference `7z2hashcat.pl` as the definitive implementation.

**Deferred to V2** -- Get ZIP working end-to-end first.

---

### Task 8: Settings UI for Hashcat Path + GPU Toggle

**Files:**
- Frontend: settings page components
- Backend: settings commands

**What this does:**
Add a settings section for hashcat path configuration and GPU status display.

**Deferred to after Tasks 1-6 are validated end-to-end.**

---

## Execution Order Summary

| Task | Description | Priority | Est. Complexity |
|------|-------------|----------|----------------|
| 1 | Hashcat detection | Must | Low |
| 2 | ZIP AES hash extraction | Must | Medium |
| 3 | ZIP PKZIP hash extraction | Should | Medium-High |
| 4 | Attack mode translation | Must | Low |
| 5 | Hashcat process manager | Must | Medium |
| 6 | GPU recovery wiring | Must | Medium |
| 7 | 7z hash extraction | Deferred V2 | High |
| 8 | Settings UI | Deferred V2 | Low |

**MVP = Tasks 1 + 2 + 4 + 5 + 6** (ZIP AES GPU recovery end-to-end)

**Critical validation:** After Task 2, manually test extracted hash with hashcat before proceeding.
