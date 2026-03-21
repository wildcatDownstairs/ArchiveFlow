// commands.rs 只负责把前端可调用的 Rust 命令集中注册到 Tauri Builder。
// 这样做的好处是：
//   - 命令清单集中，便于排查"某个命令为什么前端调不到"
//   - app/mod.rs 不会被一长串 generate_handler! 挤满

/// 把所有 Tauri 命令注册到 Builder 上。
///
/// 这里使用 `tauri::Wry` 这个桌面运行时，而不是继续做成泛型，
/// 是因为 `generate_handler!` 里包含了带 `AppHandle` 参数的命令。
/// 在当前 Tauri 版本下，这种命令注册放到泛型 `Runtime` 上会让宏推断失败。
/// 所以这里选择“对桌面端保持具体类型”，换取更稳定、可读的装配代码。
pub(crate) fn register(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.invoke_handler(tauri::generate_handler![
        crate::commands::task_commands::get_tasks,
        crate::commands::task_commands::create_task,
        crate::commands::task_commands::get_task,
        crate::commands::task_commands::delete_task,
        crate::commands::task_commands::update_task_status,
        crate::commands::task_commands::get_app_data_dir,
        crate::commands::task_commands::clear_all_tasks,
        crate::commands::task_commands::get_stats,
        crate::commands::archive_commands::inspect_archive,
        crate::commands::archive_commands::import_archive,
        crate::commands::audit_commands::get_audit_events,
        crate::commands::audit_commands::get_task_audit_events,
        crate::commands::audit_commands::clear_audit_events,
        crate::commands::audit_commands::record_setting_change,
        crate::commands::recovery_commands::start_recovery,
        crate::commands::recovery_commands::detect_hashcat,
        crate::commands::recovery_commands::get_recovery_checkpoint,
        crate::commands::recovery_commands::get_scheduled_recovery,
        crate::commands::recovery_commands::get_recovery_scheduler_snapshot,
        crate::commands::recovery_commands::set_recovery_scheduler_limit,
        crate::commands::recovery_commands::resume_recovery,
        crate::commands::recovery_commands::pause_recovery,
        crate::commands::recovery_commands::cancel_recovery,
        crate::commands::export_commands::export_tasks,
    ])
}
