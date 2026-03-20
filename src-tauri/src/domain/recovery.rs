use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

/// 攻击模式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AttackMode {
    /// 字典攻击：逐个尝试给定的密码列表
    Dictionary { wordlist: Vec<String> },
    /// 暴力破解：穷举指定字符集的所有组合
    BruteForce {
        charset: String,
        min_length: usize,
        max_length: usize,
    },
}

/// 恢复任务配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    pub task_id: String,
    pub mode: AttackMode,
}

/// 恢复状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryStatus {
    /// 正在运行
    Running,
    /// 已找到密码
    Found,
    /// 已穷尽所有候选密码
    Exhausted,
    /// 已被用户取消
    Cancelled,
    /// 发生错误
    Error,
}

/// 恢复进度事件（发送给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryProgress {
    pub task_id: String,
    /// 已尝试的密码数量
    pub tried: u64,
    /// 预估总密码数量（字典模式为精确值，暴力破解为估算值）
    pub total: u64,
    /// 每秒尝试密码数
    pub speed: f64,
    /// 当前恢复状态
    pub status: RecoveryStatus,
    /// 找到的密码（仅在 status == Found 时有值）
    pub found_password: Option<String>,
    /// 已用时间（秒）
    pub elapsed_seconds: f64,
}

/// 恢复任务管理器：管理运行中任务的取消标志
pub struct RecoveryManager {
    pub cancel_flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl RecoveryManager {
    pub fn new() -> Self {
        Self {
            cancel_flags: Mutex::new(HashMap::new()),
        }
    }

    /// 为指定任务注册一个取消标志，返回 Arc<AtomicBool> 供恢复线程使用
    pub fn register(&self, task_id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        let mut flags = self.cancel_flags.lock().unwrap();
        flags.insert(task_id.to_string(), Arc::clone(&flag));
        flag
    }

    /// 设置指定任务的取消标志
    pub fn cancel(&self, task_id: &str) -> bool {
        let flags = self.cancel_flags.lock().unwrap();
        if let Some(flag) = flags.get(task_id) {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// 移除指定任务的取消标志（任务完成后清理）
    pub fn remove(&self, task_id: &str) {
        let mut flags = self.cancel_flags.lock().unwrap();
        flags.remove(task_id);
    }
}
