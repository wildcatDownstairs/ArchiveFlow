// recovery_service 是对外暴露的恢复服务门面层。
// 命令层和 benchmark 继续从这里导入，
// 但具体实现已经按职责拆到了同名目录下的多个子模块。

mod engine;
mod generators;
mod passwords;
mod workers;

pub use engine::{run_recovery, RecoveryResult};
#[allow(unused_imports)]
pub use generators::{generate_bruteforce_passwords, BruteForceIterator, MaskIterator};

/// 界面和命令入口共用同一套掩码语法，校验不启动恢复任务。
pub fn validate_mask(mask: &str) -> Result<(), String> {
    if mask.trim().is_empty() {
        return Err("掩码不能为空 / Mask cannot be empty".into());
    }
    generators::parse_mask(mask).map(|_| ())
}
#[allow(unused_imports)]
pub use passwords::{try_password_7z, try_password_on_archive, try_password_rar, try_password_zip};

#[cfg(test)]
mod tests;
