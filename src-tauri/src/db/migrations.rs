use rusqlite::Connection;
use thiserror::Error;

pub(crate) const CURRENT_SCHEMA_VERSION: u32 = 4;

const CREATE_TASKS_TABLE_V1_SQL: &str = "CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    file_path TEXT NOT NULL,
    file_name TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    archive_type TEXT NOT NULL DEFAULT 'unknown',
    status TEXT NOT NULL DEFAULT 'ready',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    error_message TEXT
);";

const CREATE_TASKS_TABLE_LATEST_SQL: &str = "CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    file_path TEXT NOT NULL,
    file_name TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    archive_type TEXT NOT NULL DEFAULT 'unknown',
    status TEXT NOT NULL DEFAULT 'ready',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    error_message TEXT,
    found_password TEXT,
    archive_info TEXT
);";

const CREATE_AUDIT_EVENTS_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS audit_events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    task_id TEXT,
    description TEXT NOT NULL,
    timestamp TEXT NOT NULL
);";

const CREATE_INDEXES_SQL: &str = "CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_audit_task_id ON audit_events(task_id);
CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_events(timestamp);";

#[derive(Debug, Error)]
pub(crate) enum MigrationError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("数据库 schema 版本 {found} 高于当前应用支持的版本 {supported}")]
    FutureSchemaVersion { found: u32, supported: u32 },
}

pub(crate) fn migrate(conn: &mut Connection) -> Result<(), MigrationError> {
    let tx = conn.transaction()?;
    let detected_version = detect_schema_version(&tx)?;

    if detected_version > CURRENT_SCHEMA_VERSION {
        return Err(MigrationError::FutureSchemaVersion {
            found: detected_version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }

    if is_database_empty(&tx)? {
        create_latest_schema(&tx)?;
        set_user_version(&tx, CURRENT_SCHEMA_VERSION)?;
        log::info!("数据库初始化完成: schema v{}", CURRENT_SCHEMA_VERSION);
    } else {
        let mut version = detected_version;
        while version < CURRENT_SCHEMA_VERSION {
            let next_version = version + 1;
            apply_migration(&tx, next_version)?;
            set_user_version(&tx, next_version)?;
            log::info!("数据库迁移完成: v{} -> v{}", version, next_version);
            version = next_version;
        }
    }

    tx.commit()?;
    Ok(())
}

fn detect_schema_version(conn: &Connection) -> Result<u32, rusqlite::Error> {
    let user_version = get_user_version(conn)?;
    if user_version > 0 {
        return Ok(user_version);
    }

    infer_legacy_version(conn)
}

fn infer_legacy_version(conn: &Connection) -> Result<u32, rusqlite::Error> {
    if !table_exists(conn, "tasks")? {
        return Ok(0);
    }

    let has_found_password = column_exists(conn, "tasks", "found_password")?;
    let has_archive_info = column_exists(conn, "tasks", "archive_info")?;

    let version = if has_archive_info {
        3
    } else if has_found_password {
        2
    } else {
        1
    };

    Ok(version)
}

fn is_database_empty(conn: &Connection) -> Result<bool, rusqlite::Error> {
    let table_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    Ok(table_count == 0)
}

fn apply_migration(conn: &Connection, target_version: u32) -> Result<(), rusqlite::Error> {
    match target_version {
        1 => migrate_to_v1(conn),
        2 => migrate_to_v2(conn),
        3 => migrate_to_v3(conn),
        4 => migrate_to_v4(conn),
        v => unreachable!("未注册的迁移版本 v{v}；请在 apply_migration 中添加对应分支"),
    }
}

fn create_latest_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(CREATE_TASKS_TABLE_LATEST_SQL)?;
    conn.execute_batch(CREATE_AUDIT_EVENTS_TABLE_SQL)?;
    conn.execute_batch(CREATE_INDEXES_SQL)?;
    Ok(())
}

fn migrate_to_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(CREATE_TASKS_TABLE_V1_SQL)?;
    conn.execute_batch(CREATE_AUDIT_EVENTS_TABLE_SQL)?;
    conn.execute_batch(CREATE_INDEXES_SQL)?;
    Ok(())
}

fn migrate_to_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    if !column_exists(conn, "tasks", "found_password")? {
        conn.execute("ALTER TABLE tasks ADD COLUMN found_password TEXT", [])?;
        log::info!("数据库迁移: 已添加 tasks.found_password");
    }

    Ok(())
}

fn migrate_to_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    if !column_exists(conn, "tasks", "archive_info")? {
        conn.execute("ALTER TABLE tasks ADD COLUMN archive_info TEXT", [])?;
        log::info!("数据库迁移: 已添加 tasks.archive_info");
    }

    Ok(())
}

fn migrate_to_v4(conn: &Connection) -> Result<(), rusqlite::Error> {
    normalize_task_statuses(conn)?;
    Ok(())
}

fn normalize_task_statuses(conn: &Connection) -> Result<(), rusqlite::Error> {
    let updates = [
        (
            "processing",
            "UPDATE tasks SET status = 'processing' WHERE status = 'verifying'",
        ),
        (
            "cancelled",
            "UPDATE tasks SET status = 'cancelled' WHERE status = 'cleaned'",
        ),
        (
            "ready",
            "UPDATE tasks SET status = 'ready'
             WHERE status IN ('imported', 'inspecting', 'waiting_authorization')
             AND archive_type IN ('zip', 'sevenz', 'rar')
             AND archive_info IS NOT NULL",
        ),
        (
            "unsupported",
            "UPDATE tasks SET status = 'unsupported'
             WHERE status IN ('imported', 'inspecting', 'waiting_authorization')
             AND archive_type = 'unknown'
             AND error_message IS NULL",
        ),
        (
            "failed",
            "UPDATE tasks SET status = 'failed'
             WHERE status IN ('imported', 'inspecting', 'waiting_authorization')
             AND (
                (archive_type IN ('zip', 'sevenz', 'rar') AND archive_info IS NULL)
                OR (archive_type = 'unknown' AND error_message IS NOT NULL)
             )",
        ),
    ];

    for (normalized, sql) in updates {
        let affected = conn.execute(sql, [])?;
        if affected > 0 {
            log::info!("数据库状态正规化: {} 条任务 -> {}", affected, normalized);
        }
    }

    Ok(())
}

fn get_user_version(conn: &Connection) -> Result<u32, rusqlite::Error> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn set_user_version(conn: &Connection, version: u32) -> Result<(), rusqlite::Error> {
    conn.pragma_update(None, "user_version", version)
}

fn table_exists(conn: &Connection, table_name: &str) -> Result<bool, rusqlite::Error> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
        [table_name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn column_exists(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<bool, rusqlite::Error> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table_name})"))?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;

    for column in columns {
        if column? == column_name {
            return Ok(true);
        }
    }

    Ok(false)
}
