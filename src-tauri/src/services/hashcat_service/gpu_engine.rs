use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::domain::recovery::{RecoveryConfig, RecoveryProgress, RecoveryStatus};
use crate::domain::task::ArchiveType;
use crate::services::recovery_service::RecoveryResult;

use super::args::build_attack_args;
use super::detection::detect_hashcat;
use super::runner::run_hashcat;
use super::zip_aes_extract::extract_zip_hash;

/// GPU 恢复总控：
///   1. 检测 hashcat
///   2. 提取 ZIP AES 所需 hash
///   3. 生成 CLI 参数
///   4. 运行 hashcat 并把状态映射回现有 RecoveryProgress
pub fn run_gpu_recovery(
    config: RecoveryConfig,
    file_path: String,
    archive_type: ArchiveType,
    app_handle: tauri::AppHandle,
    cancel_flag: Arc<AtomicBool>,
) -> Result<RecoveryResult, String> {
    if !cfg!(windows) {
        return Err("GPU 恢复 V1 仅支持 Windows".to_string());
    }
    if archive_type != ArchiveType::Zip {
        return Err("GPU 恢复当前仅支持 ZIP AES".to_string());
    }

    let hashcat_info = detect_hashcat(config.hashcat_path.as_deref().map(Path::new))?;
    if !hashcat_info.has_usable_gpu() {
        return Err("未检测到可用 GPU 设备，无法启动 hashcat GPU 恢复".to_string());
    }

    let zip_hash = extract_zip_hash(Path::new(&file_path))?;

    let temp_dir = build_task_temp_dir(&app_handle, &config.task_id)?;
    let session_name = format!("archiveflow_{}", config.task_id.replace('-', "_"));
    let hashcat_args = build_attack_args(
        &config.mode,
        zip_hash.hash_mode,
        &zip_hash.hash_string,
        &session_name,
        &temp_dir,
    )?;

    let task_id = config.task_id.clone();
    let app_handle_for_progress = app_handle.clone();
    let result = run_hashcat(
        &hashcat_info.path,
        &hashcat_args.args,
        &hashcat_args.outfile_path,
        &task_id,
        cancel_flag,
        move |progress: RecoveryProgress| {
            let _ = app_handle_for_progress.emit("recovery-progress", progress);
        },
    );

    for temp_file in &hashcat_args.temp_files {
        let _ = std::fs::remove_file(temp_file);
    }
    let _ = std::fs::remove_dir(&temp_dir);

    result
}

fn build_task_temp_dir(app_handle: &tauri::AppHandle, task_id: &str) -> Result<PathBuf, String> {
    let base_dir = app_handle
        .path()
        .app_cache_dir()
        .map_err(|error| format!("获取应用缓存目录失败: {}", error))?;
    let temp_dir = base_dir.join("hashcat").join(task_id);
    std::fs::create_dir_all(&temp_dir)
        .map_err(|error| format!("创建 hashcat 临时目录失败: {}", error))?;
    Ok(temp_dir)
}

#[allow(dead_code)]
fn emit_initial_progress(app_handle: &tauri::AppHandle, task_id: &str, worker_count: u64) {
    let _ = app_handle.emit(
        "recovery-progress",
        RecoveryProgress {
            task_id: task_id.to_string(),
            tried: 0,
            total: 0,
            speed: 0.0,
            status: RecoveryStatus::Running,
            found_password: None,
            elapsed_seconds: 0.0,
            worker_count,
            last_checkpoint_at: None,
        },
    );
}
