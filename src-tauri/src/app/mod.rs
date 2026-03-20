// app 模块负责"把各块能力装配成一个可运行的 Tauri 应用"。
// 这里刻意不放业务逻辑，只处理：
//   1. Builder 的创建
//   2. 插件注册
//   3. setup 初始化
//   4. 命令注册

mod commands;
mod setup;

/// 运行整个 Tauri 应用。
///
/// 这是 lib.rs 对外暴露的唯一启动入口。
/// 好处是：lib.rs 本身会保持很薄，后续如果还要拆托盘、窗口、菜单等启动逻辑，
/// 只需要继续扩展 app 模块，而不用把所有内容塞回 crate 根文件。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    commands::register(setup::configure(
        tauri::Builder::default()
            .plugin(tauri_plugin_dialog::init())
            .plugin(tauri_plugin_fs::init())
            .plugin(tauri_plugin_shell::init()),
    ))
    .run(tauri::generate_context!())
    .expect("启动 ArchiveFlow 时出错");
}
