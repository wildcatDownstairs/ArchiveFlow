//! 原生宿主迁移的真实数据库 / 压缩包回归，全部使用临时数据目录。
use archiveflow_core::{
    commands::{
        archive_commands::import_archive,
        export_commands::{export_tasks, ExportOptions},
        recovery_commands::{cancel_recovery, pause_recovery, resume_recovery, start_recovery},
    },
    db::Database,
    domain::{
        recovery::{AttackMode, RecoveryCheckpoint},
        task::{Task, TaskStatus},
    },
    runtime::AppRuntime,
};
use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

fn import(app: &AppRuntime, name: &str) -> Task {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/zip")
        .join(name);
    import_archive(
        path.to_string_lossy().into_owned(),
        name.into(),
        0,
        app.database(),
    )
    .unwrap()
}

fn wait(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(12);
    while !predicate() {
        assert!(Instant::now() < deadline, "后台任务没有及时收敛");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn import_recover_export_and_reopen_without_webview() {
    let dir = tempfile::tempdir().unwrap();
    let app = AppRuntime::new(dir.path().into()).unwrap();
    let task = import(&app, "encrypted-aes.zip");
    start_recovery(
        task.id.clone(),
        "dictionary".into(),
        r#"{"wordlist":["wrong","test123"]}"#.into(),
        None,
        None,
        None,
        app.database(),
        app.scheduler(),
        app.clone(),
    )
    .unwrap();
    wait(|| !app.recovery_manager().has_running_tasks());
    let result = app.database().get_task_by_id(&task.id).unwrap().unwrap();
    assert_eq!(result.status, TaskStatus::Succeeded);
    assert_eq!(result.found_password.as_deref(), Some("test123"));
    assert!(app.progress().contains_key(&task.id));
    let raw = export_tasks(
        app.database(),
        vec![task.id.clone()],
        "json".into(),
        Some(ExportOptions {
            mask_passwords: false,
            include_audit_events: true,
        }),
    )
    .unwrap();
    assert!(raw.contains("test123"));
    for format in ["json", "csv"] {
        let masked = export_tasks(
            app.database(),
            vec![task.id.clone()],
            format.into(),
            Some(ExportOptions {
                mask_passwords: true,
                include_audit_events: true,
            }),
        )
        .unwrap();
        assert!(!masked.contains("test123"));
    }
    drop(app);
    // Worker 清理状态后会完成最后一次调度，等待它释放自己的 runtime 克隆。
    let deadline = Instant::now() + Duration::from_secs(2);
    let reopened = loop {
        match AppRuntime::new(dir.path().into()) {
            Ok(app) => break app,
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Err(e) => panic!("{e}"),
        }
    };
    assert_eq!(
        reopened
            .database()
            .get_task_by_id(&task.id)
            .unwrap()
            .unwrap()
            .found_password
            .as_deref(),
        Some("test123")
    );
}

#[test]
fn existing_database_checkpoint_and_instance_lock_are_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::new(dir.path().into()).unwrap();
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/zip/encrypted-aes.zip");
    let task = import_archive(
        path.to_string_lossy().into_owned(),
        "legacy.zip".into(),
        0,
        &db,
    )
    .unwrap();
    db.update_task_status(&task.id, "processing", None).unwrap();
    db.upsert_recovery_checkpoint(&RecoveryCheckpoint {
        task_id: task.id.clone(),
        mode: AttackMode::Dictionary {
            wordlist: vec!["test123".into()],
        },
        archive_type: task.archive_type,
        priority: 3,
        tried: 0,
        total: 1,
        updated_at: chrono::Utc::now(),
    })
    .unwrap();
    drop(db);
    let app = AppRuntime::new(dir.path().into()).unwrap();
    assert_eq!(
        app.database()
            .get_task_by_id(&task.id)
            .unwrap()
            .unwrap()
            .status,
        TaskStatus::Interrupted
    );
    assert_eq!(
        app.database()
            .get_recovery_checkpoint(&task.id)
            .unwrap()
            .unwrap()
            .priority,
        3
    );
    assert!(AppRuntime::new(dir.path().into()).is_err());
    resume_recovery(
        task.id.clone(),
        app.database(),
        app.scheduler(),
        app.clone(),
    )
    .unwrap();
    wait(|| !app.recovery_manager().has_running_tasks());
    assert_eq!(
        app.database()
            .get_task_by_id(&task.id)
            .unwrap()
            .unwrap()
            .status,
        TaskStatus::Succeeded
    );
}

#[test]
fn pause_resume_and_cancel_keep_cpu_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let app = AppRuntime::new(dir.path().into()).unwrap();
    let task = import(&app, "encrypted-aes.zip");
    start_recovery(
        task.id.clone(),
        "bruteforce".into(),
        r#"{"charset":"0123456789","min_length":7,"max_length":8}"#.into(),
        None,
        None,
        None,
        app.database(),
        app.scheduler(),
        app.clone(),
    )
    .unwrap();
    wait(|| {
        app.database()
            .get_recovery_checkpoint(&task.id)
            .unwrap()
            .is_some()
    });
    pause_recovery(
        task.id.clone(),
        app.scheduler(),
        app.recovery_manager(),
        app.database(),
    )
    .unwrap();
    wait(|| !app.recovery_manager().has_running_tasks());
    assert!(app
        .database()
        .get_recovery_checkpoint(&task.id)
        .unwrap()
        .is_some());
    resume_recovery(
        task.id.clone(),
        app.database(),
        app.scheduler(),
        app.clone(),
    )
    .unwrap();
    wait(|| app.recovery_manager().is_running(&task.id));
    cancel_recovery(
        task.id.clone(),
        app.database(),
        app.scheduler(),
        app.recovery_manager(),
    )
    .unwrap();
    wait(|| !app.recovery_manager().has_running_tasks());
    assert_eq!(
        app.database()
            .get_task_by_id(&task.id)
            .unwrap()
            .unwrap()
            .status,
        TaskStatus::Cancelled
    );
    assert!(app
        .database()
        .get_recovery_checkpoint(&task.id)
        .unwrap()
        .is_some());
}
