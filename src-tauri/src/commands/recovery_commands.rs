use tauri::{command, AppHandle, Manager, State};

use crate::db::Database;
use crate::domain::recovery::{AttackMode, RecoveryConfig, RecoveryManager};
use crate::errors::AppError;
use crate::services::recovery_service::{self, RecoveryResult};

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

    let file_path = task.file_path.clone();

    // 2. 解析攻击模式
    let attack_mode = match mode.as_str() {
        "dictionary" => {
            #[derive(serde::Deserialize)]
            struct DictConfig {
                wordlist: Vec<String>,
            }
            let cfg: DictConfig = serde_json::from_str(&config_json)?;
            if cfg.wordlist.is_empty() {
                return Err(AppError::InvalidArgument(
                    "字典列表不能为空".to_string(),
                ));
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
                return Err(AppError::InvalidArgument(
                    "字符集不能为空".to_string(),
                ));
            }
            if cfg.min_length == 0 {
                return Err(AppError::InvalidArgument(
                    "最小长度必须大于 0".to_string(),
                ));
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

    // 3. 检查是否已有运行中的恢复任务
    if recovery_manager.is_running(&task_id) {
        return Err(AppError::InvalidArgument(format!(
            "该任务已有运行中的恢复: {}",
            task_id
        )));
    }

    // 4. 注册取消标志
    let cancel_flag = recovery_manager.register(&task_id);

    // 5. 更新任务状态为 processing
    db.update_task_status(&task_id, "processing", None)?;
    log::info!("恢复任务已启动: {} (模式: {})", task_id, mode);

    // 6. 在后台线程中运行恢复
    let task_id_clone = task_id.clone();
    let app_handle_clone = app_handle.clone();

    std::thread::spawn(move || {
        let result = recovery_service::run_recovery(
            config,
            file_path,
            app_handle_clone.clone(),
            cancel_flag,
        );

        // 更新任务状态
        // 注意：这里无法直接用 State，需要从 app_handle 获取
        let db = app_handle_clone.state::<Database>();
        let recovery_mgr = app_handle_clone.state::<RecoveryManager>();

        match result {
            Ok(RecoveryResult::Found(password)) => {
                // 找到密码 → succeeded，密码存入 error_message 字段（后续可加专用列）
                log::info!("恢复成功: {} 密码={}", task_id_clone, password);
                let _ = db.update_task_status(
                    &task_id_clone,
                    "succeeded",
                    Some(&format!("密码: {}", password)),
                );
            }
            Ok(RecoveryResult::Exhausted) => {
                // 穷尽所有密码 → failed
                log::info!("恢复已穷尽: {}", task_id_clone);
                let _ = db.update_task_status(
                    &task_id_clone,
                    "failed",
                    Some("已穷尽所有候选密码，未找到匹配密码"),
                );
            }
            Ok(RecoveryResult::Cancelled) => {
                // 用户取消 → failed（取消信息）
                log::info!("恢复已取消: {}", task_id_clone);
                let _ = db.update_task_status(
                    &task_id_clone,
                    "failed",
                    Some("用户已取消恢复任务"),
                );
            }
            Err(err) => {
                // 发生错误 → failed
                log::error!("恢复出错: {} - {}", task_id_clone, err);
                let _ = db.update_task_status(
                    &task_id_clone,
                    "failed",
                    Some(&format!("恢复出错: {}", err)),
                );
            }
        }

        // 清理取消标志
        recovery_mgr.remove(&task_id_clone);
    });

    Ok(())
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
