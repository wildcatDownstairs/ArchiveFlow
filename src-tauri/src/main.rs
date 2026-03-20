// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// 这个属性宏的作用：在 Release 模式下，告诉 Windows 这是一个 GUI 程序，
// 不要额外弹出黑色的命令行窗口。Debug 模式下不生效，方便开发时查看日志。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// main.rs 是程序的入口文件。
// 注意这里只有一行代码：调用 app_lib 库里的 run() 函数。
// 这是 Tauri 的惯用模式：把所有逻辑放在 lib.rs（库 crate），
// main.rs 只负责启动。这样做的好处是方便测试——库 crate 可以被测试框架引用，
// 而 main.rs 通常不会被直接测试。
fn main() {
    app_lib::run();
}
