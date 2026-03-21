use serde::{Deserialize, Serialize};
use std::collections::HashMap;
// AtomicBool 是原子布尔类型：多线程同时读写不会出现数据竞争（data race）。
// 普通 bool 在多线程下读写是不安全的，AtomicBool 通过硬件原子指令保证安全。
use std::sync::atomic::AtomicBool;
// Arc（Atomic Reference Counting）是线程安全的引用计数智能指针。
//   - Rc<T>：单线程用，不能跨线程共享
//   - Arc<T>：多线程用，内部用原子操作维护引用计数
// Mutex（互斥锁）保护共享数据：同一时刻只有一个线程能持有锁并访问数据。
use std::sync::{Arc, Mutex};

use crate::domain::task::ArchiveType;
use chrono::{DateTime, Utc};

/// 攻击模式：定义密码恢复时使用哪种搜索策略
///
/// #[serde(tag = "type", rename_all = "snake_case")] 是"内部标签"序列化：
/// 序列化时在 JSON 对象里加一个 "type" 字段来区分变体：
///   { "type": "dictionary", "wordlist": [...] }
///   { "type": "brute_force", "charset": "abc", "min_length": 1, "max_length": 6 }
///   { "type": "mask", "mask": "?l?l?d" }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AttackMode {
    /// 字典攻击：逐个尝试给定的密码列表
    Dictionary { wordlist: Vec<String> },
    /// 暴力破解：穷举指定字符集的所有组合
    ///   - charset: 候选字符，例如 "abc123"
    ///   - min_length / max_length: 密码长度范围
    BruteForce {
        charset: String,
        min_length: usize,
        max_length: usize,
    },
    /// 掩码攻击：按位置语法穷举，支持 ?l ?u ?d ?s ?a 和 ??
    Mask { mask: String },
}

/// 恢复任务配置：前端发送给后端的启动参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    /// 目标任务的 ID
    pub task_id: String,
    /// 使用哪种攻击模式
    pub mode: AttackMode,
    /// 调度优先级：写进 checkpoint 后，重启续跑时还能保留原来的队列意图。
    pub priority: i32,
}

/// 恢复断点：用于应用重启后继续上一次恢复
///
/// 当程序意外退出或用户暂停时，将当前进度持久化到数据库，
/// 下次启动后可以从断点继续，而不必从头开始。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryCheckpoint {
    /// 属于哪个任务
    pub task_id: String,
    /// 继续恢复时要复用的攻击模式
    pub mode: AttackMode,
    /// 这次恢复原本的调度优先级。
    /// 如果应用重启后再继续恢复，就用这个值恢复原先的排队权重。
    pub priority: i32,
    /// 归档类型，用来校验断点是否还匹配当前任务
    pub archive_type: ArchiveType,
    /// 已经尝试过多少个候选
    pub tried: u64,
    /// 总候选空间大小
    pub total: u64,
    /// 最近一次保存断点的时间
    pub updated_at: DateTime<Utc>,
}

/// 恢复状态：描述恢复任务当前处于哪个阶段
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryStatus {
    /// 正在运行（密码还未找到，也未耗尽）
    Running,
    /// 已找到密码
    Found,
    /// 已穷尽所有候选密码（没找到）
    Exhausted,
    /// 已被用户取消
    Cancelled,
    /// 发生错误（如文件损坏、权限问题）
    Error,
}

/// 恢复进度事件：后端通过 Tauri 事件系统实时推送给前端的进度数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryProgress {
    /// 关联的任务 ID（前端用来匹配更新哪个任务的进度）
    pub task_id: String,
    /// 已尝试的密码数量
    pub tried: u64,
    /// 预估总密码数量（字典模式为精确值，暴力破解为估算值）
    pub total: u64,
    /// 每秒尝试密码数（性能指标）
    pub speed: f64,
    /// 当前恢复状态
    pub status: RecoveryStatus,
    /// 找到的密码（仅在 status == Found 时有值，其他时候为 None）
    pub found_password: Option<String>,
    /// 已用时间（秒，含小数）
    pub elapsed_seconds: f64,
    /// 当前这次恢复实际启用了多少个 worker。
    /// 对 Rust 新手来说，可以把它理解成“并行尝试密码的工作线程数量”。
    pub worker_count: u64,
    /// 最近一次把 checkpoint 持久化到数据库的时间。
    /// 前端用它告诉用户“断点大概保存到了什么时候”。
    pub last_checkpoint_at: Option<DateTime<Utc>>,
}

/// 调度状态：描述一个恢复任务在调度器中的排队位置。
///
/// 注意这里和 TaskStatus 是两条轴：
///   - TaskStatus 关注“任务结果”与“恢复主流程”
///   - ScheduledRecoveryState 关注“调度器有没有把它排队/暂停/运行”
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledRecoveryState {
    /// 已进入队列，等待空闲并发槽位
    Queued,
    /// 调度器已经发车，任务正在运行
    Running,
    /// 被用户暂停，保留队列元数据，稍后可继续
    Paused,
}

/// 单个调度项：保存恢复任务在调度器中的元信息。
// 加上 PartialEq 以便在测试中可以用 assert_eq! 比较两个 ScheduledRecovery。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduledRecovery {
    /// 对应哪个任务
    pub task_id: String,
    /// 这次调度要使用的攻击模式
    pub mode: AttackMode,
    /// 优先级，数字越大越先执行
    pub priority: i32,
    /// 当前调度状态
    pub state: ScheduledRecoveryState,
    /// 进入调度器的时间
    pub requested_at: DateTime<Utc>,
    /// 真正开始运行的时间（排队中/暂停中时为 None）
    pub started_at: Option<DateTime<Utc>>,
}

/// 调度器快照：前端一次性读取整个队列时使用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoverySchedulerSnapshot {
    /// 当前允许同时跑多少个恢复任务
    pub max_concurrent: usize,
    /// 当前运行中的任务数
    pub running_count: usize,
    /// 当前排队中的任务数
    pub queued_count: usize,
    /// 当前暂停中的任务数
    pub paused_count: usize,
    /// 所有调度项（已按优先级和时间排序）
    pub tasks: Vec<ScheduledRecovery>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SchedulerError {
    AlreadyScheduled,
}

#[derive(Debug)]
struct RecoverySchedulerInner {
    max_concurrent: usize,
    tasks: HashMap<String, ScheduledRecovery>,
}

/// 恢复任务调度器：管理队列、并发上限、暂停和优先级。
///
/// 这里仍然用 Mutex 包住整个内部状态，原因是：
///   1. 调度操作本身是“低频控制路径”，不是热循环；
///   2. 实现要尽量容易审查，避免为了微小性能收益引入复杂锁模型；
///   3. 队列决策需要一次性看到整体状态（运行数、优先级、队列顺序）。
pub struct RecoveryScheduler {
    inner: Mutex<RecoverySchedulerInner>,
}

impl RecoveryScheduler {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(RecoverySchedulerInner {
                // 默认并发 1：每个恢复任务内部已经会并行吃满 CPU，
                // 所以多任务同时跑默认是保守策略，避免一上来就过度抢核。
                max_concurrent: 1,
                tasks: HashMap::new(),
            }),
        }
    }

    pub fn snapshot(&self) -> RecoverySchedulerSnapshot {
        let inner = self.inner.lock().unwrap();
        Self::snapshot_locked(&inner)
    }

    pub fn get_task(&self, task_id: &str) -> Option<ScheduledRecovery> {
        let inner = self.inner.lock().unwrap();
        inner.tasks.get(task_id).cloned()
    }

    pub fn set_max_concurrent(&self, max_concurrent: usize) -> RecoverySchedulerSnapshot {
        let mut inner = self.inner.lock().unwrap();
        inner.max_concurrent = max_concurrent.max(1);
        Self::snapshot_locked(&inner)
    }

    pub fn enqueue(
        &self,
        task_id: &str,
        mode: AttackMode,
        priority: i32,
    ) -> Result<ScheduledRecovery, SchedulerError> {
        let mut inner = self.inner.lock().unwrap();
        if inner.tasks.contains_key(task_id) {
            return Err(SchedulerError::AlreadyScheduled);
        }

        let scheduled = ScheduledRecovery {
            task_id: task_id.to_string(),
            mode,
            priority,
            state: ScheduledRecoveryState::Queued,
            requested_at: Utc::now(),
            started_at: None,
        };
        inner.tasks.insert(task_id.to_string(), scheduled.clone());
        Ok(scheduled)
    }

    pub fn pause(&self, task_id: &str) -> Option<ScheduledRecovery> {
        let mut inner = self.inner.lock().unwrap();
        let scheduled = inner.tasks.get_mut(task_id)?;
        scheduled.state = ScheduledRecoveryState::Paused;
        scheduled.started_at = None;
        Some(scheduled.clone())
    }

    pub fn resume(&self, task_id: &str) -> Option<ScheduledRecovery> {
        let mut inner = self.inner.lock().unwrap();
        let scheduled = inner.tasks.get_mut(task_id)?;
        scheduled.state = ScheduledRecoveryState::Queued;
        scheduled.started_at = None;
        Some(scheduled.clone())
    }

    pub fn mark_queued(&self, task_id: &str) -> Option<ScheduledRecovery> {
        let mut inner = self.inner.lock().unwrap();
        let scheduled = inner.tasks.get_mut(task_id)?;
        scheduled.state = ScheduledRecoveryState::Queued;
        scheduled.started_at = None;
        Some(scheduled.clone())
    }

    pub fn finish(&self, task_id: &str) -> Option<ScheduledRecovery> {
        let mut inner = self.inner.lock().unwrap();
        inner.tasks.remove(task_id)
    }

    pub fn take_dispatchable_tasks(&self) -> Vec<ScheduledRecovery> {
        let mut inner = self.inner.lock().unwrap();
        let running_count = inner
            .tasks
            .values()
            .filter(|task| task.state == ScheduledRecoveryState::Running)
            .count();
        let available_slots = inner.max_concurrent.saturating_sub(running_count);
        if available_slots == 0 {
            return Vec::new();
        }

        let mut queued_tasks: Vec<ScheduledRecovery> = inner
            .tasks
            .values()
            .filter(|task| task.state == ScheduledRecoveryState::Queued)
            .cloned()
            .collect();
        queued_tasks.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.requested_at.cmp(&right.requested_at))
        });
        queued_tasks.truncate(available_slots);

        let started_at = Utc::now();
        for task in &queued_tasks {
            if let Some(entry) = inner.tasks.get_mut(&task.task_id) {
                entry.state = ScheduledRecoveryState::Running;
                entry.started_at = Some(started_at);
            }
        }

        queued_tasks
            .into_iter()
            .filter_map(|task| inner.tasks.get(&task.task_id).cloned())
            .collect()
    }

    fn snapshot_locked(inner: &RecoverySchedulerInner) -> RecoverySchedulerSnapshot {
        let mut tasks: Vec<ScheduledRecovery> = inner.tasks.values().cloned().collect();
        tasks.sort_by(|left, right| {
            Self::state_rank(&left.state)
                .cmp(&Self::state_rank(&right.state))
                .then_with(|| right.priority.cmp(&left.priority))
                .then_with(|| left.requested_at.cmp(&right.requested_at))
        });

        RecoverySchedulerSnapshot {
            max_concurrent: inner.max_concurrent,
            running_count: tasks
                .iter()
                .filter(|task| task.state == ScheduledRecoveryState::Running)
                .count(),
            queued_count: tasks
                .iter()
                .filter(|task| task.state == ScheduledRecoveryState::Queued)
                .count(),
            paused_count: tasks
                .iter()
                .filter(|task| task.state == ScheduledRecoveryState::Paused)
                .count(),
            tasks,
        }
    }

    fn state_rank(state: &ScheduledRecoveryState) -> u8 {
        match state {
            ScheduledRecoveryState::Running => 0,
            ScheduledRecoveryState::Queued => 1,
            ScheduledRecoveryState::Paused => 2,
        }
    }
}

/// 恢复任务管理器：全局单例，追踪所有正在运行的恢复任务
///
/// 设计思路：
///   - 每个运行中的任务都有一个 Arc<AtomicBool> 取消标志
///   - 工作线程定期检查这个标志，为 true 时主动退出
///   - 主线程（命令处理）通过 cancel() 把标志设为 true，通知工作线程退出
///
/// 为什么用 Mutex<HashMap>？
///   - HashMap 本身不是线程安全的（不能同时读写）
///   - Mutex 保证同一时刻只有一个线程能修改 HashMap
///   - 配合 Tauri 的 State<'_, RecoveryManager>，多个命令可以安全共享这个管理器
pub struct RecoveryManager {
    /// key: task_id（String），value: 取消标志（Arc<AtomicBool>）
    pub cancel_flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl RecoveryManager {
    /// 创建一个新的空管理器
    pub fn new() -> Self {
        Self {
            cancel_flags: Mutex::new(HashMap::new()),
        }
    }

    /// 原子地为指定任务注册取消标志（防止同一任务被重复启动）。
    ///
    /// 返回值：
    ///   - Ok(Arc<AtomicBool>)：注册成功，返回取消标志的 Arc 引用
    ///   - Err(())：任务已在运行，拒绝重复注册
    ///
    /// Arc::clone(&flag) 不是复制数据，而是复制"指向同一数据的指针"，
    /// 并把引用计数 +1。工作线程持有一份，管理器保留一份，
    /// 共同指向同一个 AtomicBool。
    pub fn try_register(&self, task_id: &str) -> Result<Arc<AtomicBool>, ()> {
        // .lock() 获取互斥锁，返回 MutexGuard（锁守卫）。
        // .unwrap() 在 Mutex 被"中毒"（持锁线程 panic）时 panic，
        // 这在实践中极少发生，此处可以接受。
        // MutexGuard 在离开作用域时自动释放锁（RAII 模式）。
        let mut flags = self.cancel_flags.lock().unwrap();
        if flags.contains_key(task_id) {
            return Err(());
        }

        // AtomicBool::new(false)：初始值为 false（未取消）
        let flag = Arc::new(AtomicBool::new(false));
        // task_id.to_string() 把 &str（借用）转为 String（拥有所有权），
        // 因为 HashMap 的 key 需要拥有所有权。
        flags.insert(task_id.to_string(), Arc::clone(&flag));
        Ok(flag)
    }

    /// 设置指定任务的取消标志为 true，通知工作线程停止。
    ///
    /// Ordering::Relaxed：最宽松的原子排序，性能最好。
    /// 对于"设置取消标志"这种场景，不需要严格的内存顺序保证，Relaxed 足够。
    ///
    /// 返回 true 表示找到了该任务并设置了标志，false 表示任务不存在。
    pub fn cancel(&self, task_id: &str) -> bool {
        let flags = self.cancel_flags.lock().unwrap();
        if let Some(flag) = flags.get(task_id) {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// 检查指定任务是否正在运行（即取消标志是否已注册）
    pub fn is_running(&self, task_id: &str) -> bool {
        let flags = self.cancel_flags.lock().unwrap();
        flags.contains_key(task_id)
    }

    /// 是否存在任意运行中的恢复任务
    pub fn has_running_tasks(&self) -> bool {
        let flags = self.cancel_flags.lock().unwrap();
        !flags.is_empty()
    }

    /// 移除指定任务的取消标志（任务完成/取消/失败后调用，释放资源）
    ///
    /// 移除后 Arc 引用计数 -1，工作线程持有的另一份 Arc 会在线程结束时归零，
    /// AtomicBool 的内存随之释放。
    pub fn remove(&self, task_id: &str) {
        let mut flags = self.cancel_flags.lock().unwrap();
        flags.remove(task_id);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AttackMode, RecoveryManager, RecoveryScheduler, ScheduledRecoveryState, SchedulerError,
    };

    #[test]
    fn try_register_is_atomic_per_task() {
        let manager = RecoveryManager::new();

        let first = manager.try_register("task-1");
        // 同一个 task_id 第二次注册应该失败
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

        // 移除后可以重新注册（例如重新启动任务）
        assert!(manager.try_register("task-1").is_ok());
    }

    #[test]
    fn cancel_sets_flag() {
        let manager = RecoveryManager::new();
        let flag = manager.try_register("task-1").unwrap();
        // 初始值为 false
        assert!(!flag.load(std::sync::atomic::Ordering::Relaxed));

        let cancelled = manager.cancel("task-1");
        assert!(cancelled);
        // cancel 之后变为 true
        assert!(flag.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn cancel_nonexistent_returns_false() {
        let manager = RecoveryManager::new();
        // 取消不存在的任务应该返回 false，而不是 panic
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

        // 取消 task-1 不应影响 task-2 的标志
        manager.cancel("task-1");

        assert!(!flag2.load(std::sync::atomic::Ordering::Relaxed));
        assert!(manager.is_running("task-2"));
    }

    #[test]
    fn scheduler_dispatch_respects_priority() {
        let scheduler = RecoveryScheduler::new();
        let mode = AttackMode::Mask {
            mask: "?d?d".to_string(),
        };
        scheduler.enqueue("task-low", mode.clone(), 1).unwrap();
        scheduler.enqueue("task-high", mode, 10).unwrap();

        let dispatched = scheduler.take_dispatchable_tasks();
        assert_eq!(dispatched.len(), 1);
        assert_eq!(dispatched[0].task_id, "task-high");
        assert_eq!(dispatched[0].state, ScheduledRecoveryState::Running);
    }

    #[test]
    fn scheduler_pause_and_resume_roundtrip() {
        let scheduler = RecoveryScheduler::new();
        scheduler
            .enqueue(
                "task-1",
                AttackMode::Dictionary {
                    wordlist: vec!["secret".to_string()],
                },
                0,
            )
            .unwrap();

        let paused = scheduler.pause("task-1").unwrap();
        assert_eq!(paused.state, ScheduledRecoveryState::Paused);

        let resumed = scheduler.resume("task-1").unwrap();
        assert_eq!(resumed.state, ScheduledRecoveryState::Queued);
    }

    #[test]
    fn scheduler_rejects_duplicate_entries() {
        let scheduler = RecoveryScheduler::new();
        let mode = AttackMode::Mask {
            mask: "?d".to_string(),
        };

        assert!(scheduler.enqueue("task-1", mode.clone(), 0).is_ok());
        assert_eq!(
            scheduler.enqueue("task-1", mode, 1),
            Err(SchedulerError::AlreadyScheduled)
        );
    }

    #[test]
    fn scheduler_max_concurrent_is_clamped_to_one() {
        let scheduler = RecoveryScheduler::new();
        let snapshot = scheduler.set_max_concurrent(0);
        assert_eq!(snapshot.max_concurrent, 1);
    }
}
