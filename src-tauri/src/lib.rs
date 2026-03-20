// lib.rs 是这个"库 crate"的根模块。
// Rust 项目通常把逻辑放在 lib.rs，main.rs 只调用 lib.rs 里的入口函数。
// 这样拆分的好处：测试代码可以直接引用库 crate，而不依赖 main.rs。

// mod 关键字声明子模块。Rust 会去找 commands/mod.rs、db/mod.rs 等文件。
// 每个子模块默认是私有的（模块外无法访问），除非用 pub mod 或 pub use 暴露。
mod app;
#[cfg(test)]
mod benchmarks;
mod commands;
mod db;
mod domain;
mod errors;
mod services;

// crate 根入口尽量保持很薄：真正的应用装配逻辑放到 app 模块中。
pub fn run() {
    app::run();
}
