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

    /// 原子地为指定任务注册取消标志。
    /// 如果任务已在运行，返回 Err(())
    pub fn try_register(&self, task_id: &str) -> Result<Arc<AtomicBool>, ()> {
        let mut flags = self.cancel_flags.lock().unwrap();
        if flags.contains_key(task_id) {
            return Err(());
        }

        let flag = Arc::new(AtomicBool::new(false));
        flags.insert(task_id.to_string(), Arc::clone(&flag));
        Ok(flag)
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

    /// 检查指定任务是否正在运行
    pub fn is_running(&self, task_id: &str) -> bool {
        let flags = self.cancel_flags.lock().unwrap();
        flags.contains_key(task_id)
    }

    /// 是否存在任意运行中的恢复任务
    pub fn has_running_tasks(&self) -> bool {
        let flags = self.cancel_flags.lock().unwrap();
        !flags.is_empty()
    }

    /// 移除指定任务的取消标志（任务完成后清理）
    pub fn remove(&self, task_id: &str) {
        let mut flags = self.cancel_flags.lock().unwrap();
        flags.remove(task_id);
    }
}

#[cfg(test)]
mod tests {
    use super::RecoveryManager;

    #[test]
    fn try_register_is_atomic_per_task() {
        let manager = RecoveryManager::new();

        let first = manager.try_register("task-1");
        let second = manager.try_register("task-1");

        assert!(first.is_ok());
        assert!(second.is_err());
        assert!(manager.is_running("task-1"));
    }

    #[test]
    fn remove_allows_reregister() {
        let manager = RecoveryManager::new();

        assert!(manager.try_register("task-1").is_ok());
        manager.remove("task-1");

        assert!(manager.try_register("task-1").is_ok());
    }

    #[test]
    fn cancel_sets_flag() {
        let manager = RecoveryManager::new();
        let flag = manager.try_register("task-1").unwrap();
        assert!(!flag.load(std::sync::atomic::Ordering::Relaxed));

        let cancelled = manager.cancel("task-1");
        assert!(cancelled);
        assert!(flag.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn cancel_nonexistent_returns_false() {
        let manager = RecoveryManager::new();
        assert!(!manager.cancel("nonexistent"));
    }

    #[test]
    fn is_running_lifecycle() {
        let manager = RecoveryManager::new();
        assert!(!manager.is_running("task-1"));

        manager.try_register("task-1").unwrap();
        assert!(manager.is_running("task-1"));

        manager.remove("task-1");
        assert!(!manager.is_running("task-1"));
    }

    #[test]
    fn has_running_tasks_lifecycle() {
        let manager = RecoveryManager::new();
        assert!(!manager.has_running_tasks());

        manager.try_register("task-1").unwrap();
        assert!(manager.has_running_tasks());

        manager.remove("task-1");
        assert!(!manager.has_running_tasks());
    }

    #[test]
    fn multiple_tasks_independent() {
        let manager = RecoveryManager::new();
        let _flag1 = manager.try_register("task-1").unwrap();
        let flag2 = manager.try_register("task-2").unwrap();

        // Cancel only task-1
        manager.cancel("task-1");

        // task-2's flag should NOT be affected
        assert!(!flag2.load(std::sync::atomic::Ordering::Relaxed));
        assert!(manager.is_running("task-2"));
    }
}
