use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde::Deserialize;

use crate::domain::recovery::{RecoveryProgress, RecoveryStatus};
use crate::services::recovery_service::RecoveryResult;

#[derive(Debug, Deserialize)]
struct HashcatStatus {
    status: i32,
    #[serde(default)]
    progress: Vec<u64>,
    #[serde(default)]
    devices: Vec<HashcatDeviceStatus>,
}

#[derive(Debug, Deserialize)]
struct HashcatDeviceStatus {
    #[serde(default)]
    speed: f64,
}

pub fn run_hashcat(
    hashcat_path: &Path,
    args: &[String],
    outfile_path: &Path,
    task_id: &str,
    cancel_flag: Arc<AtomicBool>,
    mut on_progress: impl FnMut(RecoveryProgress),
) -> Result<RecoveryResult, String> {
    let started_at = Instant::now();
    let mut last_tried = 0_u64;
    let mut last_total = 0_u64;
    let mut last_speed = 0.0_f64;
    let mut last_worker_count = 0_u64;

    let mut child = build_command(hashcat_path)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|error| format!("启动 hashcat 失败: {}", error))?;

    let stdout = child.stdout.take().ok_or("无法读取 hashcat stdout")?;
    let stderr = child.stderr.take().ok_or("无法读取 hashcat stderr")?;
    let stderr_handle = std::thread::spawn(move || {
        let mut buffer = String::new();
        let mut reader = BufReader::new(stderr);
        let _ = reader.read_to_string(&mut buffer);
        buffer
    });

    for line in BufReader::new(stdout).lines() {
        if cancel_flag.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            on_progress(RecoveryProgress {
                task_id: task_id.to_string(),
                tried: last_tried,
                total: last_total,
                speed: 0.0,
                status: RecoveryStatus::Cancelled,
                found_password: None,
                elapsed_seconds: started_at.elapsed().as_secs_f64(),
                worker_count: last_worker_count,
                last_checkpoint_at: None,
            });
            let _ = stderr_handle.join();
            return Ok(RecoveryResult::Cancelled);
        }

        let line = match line {
            Ok(line) => line,
            Err(_) => continue,
        };
        let status = match serde_json::from_str::<HashcatStatus>(&line) {
            Ok(status) => status,
            Err(_) => continue,
        };

        if status.progress.len() == 2 {
            last_tried = status.progress[0];
            last_total = status.progress[1];
        }
        last_speed = status.devices.iter().map(|device| device.speed).sum();
        last_worker_count = status.devices.len() as u64;

        on_progress(RecoveryProgress {
            task_id: task_id.to_string(),
            tried: last_tried,
            total: last_total,
            speed: last_speed,
            status: map_status(status.status),
            found_password: None,
            elapsed_seconds: started_at.elapsed().as_secs_f64(),
            worker_count: last_worker_count,
            last_checkpoint_at: None,
        });
    }

    let exit_status = child
        .wait()
        .map_err(|error| format!("等待 hashcat 进程结束失败: {}", error))?;
    let stderr_output = stderr_handle.join().unwrap_or_default();

    match exit_status.code().unwrap_or(-1) {
        0 => {
            let password = read_cracked_password(outfile_path)?;
            on_progress(RecoveryProgress {
                task_id: task_id.to_string(),
                tried: last_tried,
                total: last_total,
                speed: last_speed,
                status: RecoveryStatus::Found,
                found_password: Some(password.clone()),
                elapsed_seconds: started_at.elapsed().as_secs_f64(),
                worker_count: last_worker_count,
                last_checkpoint_at: None,
            });
            Ok(RecoveryResult::Found(password))
        }
        1 => {
            on_progress(RecoveryProgress {
                task_id: task_id.to_string(),
                tried: last_tried,
                total: last_total,
                speed: 0.0,
                status: RecoveryStatus::Exhausted,
                found_password: None,
                elapsed_seconds: started_at.elapsed().as_secs_f64(),
                worker_count: last_worker_count,
                last_checkpoint_at: None,
            });
            Ok(RecoveryResult::Exhausted)
        }
        2 | 3 | 4 | 5 => {
            on_progress(RecoveryProgress {
                task_id: task_id.to_string(),
                tried: last_tried,
                total: last_total,
                speed: 0.0,
                status: RecoveryStatus::Cancelled,
                found_password: None,
                elapsed_seconds: started_at.elapsed().as_secs_f64(),
                worker_count: last_worker_count,
                last_checkpoint_at: None,
            });
            Ok(RecoveryResult::Cancelled)
        }
        code => Err(format!("hashcat 退出码 {}: {}", code, stderr_output.trim())),
    }
}

fn map_status(status: i32) -> RecoveryStatus {
    match status {
        5 => RecoveryStatus::Exhausted,
        6 => RecoveryStatus::Found,
        7 => RecoveryStatus::Cancelled,
        13 => RecoveryStatus::Error,
        _ => RecoveryStatus::Running,
    }
}

fn read_cracked_password(outfile_path: &Path) -> Result<String, String> {
    let content = std::fs::read_to_string(outfile_path)
        .map_err(|error| format!("读取 hashcat 输出文件失败: {}", error))?;
    let password = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| "hashcat 没有输出已破解密码".to_string())?;
    Ok(password.to_string())
}

#[cfg(windows)]
fn build_command(hashcat_path: &Path) -> Command {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut command = super::build_hashcat_command(hashcat_path);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(not(windows))]
fn build_command(hashcat_path: &Path) -> Command {
    super::build_hashcat_command(hashcat_path)
}

#[cfg(test)]
mod tests {
    use super::{map_status, read_cracked_password, HashcatStatus};

    #[test]
    fn parse_hashcat_status_json() {
        let json = r#"{"status":3,"progress":[500000,2000000],"devices":[{"speed":1500000.0}]}"#;
        let status: HashcatStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.status, 3);
        assert_eq!(status.progress, vec![500000, 2000000]);
        assert_eq!(status.devices.len(), 1);
    }

    #[test]
    fn map_status_uses_existing_recovery_statuses() {
        assert_eq!(
            map_status(3),
            crate::domain::recovery::RecoveryStatus::Running
        );
        assert_eq!(
            map_status(5),
            crate::domain::recovery::RecoveryStatus::Exhausted
        );
        assert_eq!(
            map_status(6),
            crate::domain::recovery::RecoveryStatus::Found
        );
    }

    #[test]
    fn read_cracked_password_reads_first_line() {
        let temp_dir = tempfile::tempdir().unwrap();
        let outfile = temp_dir.path().join("result.out");
        std::fs::write(&outfile, "test123\n").unwrap();
        assert_eq!(read_cracked_password(&outfile).unwrap(), "test123");
    }
}
