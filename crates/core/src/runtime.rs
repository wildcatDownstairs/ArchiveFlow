//! 原生宿主持有共享状态。后台线程只发布类型化进度，不接触窗口。
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use crate::{
    db::Database,
    domain::{
        audit::AuditEventType,
        recovery::{RecoveryManager, RecoveryProgress, RecoveryScheduler},
    },
    errors::AppError,
    services::{app_log_service, audit_service},
};

#[derive(Clone)]
pub struct AppRuntime(Arc<RuntimeState>);

struct RuntimeState {
    db: Database,
    manager: RecoveryManager,
    scheduler: RecoveryScheduler,
    data_dir: PathBuf,
    progress: Mutex<HashMap<String, RecoveryProgress>>,
    shutting_down: AtomicBool,
    // 句柄存活期间保留锁，避免两个原生实例修复彼此的运行中任务。
    _instance_lock: File,
}

impl AppRuntime {
    /// 使用旧 Tauri identifier 对应的目录，直接沿用既有数据库和断点。
    pub fn default_data_dir() -> Result<PathBuf, AppError> {
        dirs::data_dir()
            .map(|path| path.join("com.archiveflow.app"))
            .ok_or_else(|| AppError::FileError("无法获取应用数据目录".into()))
    }

    pub fn new(data_dir: PathBuf) -> Result<Self, AppError> {
        std::fs::create_dir_all(&data_dir).map_err(|e| AppError::FileError(e.to_string()))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(data_dir.join("native.lock"))
            .map_err(|e| AppError::FileError(e.to_string()))?;
        fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| {
            AppError::InvalidArgument("ArchiveFlow 已在运行，请切换到已打开的窗口".into())
        })?;
        let db = Database::new(data_dir.clone()).map_err(|e| AppError::FileError(e.to_string()))?;
        for task in db.interrupt_processing_tasks()? {
            audit_service::log_audit_event(
                &db,
                AuditEventType::TaskInterrupted,
                Some(task.id),
                format!("启动修复中断任务: {}", task.file_name),
            )?;
        }
        let runtime = Self(Arc::new(RuntimeState {
            db,
            manager: RecoveryManager::new(),
            scheduler: RecoveryScheduler::new(),
            data_dir,
            progress: Mutex::new(HashMap::new()),
            shutting_down: AtomicBool::new(false),
            _instance_lock: lock,
        }));
        app_log_service::append_app_log(
            &runtime,
            "INFO",
            Some("process"),
            "ArchiveFlow GPUI 启动完成",
        )?;
        Ok(runtime)
    }

    pub fn database(&self) -> &Database {
        &self.0.db
    }
    pub fn recovery_manager(&self) -> &RecoveryManager {
        &self.0.manager
    }
    pub fn scheduler(&self) -> &RecoveryScheduler {
        &self.0.scheduler
    }
    pub fn data_dir(&self) -> &Path {
        &self.0.data_dir
    }
    pub fn cache_dir(&self) -> PathBuf {
        self.0.data_dir.join("cache")
    }
    pub fn is_shutting_down(&self) -> bool {
        self.0.shutting_down.load(Ordering::Acquire)
    }

    pub fn publish_progress(&self, progress: RecoveryProgress) -> Result<(), AppError> {
        self.0
            .progress
            .lock()
            .map_err(|_| AppError::FileError("进度锁已损坏".into()))?
            .insert(progress.task_id.clone(), progress);
        Ok(())
    }

    pub fn progress(&self) -> HashMap<String, RecoveryProgress> {
        self.0
            .progress
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default()
    }

    /// 先停止调度，再通知 worker 在安全点退出。已有 CPU 断点留在数据库中。
    pub fn request_shutdown(&self) {
        self.0.shutting_down.store(true, Ordering::Release);
        for task in self.scheduler().snapshot().tasks {
            self.scheduler().pause(&task.task_id);
            self.recovery_manager().cancel(&task.task_id);
        }
    }
}
