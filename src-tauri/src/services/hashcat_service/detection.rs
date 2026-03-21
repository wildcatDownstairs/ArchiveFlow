use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use super::build_hashcat_command;

/// hashcat 检测结果：供设置页直接展示“是否可用、路径、版本、设备列表”。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashcatDetectionResult {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub devices: Vec<HashcatDeviceInfo>,
    pub error: Option<String>,
}

/// 单个设备信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HashcatDeviceInfo {
    pub id: u32,
    pub name: String,
    pub device_type: String,
}

/// 运行时真正使用的 hashcat 信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashcatInfo {
    pub path: PathBuf,
    pub version: String,
    pub devices: Vec<HashcatDeviceInfo>,
}

impl HashcatInfo {
    /// V1 只把至少有一个 GPU 设备的环境视为“可用于外部 GPU 恢复”。
    pub fn has_usable_gpu(&self) -> bool {
        self.devices
            .iter()
            .any(|device| device.device_type.eq_ignore_ascii_case("gpu"))
    }
}

pub fn detect_hashcat(custom_path: Option<&Path>) -> Result<HashcatInfo, String> {
    let path = match custom_path {
        Some(path) if path.exists() => path.to_path_buf(),
        Some(path) => return Err(format!("指定的 hashcat 路径不存在: {}", path.display())),
        None => find_hashcat_binary()?,
    };

    let version = get_version(&path)?;
    let devices = get_devices(&path)?;

    Ok(HashcatInfo {
        path,
        version,
        devices,
    })
}

pub fn detect_hashcat_for_ui(custom_path: Option<&Path>) -> HashcatDetectionResult {
    match detect_hashcat(custom_path) {
        Ok(info) => HashcatDetectionResult {
            available: info.has_usable_gpu(),
            path: Some(info.path.to_string_lossy().to_string()),
            version: Some(info.version),
            devices: info.devices,
            error: None,
        },
        Err(error) => HashcatDetectionResult {
            available: false,
            path: custom_path.map(|path| path.to_string_lossy().to_string()),
            version: None,
            devices: Vec::new(),
            error: Some(error),
        },
    }
}

fn find_hashcat_binary() -> Result<PathBuf, String> {
    if let Some(path) = find_hashcat_in_path() {
        return Ok(path);
    }

    if let Some(path) = find_hashcat_in_archiveflow_tools() {
        return Ok(path);
    }

    Err(
        "未在 PATH 或 ArchiveFlow 本地工具目录中找到 hashcat，请在设置中指定 hashcat.exe 路径"
            .to_string(),
    )
}

fn find_hashcat_in_path() -> Option<PathBuf> {
    let command = if cfg!(windows) { "where" } else { "which" };
    let executable = if cfg!(windows) {
        "hashcat.exe"
    } else {
        "hashcat"
    };
    let output = Command::new(command).arg(executable).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.exists())
}

/// 自动扫描 ArchiveFlow 本地工具缓存目录，支持“本地已下载但未加 PATH”的场景。
fn find_hashcat_in_archiveflow_tools() -> Option<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")?;
    let tools_root = PathBuf::from(local_app_data)
        .join("ArchiveFlow")
        .join("tools")
        .join("hashcat");
    let binary_name = if cfg!(windows) {
        "hashcat.exe"
    } else {
        "hashcat"
    };

    let direct_path = tools_root.join(binary_name);
    if direct_path.exists() {
        return Some(direct_path);
    }

    let entries = fs::read_dir(tools_root).ok()?;
    for entry in entries.flatten() {
        let candidate = entry.path().join(binary_name);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

fn get_version(path: &Path) -> Result<String, String> {
    let output = build_hashcat_command(path)
        .arg("--version")
        .output()
        .map_err(|error| format!("执行 hashcat --version 失败: {}", error))?;

    if !output.status.success() {
        return Err("hashcat --version 执行失败".to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn get_devices(path: &Path) -> Result<Vec<HashcatDeviceInfo>, String> {
    let output = build_hashcat_command(path)
        .arg("-I")
        .output()
        .map_err(|error| format!("执行 hashcat -I 失败: {}", error))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "hashcat -I 退出码 {:?}，无法读取设备信息。stderr: {}  stdout: {}",
            output.status.code(),
            stderr.trim(),
            stdout.chars().take(200).collect::<String>()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let devices = parse_devices(&stdout);
    if devices.is_empty() {
        // 同时尝试 stderr，部分 hashcat 版本把 -I 输出写到 stderr
        let stderr = String::from_utf8_lossy(&output.stderr);
        let devices_from_stderr = parse_devices(&stderr);
        if !devices_from_stderr.is_empty() {
            return Ok(devices_from_stderr);
        }
        return Err("hashcat 未返回任何可用设备".to_string());
    }

    Ok(devices)
}

fn parse_devices(output: &str) -> Vec<HashcatDeviceInfo> {
    let mut devices = Vec::new();
    let mut current_id: Option<u32> = None;
    let mut current_name: Option<String> = None;
    let mut current_type: Option<String> = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(id) = parse_device_id(trimmed) {
            if let (Some(previous_id), Some(name), Some(device_type)) =
                (current_id.take(), current_name.take(), current_type.take())
            {
                devices.push(HashcatDeviceInfo {
                    id: previous_id,
                    name,
                    device_type,
                });
            }
            current_id = Some(id);
        } else if let Some(value) = trimmed.strip_prefix("Name...........:") {
            current_name = Some(value.trim().to_string());
        } else if let Some(value) = trimmed.strip_prefix("Type...........:") {
            current_type = Some(value.trim().to_string());
        }
    }

    if let (Some(id), Some(name), Some(device_type)) = (current_id, current_name, current_type) {
        devices.push(HashcatDeviceInfo {
            id,
            name,
            device_type,
        });
    }

    devices
}

fn parse_device_id(line: &str) -> Option<u32> {
    let trimmed = line
        .strip_prefix("Backend Device ID #")
        .or_else(|| line.strip_prefix("Device ID #"))?;
    let digits: String = trimmed
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{detect_hashcat, find_hashcat_in_archiveflow_tools, parse_devices};
    use std::path::Path;

    #[test]
    fn parse_devices_extracts_multiple_entries() {
        let sample = r#"
Backend Device ID #1
  Name...........: NVIDIA GeForce RTX 4080
  Type...........: GPU

Backend Device ID #2
  Name...........: Intel(R) UHD Graphics 770
  Type...........: GPU
"#;

        let devices = parse_devices(sample);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].id, 1);
        assert!(devices[0].name.contains("RTX 4080"));
        assert_eq!(devices[1].id, 2);
    }

    #[test]
    fn parse_devices_empty_input_returns_empty_list() {
        assert!(parse_devices("").is_empty());
    }

    #[test]
    fn detect_hashcat_reports_missing_custom_path() {
        let result = detect_hashcat(Some(Path::new("C:/definitely/missing/hashcat.exe")));
        assert!(result.is_err());
    }

    #[test]
    fn find_hashcat_in_archiveflow_tools_returns_none_without_cache_dir() {
        let _ = find_hashcat_in_archiveflow_tools();
    }
}
