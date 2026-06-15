use chrono::Local;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager, Runtime};

use crate::errors::AppError;

const LOG_DIR_NAME: &str = "logs";
const LOG_FILE_NAME: &str = "archiveflow.log";
const ROTATED_LOG_FILE_NAME: &str = "archiveflow.1.log";
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

pub fn log_dir<R: Runtime>(app_handle: &AppHandle<R>) -> Result<PathBuf, AppError> {
    let dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| AppError::FileError(format!("无法获取日志目录: {}", error)))?
        .join(LOG_DIR_NAME);
    fs::create_dir_all(&dir)
        .map_err(|error| AppError::FileError(format!("无法创建日志目录: {}", error)))?;
    Ok(dir)
}

pub fn log_file_path<R: Runtime>(app_handle: &AppHandle<R>) -> Result<PathBuf, AppError> {
    Ok(log_dir(app_handle)?.join(LOG_FILE_NAME))
}

pub fn append_app_log<R: Runtime>(
    app_handle: &AppHandle<R>,
    level: &str,
    category: Option<&str>,
    message: &str,
) -> Result<(), AppError> {
    let log_path = log_file_path(app_handle)?;
    rotate_if_needed(&log_path)?;

    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let line = build_log_line(&timestamp, level, category, message);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| AppError::FileError(format!("无法打开日志文件: {}", error)))?;

    file.write_all(line.as_bytes())
        .map_err(|error| AppError::FileError(format!("无法写入日志文件: {}", error)))?;
    Ok(())
}

pub fn open_log_dir<R: Runtime>(app_handle: &AppHandle<R>) -> Result<PathBuf, AppError> {
    let dir = log_dir(app_handle)?;
    open_directory(&dir)?;
    Ok(dir)
}

fn rotate_if_needed(log_path: &Path) -> Result<(), AppError> {
    let metadata = match fs::metadata(log_path) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(()),
    };
    if metadata.len() < MAX_LOG_BYTES {
        return Ok(());
    }

    let rotated_path = log_path.with_file_name(ROTATED_LOG_FILE_NAME);
    if rotated_path.exists() {
        fs::remove_file(&rotated_path)
            .map_err(|error| AppError::FileError(format!("无法轮转旧日志: {}", error)))?;
    }
    fs::rename(log_path, rotated_path)
        .map_err(|error| AppError::FileError(format!("无法轮转日志文件: {}", error)))?;
    Ok(())
}

fn build_log_line(timestamp: &str, level: &str, category: Option<&str>, message: &str) -> String {
    let level = normalize_token(level, "INFO");
    let category = category
        .map(|value| normalize_token(value, "APP"))
        .filter(|value| !value.is_empty());
    let message = sanitize_message(message);

    match category {
        Some(category) => format!("[{}] [{}] [{}] {}\n", timestamp, level, category, message),
        None => format!("[{}] [{}] {}\n", timestamp, level, message),
    }
}

fn normalize_token(value: &str, fallback: &str) -> String {
    let token: String = value
        .trim()
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_uppercase())
            } else if matches!(ch, '_' | '-' | ' ') {
                Some('_')
            } else {
                None
            }
        })
        .take(32)
        .collect();

    if token.is_empty() {
        fallback.to_string()
    } else {
        token
    }
}

fn sanitize_message(message: &str) -> String {
    message
        .replace("\r\n", "\\n")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

#[cfg(target_os = "windows")]
fn open_directory(path: &Path) -> Result<(), AppError> {
    Command::new("explorer")
        .arg(path)
        .spawn()
        .map_err(|error| AppError::FileError(format!("无法打开日志文件夹: {}", error)))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_directory(path: &Path) -> Result<(), AppError> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map_err(|error| AppError::FileError(format!("无法打开日志文件夹: {}", error)))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_directory(path: &Path) -> Result<(), AppError> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|error| AppError::FileError(format!("无法打开日志文件夹: {}", error)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::build_log_line;

    #[test]
    fn log_line_uses_expected_timestamp_level_and_category_shape() {
        let line = build_log_line(
            "2026-06-15 21:00:00",
            "warn",
            Some("boot step"),
            "加载任务\n失败",
        );

        assert_eq!(
            line,
            "[2026-06-15 21:00:00] [WARN] [BOOT_STEP] 加载任务\\n失败\n"
        );
    }

    #[test]
    fn log_line_omits_empty_category() {
        let line = build_log_line("2026-06-15 21:00:00", "", None, "ArchiveFlow started");

        assert_eq!(
            line,
            "[2026-06-15 21:00:00] [INFO] ArchiveFlow started\n"
        );
    }
}
