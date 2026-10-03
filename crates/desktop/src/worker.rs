//! 所有磁盘、系统对话框、数据库及检测操作集中在工作线程，结果通过消息回到 GPUI。
use crate::model::Settings;
use archiveflow_core::{
    commands::{
        archive_commands, audit_commands,
        export_commands::{self, ExportOptions},
        recovery_commands, task_commands,
    },
    domain::{
        audit::AuditEvent,
        recovery::{RecoveryCheckpoint, RecoveryProgress, RecoverySchedulerSnapshot},
        task::Task,
    },
    runtime::AppRuntime,
    services::{app_log_service, hashcat_service::HashcatDetectionResult},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::Duration,
};

pub struct Snapshot {
    pub tasks: Vec<Task>,
    pub audit: Vec<AuditEvent>,
    pub audit_count: u64,
    pub scheduler: RecoverySchedulerSnapshot,
    pub checkpoints: HashMap<String, RecoveryCheckpoint>,
    pub progress: HashMap<String, RecoveryProgress>,
}

pub enum RecoveryInput {
    Dictionary {
        text: String,
        filename: String,
        rules: [bool; 10],
    },
    Config {
        mode: String,
        json: String,
    },
}

impl RecoveryInput {
    fn prepare(self) -> anyhow::Result<(String, String)> {
        match self {
            Self::Dictionary {
                text,
                filename,
                rules,
            } => {
                let candidates = crate::model::dictionary_candidates(
                    &text,
                    &filename,
                    &rules,
                    crate::model::current_year(),
                );
                anyhow::ensure!(
                    !candidates.is_empty(),
                    "请输入候选密码 / Enter candidate passwords"
                );
                Ok((
                    "dictionary".into(),
                    serde_json::json!({"wordlist": candidates}).to_string(),
                ))
            }
            Self::Config { mode, json } => Ok((mode, json)),
        }
    }
}

pub enum Request {
    Import(Option<Vec<PathBuf>>),
    Dictionary,
    Start {
        id: String,
        input: RecoveryInput,
        priority: i32,
        gpu: bool,
        hashcat: String,
    },
    Pause(String),
    Resume(String),
    Cancel(String),
    Delete(String),
    Export {
        ids: Vec<String>,
        format: String,
        options: ExportOptions,
    },
    Save(Settings),
    SaveAppearance(bool),
    SaveDraft(String),
    PrepareClose(String),
    Detect(String),
    ClearTasks,
    ClearAudit,
    OpenLogs,
    OpenData,
    OpenArchiveFolder(PathBuf),
    Shutdown,
}

pub enum Event {
    Snapshot(Box<Snapshot>),
    Message(String),
    Error(String),
    SnapshotError(String),
    CloseReady,
    CloseFailed(String),
    Dictionary(String),
    Detection(HashcatDetectionResult),
    Saved(Settings),
    Started,
}

fn snapshot(runtime: &AppRuntime) -> anyhow::Result<Snapshot> {
    let db = runtime.database();
    let tasks = db.get_all_tasks()?;
    let mut checkpoints = HashMap::new();
    for task in &tasks {
        if let Some(checkpoint) = db.get_recovery_checkpoint(&task.id)? {
            checkpoints.insert(task.id.clone(), checkpoint);
        }
    }
    // 进度快照只保留仍存在的任务，已删除结果不会重新出现在界面。
    let mut progress = runtime.progress();
    progress.retain(|id, _| tasks.iter().any(|task| task.id == *id));
    Ok(Snapshot {
        tasks,
        audit: db.get_audit_events(1000)?,
        audit_count: db.get_audit_event_count()?,
        scheduler: runtime.scheduler().snapshot(),
        checkpoints,
        progress,
    })
}

pub fn start(
    runtime: AppRuntime,
    settings: Settings,
) -> (Sender<Request>, Receiver<Event>, Arc<AtomicBool>) {
    start_with_handler(runtime, settings, handle)
}

fn start_with_handler(
    runtime: AppRuntime,
    settings: Settings,
    handler: impl Fn(Request, &AppRuntime, &Sender<Event>) -> anyhow::Result<()> + Send + 'static,
) -> (Sender<Request>, Receiver<Event>, Arc<AtomicBool>) {
    let (requests, receiver) = mpsc::channel();
    let (sender, events) = mpsc::channel();
    let stopped = Arc::new(AtomicBool::new(false));
    let stopped_thread = stopped.clone();
    runtime.scheduler().set_max_concurrent(settings.concurrency);
    // 快照独立于串行写入队列：文件对话框、归档解析或检测耗时期间，恢复进度仍可刷新。
    let snapshot_runtime = runtime.clone();
    let snapshot_sender = sender.clone();
    let stop_snapshots = Arc::new(AtomicBool::new(false));
    let snapshot_stopped = stop_snapshots.clone();
    let snapshot_thread = std::thread::spawn(move || {
        while !snapshot_stopped.load(Ordering::Acquire) {
            let event = match snapshot(&snapshot_runtime) {
                Ok(value) => Event::Snapshot(Box::new(value)),
                Err(error) => Event::SnapshotError(error.to_string()),
            };
            if snapshot_sender.send(event).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(600));
        }
    });
    std::thread::spawn(move || {
        loop {
            match receiver.recv() {
                Ok(Request::Shutdown) | Err(_) => break,
                Ok(Request::PrepareClose(draft)) => {
                    match handle(Request::SaveDraft(draft), &runtime, &sender) {
                        Ok(()) => {
                            let _ = sender.send(Event::CloseReady);
                            break;
                        }
                        Err(error) => {
                            let _ = sender.send(Event::CloseFailed(error.to_string()));
                        }
                    }
                }
                Ok(request) => {
                    if let Err(error) = handler(request, &runtime, &sender) {
                        let _ = app_log_service::append_app_log(
                            &runtime,
                            "ERROR",
                            Some("ui"),
                            &error.to_string(),
                        );
                        let _ = sender.send(Event::Error(error.to_string()));
                    }
                }
            }
        }
        runtime.request_shutdown();
        stop_snapshots.store(true, Ordering::Release);
        let _ = snapshot_thread.join();
        stopped_thread.store(true, Ordering::Release);
    });
    (requests, events, stopped)
}

fn handle(request: Request, app: &AppRuntime, tx: &Sender<Event>) -> anyhow::Result<()> {
    let db = app.database();
    match request {
        Request::Import(paths) => {
            let paths = paths.or_else(|| {
                rfd::FileDialog::new()
                    .add_filter("Archives", &["zip", "7z", "rar"])
                    .pick_files()
            });
            if let Some(paths) = paths {
                let mut imported = 0;
                for path in paths {
                    match archive_commands::import_archive(
                        path.to_string_lossy().into_owned(),
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        0,
                        db,
                    ) {
                        Ok(_) => imported += 1,
                        Err(e) => {
                            tx.send(Event::Error(format!("{}: {e}", path.display())))?;
                        }
                    }
                }
                tx.send(Event::Message(format!("已导入 / Imported: {imported}")))?;
            } else {
                tx.send(Event::Message(String::new()))?;
            }
        }
        Request::Dictionary => {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("UTF-8 dictionary", &["txt", "dic", "lst"])
                .pick_file()
            {
                anyhow::ensure!(
                    std::fs::metadata(&path)?.len() <= 16 * 1024 * 1024,
                    "字典文件不能超过 16 MiB / Dictionary exceeds 16 MiB"
                );
                let input = std::fs::read_to_string(path)?;
                tx.send(Event::Dictionary(
                    input.trim_start_matches('\u{feff}').to_owned(),
                ))?;
            } else {
                tx.send(Event::Message(String::new()))?;
            }
        }
        Request::Start {
            id,
            input,
            priority,
            gpu,
            hashcat,
        } => {
            anyhow::ensure!(
                !app.is_shutting_down(),
                "应用正在退出 / Application is closing"
            );
            let (mode, config) = input.prepare()?;
            anyhow::ensure!(
                !app.is_shutting_down(),
                "应用正在退出 / Application is closing"
            );
            recovery_commands::start_recovery(
                id,
                mode,
                config,
                Some(priority),
                Some(if gpu { "gpu" } else { "cpu" }.into()),
                Some(hashcat),
                db,
                app.scheduler(),
                app.clone(),
            )?;
            tx.send(Event::Started)?;
        }
        Request::Pause(id) => {
            recovery_commands::pause_recovery(id, app.scheduler(), app.recovery_manager(), db)?;
            tx.send(Event::Message("正在暂停 / Pausing".into()))?;
        }
        Request::Resume(id) => {
            recovery_commands::resume_recovery(id, db, app.scheduler(), app.clone())?;
            tx.send(Event::Message("正在继续 / Resuming".into()))?;
        }
        Request::Cancel(id) => {
            recovery_commands::cancel_recovery(id, db, app.scheduler(), app.recovery_manager())?;
            tx.send(Event::Message("正在取消 / Cancelling".into()))?;
        }
        Request::Delete(id) => {
            anyhow::ensure!(
                app.scheduler().get_task(&id).is_none(),
                "请先取消队列中的任务 / Cancel the scheduled task first"
            );
            task_commands::delete_task(id, db, app.recovery_manager())?;
            tx.send(Event::Message("任务已删除 / Task deleted".into()))?;
        }
        Request::Export {
            ids,
            format,
            options,
        } => {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name(format!(
                    "ArchiveFlow-{}.{}",
                    chrono::Local::now().format("%Y%m%d-%H%M%S"),
                    format
                ))
                .add_filter(format.to_uppercase(), &[&format])
                .save_file()
            {
                let content = export_commands::export_tasks(db, ids, format, Some(options))?;
                std::fs::write(&path, content)?;
                tx.send(Event::Message(format!(
                    "已导出 / Exported: {}",
                    path.display()
                )))?;
            } else {
                tx.send(Event::Message(String::new()))?;
            }
        }
        Request::Save(settings) => {
            settings.save(app.data_dir())?;
            recovery_commands::set_recovery_scheduler_limit(
                settings.concurrency,
                app.scheduler(),
                db,
                app.clone(),
            )?;
            tx.send(Event::Saved(settings))?;
        }
        Request::SaveDraft(draft) => {
            let mut settings = Settings::load(app.data_dir())?;
            settings.dictionary_draft = if settings.clear_dictionary {
                String::new()
            } else {
                draft
            };
            settings.save(app.data_dir())?;
        }
        Request::SaveAppearance(dark) => {
            let mut settings = Settings::load(app.data_dir())?;
            settings.dark = dark;
            settings.save(app.data_dir())?;
            tx.send(Event::Message(String::new()))?;
        }
        Request::Detect(path) => {
            tx.send(Event::Detection(recovery_commands::detect_hashcat(Some(
                path,
            ))?))?;
        }
        Request::ClearTasks => {
            anyhow::ensure!(
                app.scheduler().snapshot().tasks.is_empty(),
                "请先取消所有排队或暂停的任务 / Cancel all scheduled tasks first"
            );
            task_commands::clear_all_tasks(db, app.recovery_manager())?;
            tx.send(Event::Message("已清空任务 / Tasks cleared".into()))?;
        }
        Request::ClearAudit => {
            audit_commands::clear_audit_events(db)?;
            tx.send(Event::Message("已清空操作记录 / Audit cleared".into()))?;
        }
        Request::OpenLogs => {
            app_log_service::open_log_dir(app)?;
        }
        Request::OpenData => {
            open::that(app.data_dir())?;
        }
        Request::OpenArchiveFolder(path) => {
            let parent = path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("无法定位归档目录 / Archive folder unavailable"))?;
            anyhow::ensure!(
                parent.is_dir(),
                "归档目录不存在 / Archive folder no longer exists"
            );
            open::that(parent)?;
            tx.send(Event::Message(String::new()))?;
        }
        Request::Shutdown | Request::PrepareClose(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn await_close(events: &Receiver<Event>, stopped: &AtomicBool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut acknowledged = false;
        while !acknowledged || !stopped.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline, "close was not acknowledged");
            match events.recv_timeout(Duration::from_millis(50)) {
                Ok(Event::CloseReady) => acknowledged = true,
                Ok(Event::Error(error) | Event::CloseFailed(error)) => panic!("{error}"),
                _ => {}
            }
        }
    }

    #[test]
    fn save_then_close_preserves_preferences_and_latest_draft() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = AppRuntime::new(directory.path().to_path_buf()).unwrap();
        let settings = Settings {
            english: true,
            mask_exports: true,
            concurrency: 3,
            ..Settings::default()
        };
        let (requests, events, stopped) = start(runtime.clone(), Settings::default());
        requests.send(Request::Save(settings)).unwrap();
        runtime.request_shutdown();
        requests
            .send(Request::PrepareClose("final dictionary edit".into()))
            .unwrap();
        await_close(&events, &stopped);
        let saved = Settings::load(directory.path()).unwrap();
        assert!(saved.english && saved.mask_exports);
        assert_eq!(saved.concurrency, 3);
        assert_eq!(saved.dictionary_draft, "final dictionary edit");
    }

    #[test]
    fn close_save_failure_keeps_worker_available_for_retry() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = AppRuntime::new(directory.path().to_path_buf()).unwrap();
        std::fs::write(
            directory.path().join("native-settings.json"),
            "invalid json",
        )
        .unwrap();
        let (requests, events, stopped) = start(runtime.clone(), Settings::default());
        runtime.request_shutdown();
        requests
            .send(Request::PrepareClose("draft retained".into()))
            .unwrap();
        loop {
            match events.recv_timeout(Duration::from_secs(3)).unwrap() {
                Event::CloseFailed(_) => break,
                Event::CloseReady => panic!("close succeeded despite persistence failure"),
                _ => {}
            }
        }
        assert!(!stopped.load(Ordering::Acquire));
        Settings::default().save(directory.path()).unwrap();
        requests
            .send(Request::PrepareClose("draft retained".into()))
            .unwrap();
        await_close(&events, &stopped);
        assert_eq!(
            Settings::load(directory.path()).unwrap().dictionary_draft,
            "draft retained"
        );
    }

    #[test]
    fn snapshots_continue_during_a_blocked_request() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = AppRuntime::new(directory.path().to_path_buf()).unwrap();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (requests, events, stopped) = start_with_handler(
            runtime,
            Settings::default(),
            move |request, runtime, sender| {
                if matches!(request, Request::Detect(_)) {
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    Ok(())
                } else {
                    handle(request, runtime, sender)
                }
            },
        );
        requests.send(Request::Detect(String::new())).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        events.try_iter().for_each(drop);
        let mut snapshots = 0;
        let deadline = Instant::now() + Duration::from_secs(3);
        while snapshots < 2 && Instant::now() < deadline {
            if matches!(
                events.recv_timeout(Duration::from_millis(800)),
                Ok(Event::Snapshot(_))
            ) {
                snapshots += 1;
            }
        }
        release_tx.send(()).unwrap();
        requests.send(Request::PrepareClose(String::new())).unwrap();
        await_close(&events, &stopped);
        assert_eq!(
            snapshots, 2,
            "progress stopped while the request was blocked"
        );
    }

    #[test]
    fn appearance_change_preserves_saved_recovery_preferences_and_dictionary() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = AppRuntime::new(directory.path().to_path_buf()).unwrap();
        let saved = Settings {
            dark: false,
            english: true,
            charset: "abc123".into(),
            concurrency: 3,
            dictionary_draft: "retained draft".into(),
            ..Settings::default()
        };
        saved.save(directory.path()).unwrap();
        let (events, _receiver) = mpsc::channel();
        handle(Request::SaveAppearance(true), &runtime, &events).unwrap();
        let mut expected = serde_json::to_value(&saved).unwrap();
        expected["dark"] = serde_json::Value::Bool(true);
        assert_eq!(
            serde_json::to_value(Settings::load(directory.path()).unwrap()).unwrap(),
            expected
        );
    }

    #[test]
    fn shutdown_flushes_pending_dictionary_draft_and_respects_clear_preference() {
        for clear_dictionary in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let runtime = AppRuntime::new(directory.path().to_path_buf()).unwrap();
            let settings = Settings {
                clear_dictionary,
                ..Settings::default()
            };
            settings.save(directory.path()).unwrap();
            let (requests, events, stopped) = start(runtime.clone(), settings);

            // 模拟窗口关闭：先禁止新任务，再排入最新草稿和退出请求。
            runtime.request_shutdown();
            requests
                .send(Request::SaveDraft("临关闭前的字典草稿".into()))
                .unwrap();
            requests.send(Request::Shutdown).unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            while !stopped.load(Ordering::Acquire) {
                assert!(Instant::now() < deadline, "worker did not finish shutdown");
                std::thread::sleep(Duration::from_millis(10));
            }
            for event in events.try_iter() {
                if let Event::Error(error) = event {
                    panic!("worker failed: {error}");
                }
            }
            let saved = Settings::load(directory.path()).unwrap();
            assert_eq!(
                saved.dictionary_draft,
                if clear_dictionary {
                    ""
                } else {
                    "临关闭前的字典草稿"
                }
            );
        }
    }
}
