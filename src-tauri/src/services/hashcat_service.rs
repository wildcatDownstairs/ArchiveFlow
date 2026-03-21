// hashcat_service 是对外暴露的 GPU 恢复服务门面层。
// 这里把 detection / 参数构建 / ZIP hash 提取 / 子进程运行拆到子模块，
// 这样命令层只需要依赖少量稳定入口。

mod args;
mod detection;
mod gpu_engine;
mod runner;
mod zip_aes_extract;

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
