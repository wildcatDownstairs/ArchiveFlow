// lib.rs 是这个"库 crate"的根模块。
// Rust 项目通常把逻辑放在 lib.rs，main.rs 只调用 lib.rs 里的入口函数。
// 这样拆分的好处：测试代码可以直接引用库 crate，而不依赖 main.rs。

// mod 关键字声明子模块。Rust 会去找 commands/mod.rs、db/mod.rs 等文件。
// 每个子模块默认是私有的（模块外无法访问），除非用 pub mod 或 pub use 暴露。
mod commands;
mod db;
mod domain;
mod errors;
mod services;

// use 语句把路径较长的类型引入当前作用域，方便后续直接使用短名称。
use db::Database;
use domain::audit::AuditEventType;
use domain::recovery::RecoveryManager;
use services::audit_service;
// tauri::Manager 是一个 trait，提供 .manage() / .path() 等方法。
// 如果不 use 它，直接调用 app.manage() 会报"方法未找到"的编译错误——
// 这是 Rust trait 方法的特性：只有当 trait 在作用域内时，其方法才可见。
use tauri::Manager;

// #[cfg_attr(mobile, tauri::mobile_entry_point)] 是条件编译属性：
// 当编译目标是移动平台（iOS/Android）时，给 run() 函数加上移动端入口标注。
// 在桌面平台编译时，这个属性完全消失，不产生任何效果。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // tauri::Builder 使用"建造者模式"（Builder Pattern）来配置 Tauri 应用。
    // 每个 .plugin() / .setup() / .invoke_handler() 调用都返回 Builder 自身，
    // 允许链式调用（method chaining），最终 .run() 启动应用。
    tauri::Builder::default()
        // 注册 Tauri 官方插件：文件对话框、文件系统访问、Shell 命令执行。
        // 插件在 tauri.conf.json 中也需要对应配置才能在前端使用。
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        // .setup() 接收一个闭包（closure），在应用窗口显示之前执行一次初始化逻辑。
        // |app| 是闭包参数，app 是 &mut tauri::App 类型。
        // 闭包返回 Result，? 操作符在出错时提前返回错误。
        .setup(|app| {
            // 日志插件
            // cfg!(debug_assertions) 是编译时常量：Debug 模式为 true，Release 为 false。
            // 只在开发时启用详细日志，避免发布版本打印过多信息。
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Debug)
                        .build(),
                )?;
            }

            // 初始化数据库
            // app.path().app_data_dir() 获取系统特定的应用数据目录：
            //   Windows: C:\Users\<用户名>\AppData\Roaming\<应用名>
            //   macOS:   ~/Library/Application Support/<应用名>
            //   Linux:   ~/.local/share/<应用名>
            // .expect("...") 在 Option 为 None 或 Result 为 Err 时直接 panic，
            // 适合用于"如果失败程序根本无法运行"的情况。
            let app_dir = app.path().app_data_dir().expect("无法获取应用数据目录");
            std::fs::create_dir_all(&app_dir).expect("无法创建应用数据目录");
            let db = Database::new(app_dir).expect("数据库初始化失败");

            // 应用启动时，检查上次运行是否有"正在处理中"却没有正常结束的任务。
            // .unwrap_or_else(|e| {...}) 处理错误：出错时执行闭包，返回空 Vec，
            // 保证程序继续运行，而不是因为这个非关键操作而崩溃。
            let interrupted_tasks = db
                .interrupt_processing_tasks()
                .unwrap_or_else(|e| {
                    log::error!("启动残留任务修复失败: {e}");
                    vec![]
                });
            // 遍历所有被修复的任务，记录审计日志。
            // for...in 会取得 interrupted_tasks 的所有权（move）。
            for task in interrupted_tasks {
                // let _ = ... 表示故意忽略返回值。
                // 审计日志记录失败不影响主流程，所以不用 ? 传播错误。
                let _ = audit_service::log_audit_event(
                    &db,
                    AuditEventType::TaskInterrupted,
                    Some(task.id.clone()),
                    format!("启动修复中断任务: {}", task.file_name),
                );
            }

            // app.manage(db) 把数据库实例注册到 Tauri 的"状态管理器"中。
            // 之后在任何 #[command] 函数里，只需声明 State<'_, Database> 参数，
            // Tauri 会自动注入这个实例——这就是依赖注入（Dependency Injection）。
            // manage() 内部使用 Arc（原子引用计数）保证线程安全共享。
            app.manage(db);

            // 初始化恢复任务管理器（用于控制密码恢复的启动/取消）
            app.manage(RecoveryManager::new());

            log::info!("ArchiveFlow 启动成功");
            Ok(())
        })
        // invoke_handler 注册所有可以从前端 JavaScript 调用的 Rust 命令。
        // tauri::generate_handler![] 宏把函数列表转换成 Tauri 能处理的分发器。
        // 前端通过 invoke("命令名", {...参数}) 调用对应的 Rust 函数。
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
            commands::audit_commands::record_setting_change,
            commands::recovery_commands::start_recovery,
            commands::recovery_commands::get_recovery_checkpoint,
            commands::recovery_commands::resume_recovery,
            commands::recovery_commands::cancel_recovery,
            commands::export_commands::export_tasks,
        ])
        // tauri::generate_context!() 读取 tauri.conf.json 并生成配置结构体。
        .run(tauri::generate_context!())
        .expect("启动 ArchiveFlow 时出错");
}
