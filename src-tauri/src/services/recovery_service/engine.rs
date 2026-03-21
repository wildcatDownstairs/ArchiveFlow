// engine.rs 负责把候选生成、worker、断点、事件发送串成一条完整恢复流程。
// 它像是"调度总控"，但不关心具体某种格式的解密细节。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use tauri::{Emitter, Manager};

use crate::db::Database;
use crate::domain::recovery::{
    AttackMode, RecoveryCheckpoint, RecoveryConfig, RecoveryProgress, RecoveryStatus,
};
use crate::domain::task::ArchiveType;

use super::generators::{BruteForceIterator, MaskIterator};
use super::passwords::validate_recovery_target;
use super::workers::{
    create_result_channel, format_worker_panic_error, join_worker_handles,
    run_stateless_worker_shard, run_zip_worker_shard,
};

const PROGRESS_INTERVAL_MS: u64 = 500;

#[derive(Debug)]
pub enum RecoveryResult {
    Found(String),
    Exhausted,
    Cancelled,
}

pub(crate) fn load_resume_offset(
    app_handle: &tauri::AppHandle,
    task_id: &str,
    mode: &AttackMode,
    archive_type: &ArchiveType,
    total: u64,
) -> u64 {
    let db = app_handle.state::<Database>();
    match db.get_recovery_checkpoint(task_id) {
        Ok(Some(checkpoint))
            if checkpoint.archive_type == *archive_type && checkpoint.mode == *mode =>
        {
            checkpoint.tried.min(total)
        }
        Ok(_) => 0,
        Err(error) => {
            log::error!("读取恢复断点失败: task={} error={}", task_id, error);
            0
        }
    }
}

pub(crate) fn persist_recovery_checkpoint(
    app_handle: &tauri::AppHandle,
    task_id: &str,
    mode: &AttackMode,
    archive_type: &ArchiveType,
    priority: i32,
    tried: u64,
    total: u64,
) {
    let db = app_handle.state::<Database>();
    let checkpoint = RecoveryCheckpoint {
        task_id: task_id.to_string(),
        mode: mode.clone(),
        archive_type: archive_type.clone(),
        priority,
        tried,
        total,
        updated_at: Utc::now(),
    };

    if let Err(error) = db.upsert_recovery_checkpoint(&checkpoint) {
        log::error!("写入恢复断点失败: task={} error={}", task_id, error);
    }
}

pub fn run_recovery(
    config: RecoveryConfig,
    file_path: String,
    archive_type: ArchiveType,
    app_handle: tauri::AppHandle,
    cancel_flag: Arc<AtomicBool>,
) -> Result<RecoveryResult, String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("文件不存在: {}", file_path));
    }
    let path_buf = path.to_path_buf();
    let task_id = config.task_id.clone();
    let mode = Arc::new(config.mode);

    validate_recovery_target(path, &archive_type)?;

    let total = match mode.as_ref() {
        AttackMode::Dictionary { wordlist } => wordlist.len() as u64,
        AttackMode::BruteForce {
            charset,
            min_length,
            max_length,
        } => BruteForceIterator::total_combinations(
            charset.chars().count(),
            *min_length,
            *max_length,
        ),
        AttackMode::Mask { mask } => MaskIterator::total_combinations(mask)?,
    };

    let resume_from =
        load_resume_offset(&app_handle, &task_id, mode.as_ref(), &archive_type, total);
    let remaining = total.saturating_sub(resume_from);

    let num_workers = {
        let cpus = num_cpus::get() as u64;
        let n = std::cmp::max(1, cpus.saturating_sub(1));
        std::cmp::min(n, remaining.max(1))
    };
    let shard_size = remaining / num_workers;

    let shards: Vec<(u64, u64)> = (0..num_workers)
        .map(|i| {
            let start = resume_from + i * shard_size;
            let end = if i == num_workers - 1 {
                total
            } else {
                start + shard_size
            };
            (start, end)
        })
        .collect();

    log::info!(
        "开始并行恢复: task={}, workers={}, total={}, resume_from={}, archive_type={:?}",
        task_id,
        num_workers,
        total,
        resume_from,
        archive_type
    );

    let tried_counter = Arc::new(AtomicU64::new(resume_from));
    let (result_tx, result_rx) = create_result_channel::<String>(num_workers);

    persist_recovery_checkpoint(
        &app_handle,
        &task_id,
        mode.as_ref(),
        &archive_type,
        config.priority,
        resume_from,
        total,
    );
    let mut last_checkpoint_at = Some(Utc::now());

    let start_time = Instant::now();
    let _ = app_handle.emit(
        "recovery-progress",
        RecoveryProgress {
            task_id: task_id.clone(),
            tried: resume_from,
            total,
            speed: 0.0,
            status: RecoveryStatus::Running,
            found_password: None,
            elapsed_seconds: 0.0,
            worker_count: num_workers,
            last_checkpoint_at,
        },
    );

    let mut handles = Some(Vec::new());
    for (shard_start, shard_end) in shards {
        let path_clone: PathBuf = path_buf.clone();
        let mode_clone = Arc::clone(&mode);
        let cancel_clone = Arc::clone(&cancel_flag);
        let tried_clone = Arc::clone(&tried_counter);
        let tx_clone = result_tx.clone();
        let archive_type_clone = archive_type.clone();

        let handle = std::thread::spawn(move || match archive_type_clone {
            ArchiveType::Zip => run_zip_worker_shard(
                path_clone,
                mode_clone,
                shard_start,
                shard_end,
                cancel_clone,
                tried_clone,
                tx_clone,
            ),
            ArchiveType::SevenZ | ArchiveType::Rar => run_stateless_worker_shard(
                path_clone,
                archive_type_clone,
                mode_clone,
                shard_start,
                shard_end,
                cancel_clone,
                tried_clone,
                tx_clone,
            ),
            ArchiveType::Unknown => {}
        });
        handles
            .as_mut()
            .expect("worker handles should be available before joining")
            .push(handle);
    }
    drop(result_tx);

    let mut last_tried: u64 = 0;
    let mut last_poll_time = Instant::now();
    let poll_interval = Duration::from_millis(PROGRESS_INTERVAL_MS);

    let result = loop {
        std::thread::sleep(Duration::from_millis(50));

        let current_tried = tried_counter.load(Ordering::Relaxed);

        match result_rx.try_recv() {
            Ok(password) => {
                cancel_flag.store(true, Ordering::Relaxed);
                if let Some(worker_handles) = handles.take() {
                    let panic_messages = join_worker_handles(worker_handles);
                    for message in panic_messages {
                        log::error!("恢复 worker 在成功收敛后 panic: {}", message);
                    }
                }
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    current_tried as f64 / elapsed
                } else {
                    0.0
                };
                let _ = app_handle.emit(
                    "recovery-progress",
                    RecoveryProgress {
                        task_id: task_id.clone(),
                        tried: current_tried,
                        total,
                        speed,
                        status: RecoveryStatus::Found,
                        found_password: Some(password.clone()),
                        elapsed_seconds: elapsed,
                        worker_count: num_workers,
                        last_checkpoint_at,
                    },
                );
                log::info!(
                    "密码已找到: {} (尝试 {} 次, 耗时 {:.1}s, 速度 {:.0} p/s)",
                    task_id,
                    current_tried,
                    elapsed,
                    speed
                );
                break Ok(RecoveryResult::Found(password));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                if let Some(worker_handles) = handles.take() {
                    let panic_messages = join_worker_handles(worker_handles);
                    if !panic_messages.is_empty() {
                        break Err(format_worker_panic_error(&panic_messages));
                    }
                }

                break if cancel_flag.load(Ordering::Relaxed) {
                    Ok(RecoveryResult::Cancelled)
                } else {
                    Ok(RecoveryResult::Exhausted)
                };
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }

        if cancel_flag.load(Ordering::Relaxed) {
            if let Some(worker_handles) = handles.take() {
                let panic_messages = join_worker_handles(worker_handles);
                for message in panic_messages {
                    log::error!("恢复 worker 在取消收敛后 panic: {}", message);
                }
            }
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                current_tried as f64 / elapsed
            } else {
                0.0
            };
            persist_recovery_checkpoint(
                &app_handle,
                &task_id,
                mode.as_ref(),
                &archive_type,
                config.priority,
                current_tried,
                total,
            );
            last_checkpoint_at = Some(Utc::now());
            let _ = app_handle.emit(
                "recovery-progress",
                RecoveryProgress {
                    task_id: task_id.clone(),
                    tried: current_tried,
                    total,
                    speed,
                    status: RecoveryStatus::Cancelled,
                    found_password: None,
                    elapsed_seconds: elapsed,
                    worker_count: num_workers,
                    last_checkpoint_at,
                },
            );
            log::info!(
                "恢复任务已取消: {} (已尝试 {} 个密码)",
                task_id,
                current_tried
            );
            break Ok(RecoveryResult::Cancelled);
        }

        let now = Instant::now();
        if now.duration_since(last_poll_time) >= poll_interval {
            let elapsed = start_time.elapsed().as_secs_f64();
            let delta = current_tried.saturating_sub(last_tried);
            let interval_secs = now.duration_since(last_poll_time).as_secs_f64();
            let speed = if interval_secs > 0.0 {
                delta as f64 / interval_secs
            } else {
                0.0
            };
            last_tried = current_tried;
            last_poll_time = now;
            persist_recovery_checkpoint(
                &app_handle,
                &task_id,
                mode.as_ref(),
                &archive_type,
                config.priority,
                current_tried,
                total,
            );
            last_checkpoint_at = Some(Utc::now());

            let _ = app_handle.emit(
                "recovery-progress",
                RecoveryProgress {
                    task_id: task_id.clone(),
                    tried: current_tried,
                    total,
                    speed,
                    status: RecoveryStatus::Running,
                    found_password: None,
                    elapsed_seconds: elapsed,
                    worker_count: num_workers,
                    last_checkpoint_at,
                },
            );
        }
    };

    if let Ok(ref r) = result {
        if let RecoveryResult::Exhausted = r {
            let elapsed = start_time.elapsed().as_secs_f64();
            let current_tried = tried_counter.load(Ordering::Relaxed);
            let speed = if elapsed > 0.0 {
                current_tried as f64 / elapsed
            } else {
                0.0
            };
            let _ = app_handle.emit(
                "recovery-progress",
                RecoveryProgress {
                    task_id: task_id.clone(),
                    tried: current_tried,
                    total,
                    speed,
                    status: RecoveryStatus::Exhausted,
                    found_password: None,
                    elapsed_seconds: elapsed,
                    worker_count: num_workers,
                    last_checkpoint_at,
                },
            );
            log::info!(
                "密码穷尽: {} (尝试 {} 次, 耗时 {:.1}s, 速度 {:.0} p/s)",
                task_id,
                current_tried,
                elapsed,
                speed
            );
        }
    }

    result
}
