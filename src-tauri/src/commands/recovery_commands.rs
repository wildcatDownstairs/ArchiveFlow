use std::sync::{atomic::AtomicBool, Arc};

use tauri::{command, AppHandle, Manager, State};

use crate::db::Database;
use crate::domain::audit::AuditEventType;
use crate::domain::recovery::{
    AttackMode, RecoveryCheckpoint, RecoveryConfig, RecoveryManager, RecoveryScheduler,
    RecoverySchedulerSnapshot, ScheduledRecovery, ScheduledRecoveryState,
};
use crate::domain::task::{ArchiveType, TaskStatus};
use crate::errors::AppError;
use crate::services::audit_service;
use crate::services::recovery_service::{self, RecoveryResult};

fn supports_password_recovery(archive_type: &ArchiveType) -> bool {
    matches!(
        archive_type,
        ArchiveType::Zip | ArchiveType::SevenZ | ArchiveType::Rar
    )
}

fn describe_attack_mode(mode: &AttackMode) -> String {
    match mode {
        AttackMode::Dictionary { wordlist } => {
            format!("字典攻击 (候选数: {})", wordlist.len())
        }
        AttackMode::BruteForce {
            charset,
            min_length,
            max_length,
        } => format!(
            "暴力破解 (字符集长度: {}, 长度范围: {}-{})",
            charset.chars().count(),
            min_length,
            max_length
        ),
        AttackMode::Mask { mask } => format!("掩码攻击 (模式: {})", mask),
    }
}

/// 统一判断任务状态是否允许启动或继续恢复。
/// 这里单独提成函数，避免 `start_recovery` 和 `resume_recovery`
/// 两条命令未来出现状态判断不一致的问题。
fn can_start_recovery(status: &TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Ready
            | TaskStatus::Failed
            | TaskStatus::Cancelled
            | TaskStatus::Exhausted
            | TaskStatus::Interrupted
    )
}

fn parse_attack_mode(mode: &str, config_json: &str) -> Result<AttackMode, AppError> {
    match mode {
        "dictionary" => {
            #[derive(serde::Deserialize)]
            struct DictConfig {
                wordlist: Vec<String>,
            }
            let cfg: DictConfig = serde_json::from_str(config_json)?;
            if cfg.wordlist.is_empty() {
                return Err(AppError::InvalidArgument("字典列表不能为空".to_string()));
            }
            Ok(AttackMode::Dictionary {
                wordlist: cfg.wordlist,
            })
        }
        "bruteforce" => {
            #[derive(serde::Deserialize)]
            struct BfConfig {
                charset: String,
                min_length: usize,
                max_length: usize,
            }
            let cfg: BfConfig = serde_json::from_str(config_json)?;
            if cfg.charset.is_empty() {
                return Err(AppError::InvalidArgument("字符集不能为空".to_string()));
            }
            if cfg.min_length == 0 {
                return Err(AppError::InvalidArgument("最小长度必须大于 0".to_string()));
            }
            if cfg.max_length < cfg.min_length {
                return Err(AppError::InvalidArgument(
                    "最大长度不能小于最小长度".to_string(),
                ));
            }
            Ok(AttackMode::BruteForce {
                charset: cfg.charset,
                min_length: cfg.min_length,
                max_length: cfg.max_length,
            })
        }
        "mask" => {
            #[derive(serde::Deserialize)]
            struct MaskConfig {
                mask: String,
            }
            let cfg: MaskConfig = serde_json::from_str(config_json)?;
            if cfg.mask.trim().is_empty() {
                return Err(AppError::InvalidArgument("掩码不能为空".to_string()));
            }
            Ok(AttackMode::Mask { mask: cfg.mask })
        }
        _ => Err(AppError::InvalidArgument(format!(
            "不支持的攻击模式: {}",
            mode
        ))),
    }
}

/// 调度器从队列中取出可运行任务并启动后台 worker。
/// 这段逻辑会在“新任务入队”“恢复任务结束”“并发上限调整”后重复调用。
fn dispatch_scheduled_recoveries(app_handle: &AppHandle) {
    let db = app_handle.state::<Database>();
    let recovery_manager = app_handle.state::<RecoveryManager>();
    let scheduler = app_handle.state::<RecoveryScheduler>();

    for scheduled in scheduler.take_dispatchable_tasks() {
        let task = match db.get_task_by_id(&scheduled.task_id) {
            Ok(Some(task)) => task,
            Ok(None) => {
                scheduler.finish(&scheduled.task_id);
                continue;
            }
            Err(error) => {
                log::error!("读取调度任务失败: task={} error={}", scheduled.task_id, error);
                let _ = scheduler.mark_queued(&scheduled.task_id);
                continue;
            }
        };

        if !supports_password_recovery(&task.archive_type) || !can_start_recovery(&task.status) {
            let _ = scheduler.mark_queued(&scheduled.task_id);
            continue;
        }

        let cancel_flag = match recovery_manager.try_register(&scheduled.task_id) {
            Ok(flag) => flag,
            Err(_) => {
                let _ = scheduler.mark_queued(&scheduled.task_id);
                continue;
            }
        };

        if let Err(error) =
            db.update_task_recovery_result(&scheduled.task_id, "processing", None, None)
        {
            log::error!(
                "更新调度任务状态失败: task={} error={}",
                scheduled.task_id,
                error
            );
            recovery_manager.remove(&scheduled.task_id);
            let _ = scheduler.mark_queued(&scheduled.task_id);
            continue;
        }

        let _ = audit_service::log_audit_event(
            &db,
            AuditEventType::RecoveryStarted,
            Some(scheduled.task_id.clone()),
            format!(
                "调度启动密码恢复: {} ({:?}, {}, 优先级 {})",
                task.file_name,
                task.archive_type,
                describe_attack_mode(&scheduled.mode),
                scheduled.priority
            ),
        );

        spawn_recovery_worker(
            scheduled.task_id.clone(),
            task.file_path,
            task.archive_type,
            RecoveryConfig {
                task_id: scheduled.task_id.clone(),
                mode: scheduled.mode.clone(),
            },
            cancel_flag,
            app_handle.clone(),
        );
    }
}

/// 后台线程真正执行恢复，并在结束时统一处理 DB 状态和审计。
/// 对 Rust 新手来说，这里把“线程里做什么”集中在一个函数里更容易跟踪，
/// 也能避免 `start` / `resume` 两个入口各自复制一份收尾逻辑。
fn spawn_recovery_worker(
    task_id: String,
    file_path: String,
    archive_type: ArchiveType,
    config: RecoveryConfig,
    cancel_flag: Arc<AtomicBool>,
    app_handle: AppHandle,
) {
    std::thread::spawn(move || {
        let result = recovery_service::run_recovery(
            config,
            file_path,
            archive_type,
            app_handle.clone(),
            cancel_flag,
        );

        let db = app_handle.state::<Database>();
        let recovery_mgr = app_handle.state::<RecoveryManager>();
        let scheduler = app_handle.state::<RecoveryScheduler>();

        match result {
            Ok(RecoveryResult::Found(password)) => {
                log::info!("恢复成功: {} 密码={}", task_id, password);
                let _ =
                    db.update_task_recovery_result(&task_id, "succeeded", None, Some(&password));
                // 成功后 checkpoint 已经没有继续意义，主动删除，避免下次误续跑。
                let _ = db.delete_recovery_checkpoint(&task_id);
                let _ = scheduler.finish(&task_id);
                let _ = audit_service::log_audit_event(
                    &db,
                    AuditEventType::RecoverySucceeded,
                    Some(task_id.clone()),
                    format!("密码恢复成功: {}", task_id),
                );
            }
            Ok(RecoveryResult::Exhausted) => {
                log::info!("恢复已穷尽: {}", task_id);
                let _ = db.update_task_recovery_result(&task_id, "exhausted", None, None);
                // 候选空间已经全部跑完，保留 checkpoint 没有价值。
                let _ = db.delete_recovery_checkpoint(&task_id);
                let _ = scheduler.finish(&task_id);
                let _ = audit_service::log_audit_event(
                    &db,
                    AuditEventType::RecoveryExhausted,
                    Some(task_id.clone()),
                    format!("密码穷尽未找到: {}", task_id),
                );
            }
            Ok(RecoveryResult::Cancelled) => {
                if matches!(
                    scheduler.get_task(&task_id).as_ref().map(|task| &task.state),
                    Some(ScheduledRecoveryState::Paused)
                ) {
                    log::info!("恢复已暂停: {}", task_id);
                    let _ = db.update_task_recovery_result(
                        &task_id,
                        "cancelled",
                        Some("恢复已暂停，可从断点继续"),
                        None,
                    );
                    let _ = audit_service::log_audit_event(
                        &db,
                        AuditEventType::RecoveryPaused,
                        Some(task_id.clone()),
                        format!("恢复任务已暂停: {}", task_id),
                    );
                } else {
                    log::info!("恢复已取消: {}", task_id);
                    let _ = scheduler.finish(&task_id);
                    let _ = db.update_task_recovery_result(&task_id, "cancelled", None, None);
                    // 取消时不删除 checkpoint，这样用户下次可以继续跑。
                    let _ = audit_service::log_audit_event(
                        &db,
                        AuditEventType::RecoveryCancelled,
                        Some(task_id.clone()),
                        format!("用户取消恢复: {}", task_id),
                    );
                }
            }
            Err(err) => {
                log::error!("恢复出错: {} - {}", task_id, err);
                let _ = scheduler.finish(&task_id);
                let _ = db.update_task_recovery_result(
                    &task_id,
                    "failed",
                    Some(&format!("恢复出错: {}", err)),
                    None,
                );
                let _ = audit_service::log_audit_event(
                    &db,
                    AuditEventType::RecoveryFailed,
                    Some(task_id.clone()),
                    format!("恢复出错: {} - {}", task_id, err),
                );
            }
        }

        // 无论成功、失败还是取消，都要把运行中的取消标志清理掉，
        // 否则后面再次启动同一个任务会被错误地当成“仍在运行”。
        recovery_mgr.remove(&task_id);
        dispatch_scheduled_recoveries(&app_handle);
    });
}

#[command]
pub async fn get_recovery_checkpoint(
    task_id: String,
    db: State<'_, Database>,
) -> Result<Option<RecoveryCheckpoint>, AppError> {
    db.get_recovery_checkpoint(&task_id)
}

/// 启动密码恢复任务
///
/// - `task_id`: 任务 ID
/// - `mode`: 攻击模式 ("dictionary" 或 "bruteforce")
/// - `config_json`: 模式相关配置的 JSON 字符串
///   - 字典模式: `{"wordlist": ["pass1", "pass2", ...]}`
///   - 暴力模式: `{"charset": "abc...z0...9", "min_length": 1, "max_length": 6}`
#[command]
pub async fn start_recovery(
    task_id: String,
    mode: String,
    config_json: String,
    priority: Option<i32>,
    db: State<'_, Database>,
    scheduler: State<'_, RecoveryScheduler>,
    app_handle: AppHandle,
) -> Result<ScheduledRecoveryState, AppError> {
    // 1. 获取任务信息
    let task = db
        .get_task_by_id(&task_id)?
        .ok_or_else(|| AppError::TaskNotFound(task_id.clone()))?;

    if !supports_password_recovery(&task.archive_type) {
        return Err(AppError::InvalidArgument(
            "当前归档类型不支持密码恢复".to_string(),
        ));
    }

    if !can_start_recovery(&task.status) {
        return Err(AppError::InvalidArgument(format!(
            "当前任务状态不允许启动恢复: {}",
            task.status.as_str()
        )));
    }

    if !task
        .archive_info
        .as_ref()
        .map(|info| info.is_encrypted)
        .unwrap_or(false)
    {
        return Err(AppError::InvalidArgument(
            "当前归档没有可恢复的加密内容".to_string(),
        ));
    }

    let attack_mode = parse_attack_mode(&mode, &config_json)?;
    scheduler
        .enqueue(&task_id, attack_mode.clone(), priority.unwrap_or(0))
        .map_err(|_| AppError::InvalidArgument(format!("该任务已在调度队列中: {}", task_id)))?;

    dispatch_scheduled_recoveries(&app_handle);

    let state = scheduler
        .get_task(&task_id)
        .map(|entry| entry.state)
        .unwrap_or(ScheduledRecoveryState::Running);

    if state == ScheduledRecoveryState::Queued {
        let _ = audit_service::log_audit_event(
            &db,
            AuditEventType::RecoveryQueued,
            Some(task_id.clone()),
            format!(
                "恢复任务已入队: {} ({:?}, {}, 优先级 {})",
                task.file_name,
                task.archive_type,
                describe_attack_mode(&attack_mode),
                priority.unwrap_or(0)
            ),
        );
    }

    log::info!("恢复任务已调度: {} (模式: {}, 状态: {:?})", task_id, mode, state);
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::{can_start_recovery, describe_attack_mode, supports_password_recovery};
    use crate::domain::recovery::AttackMode;
    use crate::domain::task::{ArchiveType, TaskStatus};

    #[test]
    fn password_recovery_supports_zip_7z_and_rar() {
        assert!(supports_password_recovery(&ArchiveType::Zip));
        assert!(supports_password_recovery(&ArchiveType::SevenZ));
        assert!(supports_password_recovery(&ArchiveType::Rar));
    }

    #[test]
    fn password_recovery_rejects_unknown_archive_type() {
        assert!(!supports_password_recovery(&ArchiveType::Unknown));
    }

    #[test]
    fn describe_dictionary_attack_mode() {
        let mode = AttackMode::Dictionary {
            wordlist: vec!["a".into(), "b".into(), "c".into()],
        };
        assert_eq!(describe_attack_mode(&mode), "字典攻击 (候选数: 3)");
    }

    #[test]
    fn describe_bruteforce_attack_mode() {
        let mode = AttackMode::BruteForce {
            charset: "abc123".into(),
            min_length: 2,
            max_length: 5,
        };
        assert_eq!(
            describe_attack_mode(&mode),
            "暴力破解 (字符集长度: 6, 长度范围: 2-5)"
        );
    }

    #[test]
    fn describe_mask_attack_mode() {
        let mode = AttackMode::Mask {
            mask: "?d?dAB".into(),
        };
        assert_eq!(describe_attack_mode(&mode), "掩码攻击 (模式: ?d?dAB)");
    }

    #[test]
    fn interrupted_task_can_resume() {
        assert!(can_start_recovery(&TaskStatus::Interrupted));
    }
}

/// 继续上一次已保存断点的恢复任务。
/// 这里不要求前端重新提交完整配置，而是直接读取数据库里的 checkpoint。
#[command]
pub async fn resume_recovery(
    task_id: String,
    db: State<'_, Database>,
    scheduler: State<'_, RecoveryScheduler>,
    app_handle: AppHandle,
) -> Result<ScheduledRecoveryState, AppError> {
    let task = db
        .get_task_by_id(&task_id)?
        .ok_or_else(|| AppError::TaskNotFound(task_id.clone()))?;
    let checkpoint = db
        .get_recovery_checkpoint(&task_id)?
        .ok_or_else(|| AppError::InvalidArgument("当前任务没有可继续的恢复断点".to_string()))?;

    if !supports_password_recovery(&task.archive_type) {
        return Err(AppError::InvalidArgument(
            "当前归档类型不支持密码恢复".to_string(),
        ));
    }
    if !can_start_recovery(&task.status) {
        return Err(AppError::InvalidArgument(format!(
            "当前任务状态不允许继续恢复: {}",
            task.status.as_str()
        )));
    }

    if let Some(existing) = scheduler.get_task(&task_id) {
        if existing.state != ScheduledRecoveryState::Paused {
            return Err(AppError::InvalidArgument("当前任务不处于暂停状态".to_string()));
        }
        let resumed = scheduler
            .resume(&task_id)
            .ok_or_else(|| AppError::InvalidArgument("当前任务无法继续调度".to_string()))?;
        dispatch_scheduled_recoveries(&app_handle);
        let state = scheduler
            .get_task(&task_id)
            .map(|entry| entry.state)
            .unwrap_or(resumed.state);
        let _ = audit_service::log_audit_event(
            &db,
            AuditEventType::RecoveryResumed,
            Some(task_id.clone()),
            format!("恢复任务继续排队/运行: {} ({:?})", task.file_name, state),
        );
        return Ok(state);
    }

    scheduler
        .enqueue(&task_id, checkpoint.mode.clone(), 0)
        .map_err(|_| AppError::InvalidArgument(format!("该任务已在调度队列中: {}", task_id)))?;

    dispatch_scheduled_recoveries(&app_handle);

    let state = scheduler
        .get_task(&task_id)
        .map(|entry| entry.state)
        .unwrap_or(ScheduledRecoveryState::Running);
    let _ = audit_service::log_audit_event(
        &db,
        AuditEventType::RecoveryResumed,
        Some(task_id.clone()),
        format!(
            "继续密码恢复: {} ({:?}, {}, 已尝试 {}/{})",
            task.file_name,
            task.archive_type,
            describe_attack_mode(&checkpoint.mode),
            checkpoint.tried,
            checkpoint.total
        ),
    );

    Ok(state)
}

/// 取消正在运行的恢复任务
#[command]
pub async fn cancel_recovery(
    task_id: String,
    db: State<'_, Database>,
    scheduler: State<'_, RecoveryScheduler>,
    recovery_manager: State<'_, RecoveryManager>,
) -> Result<(), AppError> {
    if let Some(scheduled) = scheduler.get_task(&task_id) {
        let _ = scheduler.finish(&task_id);

        if scheduled.state != ScheduledRecoveryState::Running {
            let _ = audit_service::log_audit_event(
                &db,
                AuditEventType::RecoveryCancelled,
                Some(task_id.clone()),
                format!("已取消排队中的恢复任务: {}", task_id),
            );
            return Ok(());
        }
    }

    if recovery_manager.cancel(&task_id) {
        log::info!("已发送取消信号: {}", task_id);
        Ok(())
    } else {
        Err(AppError::InvalidArgument(format!(
            "没有找到运行中的恢复任务: {}",
            task_id
        )))
    }
}

/// 查询调度器中单个任务的调度信息。
/// 前端用于显示某个任务当前在队列中的状态（排队中/运行中/暂停）。
#[command]
pub async fn get_scheduled_recovery(
    task_id: String,
    scheduler: State<'_, RecoveryScheduler>,
) -> Result<Option<ScheduledRecovery>, AppError> {
    Ok(scheduler.get_task(&task_id))
}

/// 返回调度器的完整快照，包括当前并发限制和所有已调度任务。
/// 前端用于渲染调度队列列表。
#[command]
pub async fn get_recovery_scheduler_snapshot(
    scheduler: State<'_, RecoveryScheduler>,
) -> Result<RecoverySchedulerSnapshot, AppError> {
    Ok(scheduler.snapshot())
}

/// 更新调度器允许的最大并发恢复数量。
/// 传入 0 或负数时会被强制钳位为 1。
#[command]
pub async fn set_recovery_scheduler_limit(
    max_concurrent: usize,
    scheduler: State<'_, RecoveryScheduler>,
    db: State<'_, Database>,
    app_handle: AppHandle,
) -> Result<RecoverySchedulerSnapshot, AppError> {
    let snapshot = scheduler.set_max_concurrent(max_concurrent);
    let _ = audit_service::log_audit_event(
        &db,
        AuditEventType::SettingChanged,
        None,
        format!("调度器并发上限更新为 {}", snapshot.max_concurrent),
    );
    dispatch_scheduled_recoveries(&app_handle);
    Ok(snapshot)
}

/// 暂停指定恢复任务。
/// 如果任务正在运行，会同时发出取消信号，让 worker 在安全检查点停止。
#[command]
pub async fn pause_recovery(
    task_id: String,
    scheduler: State<'_, RecoveryScheduler>,
    recovery_manager: State<'_, RecoveryManager>,
    db: State<'_, Database>,
) -> Result<(), AppError> {
    let scheduled = scheduler
        .get_task(&task_id)
        .ok_or_else(|| AppError::InvalidArgument("当前任务不在调度器中".to_string()))?;

    let _ = scheduler.pause(&task_id);
    if scheduled.state == ScheduledRecoveryState::Running {
        if !recovery_manager.cancel(&task_id) {
            let _ = scheduler.resume(&task_id);
            return Err(AppError::InvalidArgument(format!(
                "没有找到运行中的恢复任务: {}",
                task_id
            )));
        }
    } else {
        let _ = audit_service::log_audit_event(
            &db,
            AuditEventType::RecoveryPaused,
            Some(task_id.clone()),
            format!("恢复任务已暂停排队: {}", task_id),
        );
    }

    Ok(())
}
