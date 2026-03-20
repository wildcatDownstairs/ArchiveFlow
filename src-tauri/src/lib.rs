mod commands;
mod db;
mod domain;
mod errors;
mod services;

use db::Database;
use domain::audit::AuditEventType;
use domain::recovery::RecoveryManager;
use services::audit_service;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // 日志插件
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Debug)
                        .build(),
                )?;
            }

            // 初始化数据库
            let app_dir = app.path().app_data_dir().expect("无法获取应用数据目录");
            std::fs::create_dir_all(&app_dir).expect("无法创建应用数据目录");
            let db = Database::new(app_dir).expect("数据库初始化失败");

            let interrupted_tasks = db
                .interrupt_processing_tasks()
                .expect("启动残留任务修复失败");
            for task in interrupted_tasks {
                let _ = audit_service::log_audit_event(
                    &db,
                    AuditEventType::TaskInterrupted,
                    Some(task.id.clone()),
                    format!("启动修复中断任务: {}", task.file_name),
                );
            }

            app.manage(db);

            // 初始化恢复任务管理器
            app.manage(RecoveryManager::new());

            log::info!("ArchiveFlow 启动成功");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::task_commands::get_tasks,
            commands::task_commands::create_task,
            commands::task_commands::get_task,
            commands::task_commands::delete_task,
            commands::task_commands::update_task_status,
            commands::task_commands::get_app_data_dir,
            commands::task_commands::clear_all_tasks,
            commands::task_commands::get_stats,
            commands::archive_commands::inspect_archive,
            commands::archive_commands::import_archive,
            commands::audit_commands::get_audit_events,
            commands::audit_commands::get_task_audit_events,
            commands::audit_commands::clear_audit_events,
            commands::recovery_commands::start_recovery,
            commands::recovery_commands::cancel_recovery,
        ])
        .run(tauri::generate_context!())
        .expect("启动 ArchiveFlow 时出错");
}
