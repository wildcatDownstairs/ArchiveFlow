use tauri::{command, AppHandle, Manager, State};

use crate::db::Database;
use crate::domain::audit::AuditEventType;
use crate::domain::recovery::{AttackMode, RecoveryConfig, RecoveryManager};
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
    db: State<'_, Database>,
    recovery_manager: State<'_, RecoveryManager>,
    app_handle: AppHandle,
) -> Result<(), AppError> {
    // 1. 获取任务信息
    let task = db
        .get_task_by_id(&task_id)?
        .ok_or_else(|| AppError::TaskNotFound(task_id.clone()))?;

    if !supports_password_recovery(&task.archive_type) {
        return Err(AppError::InvalidArgument(
            "当前归档类型不支持密码恢复".to_string(),
        ));
    }

    if !matches!(
        task.status,
        TaskStatus::Ready
            | TaskStatus::Failed
            | TaskStatus::Cancelled
            | TaskStatus::Exhausted
            | TaskStatus::Interrupted
    ) {
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

    let file_path = task.file_path.clone();
    let archive_type = task.archive_type.clone();

    // 2. 解析攻击模式
    let attack_mode = match mode.as_str() {
        "dictionary" => {
            #[derive(serde::Deserialize)]
            struct DictConfig {
                wordlist: Vec<String>,
            }
            let cfg: DictConfig = serde_json::from_str(&config_json)?;
            if cfg.wordlist.is_empty() {
                return Err(AppError::InvalidArgument("字典列表不能为空".to_string()));
            }
            AttackMode::Dictionary {
                wordlist: cfg.wordlist,
            }
        }
        "bruteforce" => {
            #[derive(serde::Deserialize)]
            struct BfConfig {
                charset: String,
                min_length: usize,
                max_length: usize,
            }
            let cfg: BfConfig = serde_json::from_str(&config_json)?;
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
            AttackMode::BruteForce {
                charset: cfg.charset,
                min_length: cfg.min_length,
                max_length: cfg.max_length,
            }
        }
        _ => {
            return Err(AppError::InvalidArgument(format!(
                "不支持的攻击模式: {}",
                mode
            )));
        }
    };

    let config = RecoveryConfig {
        task_id: task_id.clone(),
        mode: attack_mode,
    };

    // 3. 原子注册恢复任务，避免重复启动竞态
    let cancel_flag = recovery_manager
        .try_register(&task_id)
        .map_err(|_| AppError::InvalidArgument(format!("该任务已有运行中的恢复: {}", task_id)))?;

    // 4. 更新任务状态为 processing，并清理上一次恢复结果
    db.update_task_recovery_result(&task_id, "processing", None, None)?;

    // 记录恢复任务启动审计事件
    let _ = audit_service::log_audit_event(
        &db,
        AuditEventType::RecoveryStarted,
        Some(task_id.clone()),
        format!("启动密码恢复: {} (模式: {})", task_id, mode),
    );

    log::info!("恢复任务已启动: {} (模式: {})", task_id, mode);

    // 5. 在后台线程中运行恢复
    let task_id_clone = task_id.clone();
    let app_handle_clone = app_handle.clone();

    std::thread::spawn(move || {
        let result = recovery_service::run_recovery(
            config,
            file_path,
            archive_type,
            app_handle_clone.clone(),
            cancel_flag,
        );

        // 更新任务状态
        // 注意：这里无法直接用 State，需要从 app_handle 获取
        let db = app_handle_clone.state::<Database>();
        let recovery_mgr = app_handle_clone.state::<RecoveryManager>();

        match result {
            Ok(RecoveryResult::Found(password)) => {
                // 找到密码 → succeeded，密码持久化到专用字段
                log::info!("恢复成功: {} 密码={}", task_id_clone, password);
                let _ = db.update_task_recovery_result(
                    &task_id_clone,
                    "succeeded",
                    None,
                    Some(&password),
                );
                // 记录密码恢复成功审计事件
                let _ = audit_service::log_audit_event(
                    &db,
                    AuditEventType::RecoverySucceeded,
                    Some(task_id_clone.clone()),
                    format!("密码恢复成功: {}", task_id_clone),
                );
            }
            Ok(RecoveryResult::Exhausted) => {
                // 穷尽所有密码 → exhausted
                log::info!("恢复已穷尽: {}", task_id_clone);
                let _ = db.update_task_recovery_result(&task_id_clone, "exhausted", None, None);
                // 记录密码穷尽审计事件
                let _ = audit_service::log_audit_event(
                    &db,
                    AuditEventType::RecoveryExhausted,
                    Some(task_id_clone.clone()),
                    format!("密码穷尽未找到: {}", task_id_clone),
                );
            }
            Ok(RecoveryResult::Cancelled) => {
                // 用户取消 → cancelled
                log::info!("恢复已取消: {}", task_id_clone);
                let _ = db.update_task_recovery_result(&task_id_clone, "cancelled", None, None);
                // 记录用户取消恢复审计事件
                let _ = audit_service::log_audit_event(
                    &db,
                    AuditEventType::RecoveryCancelled,
                    Some(task_id_clone.clone()),
                    format!("用户取消恢复: {}", task_id_clone),
                );
            }
            Err(err) => {
                // 发生错误 → failed
                log::error!("恢复出错: {} - {}", task_id_clone, err);
                let _ = db.update_task_recovery_result(
                    &task_id_clone,
                    "failed",
                    Some(&format!("恢复出错: {}", err)),
                    None,
                );
                // 记录恢复出错审计事件
                let _ = audit_service::log_audit_event(
                    &db,
                    AuditEventType::RecoveryFailed,
                    Some(task_id_clone.clone()),
                    format!("恢复出错: {} - {}", task_id_clone, err),
                );
            }
        }

        // 清理取消标志
        recovery_mgr.remove(&task_id_clone);
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::supports_password_recovery;
    use crate::domain::task::ArchiveType;

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
}

/// 取消正在运行的恢复任务
#[command]
pub async fn cancel_recovery(
    task_id: String,
    recovery_manager: State<'_, RecoveryManager>,
) -> Result<(), AppError> {
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
