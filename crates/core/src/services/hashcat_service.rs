// hashcat_service 是对外暴露的 GPU 恢复服务门面层。
// 这里把 detection / 参数构建 / ZIP hash 提取 / 子进程运行拆到子模块，
// 这样命令层只需要依赖少量稳定入口。

mod args;
mod detection;
mod gpu_engine;
mod runner;
mod zip_aes_extract;

use std::path::Path;
use std::process::Command;

#[allow(unused_imports)]
pub use detection::{
    detect_hashcat, detect_hashcat_for_ui, HashcatDetectionResult, HashcatDeviceInfo, HashcatInfo,
};
pub use gpu_engine::run_gpu_recovery;

#[allow(unused_imports)]
pub use args::{build_attack_args, HashcatArgs};

#[allow(unused_imports)]
pub use runner::run_hashcat;

#[allow(unused_imports)]
pub use zip_aes_extract::{extract_zip_aes_hash, extract_zip_hash, HashcatZipHash};

/// hashcat 的 OpenCL / modules 目录默认按"当前工作目录"解析。
/// 因此总是把进程工作目录切到 hashcat.exe 所在目录，
/// 否则即使提供了正确的 exe 路径，也可能因为 cwd 不对而报
/// `./OpenCL/: No such file or directory`。
///
/// 这是 detection 和 runner 共享的基础构建逻辑。
fn build_hashcat_command(path: &Path) -> Command {
    let mut command = Command::new(path);
    if let Some(parent) = path.parent() {
        command.current_dir(parent);
    }
    command
}
