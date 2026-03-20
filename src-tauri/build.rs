// build.rs 是 Cargo 的构建脚本，在编译主程序之前自动运行。
// Tauri 需要它来生成一些平台相关的构建元数据（如 Windows 资源文件）。
// 一般不需要修改这个文件。
fn main() {
    tauri_build::build()
}
