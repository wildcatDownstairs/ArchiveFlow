use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::db::Database;
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
///
/// 注意: GPU 恢复不支持暂停/续跑。取消后 hashcat 进程被终止，用户必须重新启动。
/// 因此这里会主动删除 CPU 遗留的旧断点，避免 resume_recovery 误用旧断点以 CPU 模式重跑。
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
        return Err("GPU 恢复当前仅支持 ZIP（AES 或 PKZIP）".to_string());
    }

    // GPU 不支持断点续跑，启动前清除旧的 CPU checkpoint，
    // 防止用户稍后误点"继续"时以 CPU 模式从旧断点恢复。
    let db = app_handle.state::<Database>();
    let _ = db.delete_recovery_checkpoint(&config.task_id);

    log::info!(
        "GPU 恢复启动: task={} file={} hashcat_path={:?}",
        config.task_id,
        file_path,
        config.hashcat_path
    );

    let hashcat_info = detect_hashcat(config.hashcat_path.as_deref().map(Path::new))?;
    log::info!(
        "hashcat 检测成功: path={} version={} devices={}",
        hashcat_info.path.display(),
        hashcat_info.version,
        hashcat_info.devices.len()
    );
    if !hashcat_info.has_usable_gpu() {
        return Err("未检测到可用 GPU 设备，无法启动 hashcat GPU 恢复".to_string());
    }

    let zip_hash = extract_zip_hash(Path::new(&file_path))?;
    log::info!(
        "hash 提取成功: mode={} hash_len={}",
        zip_hash.hash_mode,
        zip_hash.hash_string.len()
    );

    let temp_dir = build_task_temp_dir(&app_handle, &config.task_id)?;
    let session_name = format!("archiveflow_{}", config.task_id.replace('-', "_"));
    let hashcat_args = build_attack_args(
        &config.mode,
        zip_hash.hash_mode,
        &zip_hash.hash_string,
        &session_name,
        &temp_dir,
    )?;

    log::info!("hashcat 参数: {:?}", hashcat_args.args);

    let task_id = config.task_id.clone();
    let device_count = hashcat_info.devices.len() as u64;

    // hashcat 启动后有 5-10 秒自动调参（autotune）阶段不会输出 status JSON，
    // 先发一个初始进度让前端立刻看到"正在运行"的状态。
    emit_initial_progress(&app_handle, &task_id, device_count);

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

    match &result {
        Ok(ref res) => log::info!("hashcat 结束: task={} result={:?}", task_id, res),
        Err(ref err) => log::error!("hashcat 失败: task={} error={}", task_id, err),
    }

    cleanup_temp_dir(&temp_dir, &hashcat_args.temp_files);

    result
}

/// 尽力清理 hashcat 临时目录。
/// 先删除已知的临时文件，再尝试 remove_dir_all 清理 hashcat 自动生成的
/// session/potfile/outfile 等残留文件。
fn cleanup_temp_dir(temp_dir: &Path, temp_files: &[PathBuf]) {
    for temp_file in temp_files {
        let _ = std::fs::remove_file(temp_file);
    }
    // 使用 remove_dir_all 而不是 remove_dir，因为 hashcat 可能在目录里
    // 创建了额外的 .restore / .log 等文件，remove_dir 只能删除空目录。
    let _ = std::fs::remove_dir_all(temp_dir);
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
