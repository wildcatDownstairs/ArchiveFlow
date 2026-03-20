use crate::db::Database;
use crate::domain::audit::AuditEventType;
use crate::domain::recovery::{RecoveryManager, RecoveryScheduler};
use crate::services::audit_service;
use tauri::Manager;

/// 把应用启动时的一次性初始化逻辑挂到 Builder 上。
///
/// 为什么单独拆这个文件？
///   - `setup()` 闭包通常会越来越大（数据库、日志、迁移、托盘、窗口恢复等）
///   - 单独成模块后，crate 根入口更容易阅读
///   - 测试或重构初始化流程时，关注点会更集中
pub(crate) fn configure<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.setup(|app| {
        // Debug 构建启用详细日志，方便开发时排查问题。
        if cfg!(debug_assertions) {
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Debug)
                    .build(),
            )?;
        }

        // app_data_dir 是跨平台的应用数据目录。
        // 把数据库、缓存等持久化文件都放在这里，符合桌面应用的约定。
        let app_dir = app.path().app_data_dir().expect("无法获取应用数据目录");
        std::fs::create_dir_all(&app_dir).expect("无法创建应用数据目录");
        let db = Database::new(app_dir).expect("数据库初始化失败");

        // 启动时把残留 processing 任务修正成 interrupted，避免 UI 永久卡在处理中。
        let interrupted_tasks = db.interrupt_processing_tasks().unwrap_or_else(|error| {
            log::error!("启动残留任务修复失败: {error}");
            vec![]
        });
        for task in interrupted_tasks {
            let _ = audit_service::log_audit_event(
                &db,
                AuditEventType::TaskInterrupted,
                Some(task.id.clone()),
                format!("启动修复中断任务: {}", task.file_name),
            );
        }

        // manage() 相当于把共享状态注入到 Tauri 容器里。
        // 后续命令函数只要声明 State<'_, T> 参数，就能拿到这里注册的实例。
        app.manage(db);
        app.manage(RecoveryManager::new());
        app.manage(RecoveryScheduler::new());

        log::info!("ArchiveFlow 启动成功");
        Ok(())
    })
}
