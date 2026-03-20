mod migrations;

use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::domain::archive::ArchiveInfo;
use crate::domain::audit::{AuditEvent, AuditEventType};
use crate::domain::recovery::{AttackMode, RecoveryCheckpoint};
use crate::domain::task::{ArchiveType, Task, TaskStatus};
use crate::errors::AppError;
use chrono::{DateTime, Utc};

pub struct Database {
    pub conn: Mutex<Connection>,
}

const STARTUP_INTERRUPTED_MESSAGE: &str =
    "应用启动时检测到上次恢复未正常结束，任务已标记为 interrupted";

impl Database {
    pub fn new(app_dir: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let db_path = app_dir.join("archiveflow.db");
        let mut conn = Connection::open(&db_path)?;
        migrations::migrate(&mut conn)?;

        log::info!("数据库初始化完成: {:?}", db_path);
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 从 row 解析 Task（共享逻辑）
    fn parse_task_row(row: &rusqlite::Row) -> rusqlite::Result<Task> {
        let file_size: i64 = row.get(3)?;
        let archive_type_str: String = row.get(4)?;
        let status_str: String = row.get(5)?;
        let created_at_str: String = row.get(6)?;
        let updated_at_str: String = row.get(7)?;
        let error_message: Option<String> = row.get(8)?;
        let found_password: Option<String> = row.get(9)?;
        let archive_info_json: Option<String> = row.get(10)?;

        let archive_type: ArchiveType =
            serde_json::from_value(serde_json::Value::String(archive_type_str))
                .unwrap_or(ArchiveType::Unknown);
        let status = TaskStatus::normalize_persisted(
            &status_str,
            &archive_type,
            error_message.as_deref(),
            archive_info_json.is_some(),
        );
        let created_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let updated_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&updated_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let archive_info: Option<ArchiveInfo> =
            archive_info_json.and_then(|json| serde_json::from_str(&json).ok());

        Ok(Task {
            id: row.get(0)?,
            file_path: row.get(1)?,
            file_name: row.get(2)?,
            file_size: file_size as u64,
            archive_type,
            status,
            created_at,
            updated_at,
            error_message,
            found_password,
            archive_info,
        })
    }

    /// 插入新任务
    pub fn insert_task(&self, task: &Task) -> Result<(), AppError> {
        let conn = self.conn.lock().unwrap();
        let status_str = task.status.as_str().to_string();
        let archive_type_str = serde_json::to_value(&task.archive_type)?
            .as_str()
            .unwrap()
            .to_string();
        let created_at_str = task.created_at.to_rfc3339();
        let updated_at_str = task.updated_at.to_rfc3339();
        let found_password = task.found_password.clone();
        let archive_info_json = task
            .archive_info
            .as_ref()
            .map(|info| serde_json::to_string(info))
            .transpose()?;

        conn.execute(
            "INSERT INTO tasks (id, file_path, file_name, file_size, archive_type, status, created_at, updated_at, error_message, found_password, archive_info)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                task.id,
                task.file_path,
                task.file_name,
                task.file_size as i64,
                archive_type_str,
                status_str,
                created_at_str,
                updated_at_str,
                task.error_message,
                found_password,
                archive_info_json,
            ],
        )?;
        Ok(())
    }

    /// 获取所有任务，按创建时间降序
    pub fn get_all_tasks(&self) -> Result<Vec<Task>, AppError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, file_path, file_name, file_size, archive_type, status, created_at, updated_at, error_message, found_password, archive_info
             FROM tasks ORDER BY created_at DESC",
        )?;

        let tasks = stmt.query_map([], Self::parse_task_row)?;

        let mut result = Vec::new();
        for task in tasks {
            result.push(task?);
        }
        Ok(result)
    }

    /// 按 ID 获取单个任务
    pub fn get_task_by_id(&self, id: &str) -> Result<Option<Task>, AppError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, file_path, file_name, file_size, archive_type, status, created_at, updated_at, error_message, found_password, archive_info
             FROM tasks WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map(params![id], Self::parse_task_row)?;

        match rows.next() {
            Some(task) => Ok(Some(task?)),
            None => Ok(None),
        }
    }

    /// 更新任务状态
    pub fn update_task_status(
        &self,
        id: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), AppError> {
        let conn = self.conn.lock().unwrap();
        let updated_at = Utc::now().to_rfc3339();
        let updated = conn.execute(
            "UPDATE tasks SET status = ?1, updated_at = ?2, error_message = ?3 WHERE id = ?4",
            params![status, updated_at, error_message, id],
        )?;
        if updated == 0 {
            return Err(AppError::TaskNotFound(id.to_string()));
        }
        Ok(())
    }

    /// 更新恢复任务终态，确保密码与错误字段语义正确
    pub fn update_task_recovery_result(
        &self,
        id: &str,
        status: &str,
        error_message: Option<&str>,
        found_password: Option<&str>,
    ) -> Result<(), AppError> {
        let conn = self.conn.lock().unwrap();
        let updated_at = Utc::now().to_rfc3339();
        let updated = conn.execute(
            "UPDATE tasks
             SET status = ?1, updated_at = ?2, error_message = ?3, found_password = ?4
             WHERE id = ?5",
            params![status, updated_at, error_message, found_password, id],
        )?;
        if updated == 0 {
            return Err(AppError::TaskNotFound(id.to_string()));
        }
        Ok(())
    }

    /// 启动时将残留的 processing 任务转为 interrupted
    pub fn interrupt_processing_tasks(&self) -> Result<Vec<Task>, AppError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, file_path, file_name, file_size, archive_type, status, created_at, updated_at, error_message, found_password, archive_info
             FROM tasks WHERE status = 'processing' ORDER BY created_at ASC",
        )?;

        let tasks = stmt.query_map([], Self::parse_task_row)?;
        let mut interrupted_tasks = Vec::new();
        for task in tasks {
            interrupted_tasks.push(task?);
        }

        drop(stmt);

        if interrupted_tasks.is_empty() {
            return Ok(interrupted_tasks);
        }

        let now = Utc::now();
        let updated_at = now.to_rfc3339();

        for task in &mut interrupted_tasks {
            conn.execute(
                "UPDATE tasks
                 SET status = 'interrupted', updated_at = ?1, error_message = ?2
                 WHERE id = ?3",
                params![updated_at, STARTUP_INTERRUPTED_MESSAGE, task.id],
            )?;
            task.status = TaskStatus::Interrupted;
            task.updated_at = now;
            task.error_message = Some(STARTUP_INTERRUPTED_MESSAGE.to_string());
        }

        Ok(interrupted_tasks)
    }

    /// 更新任务的 archive_info
    #[allow(dead_code)]
    pub fn update_task_archive_info(
        &self,
        id: &str,
        archive_type: &str,
        archive_info: &ArchiveInfo,
    ) -> Result<(), AppError> {
        let conn = self.conn.lock().unwrap();
        let updated_at = Utc::now().to_rfc3339();
        let archive_info_json = serde_json::to_string(archive_info)?;
        let updated = conn.execute(
            "UPDATE tasks SET archive_type = ?1, archive_info = ?2, updated_at = ?3 WHERE id = ?4",
            params![archive_type, archive_info_json, updated_at, id],
        )?;
        if updated == 0 {
            return Err(AppError::TaskNotFound(id.to_string()));
        }
        Ok(())
    }

    /// 删除任务
    pub fn delete_task(&self, id: &str) -> Result<(), AppError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM recovery_checkpoints WHERE task_id = ?1",
            params![id],
        )?;
        let deleted = conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
        if deleted == 0 {
            return Err(AppError::TaskNotFound(id.to_string()));
        }
        Ok(())
    }

    // ─── Recovery Checkpoints ───────────────────────────────────────

    fn parse_recovery_checkpoint_row(
        row: &rusqlite::Row,
    ) -> rusqlite::Result<RecoveryCheckpoint> {
        // SQLite 里保存的是字符串和整数，这里把它们重新组装成业务结构体，
        // 这样上层代码就不需要关心底层表结构细节。
        let mode_json: String = row.get(1)?;
        let archive_type_str: String = row.get(2)?;
        let tried: i64 = row.get(3)?;
        let total: i64 = row.get(4)?;
        let updated_at_str: String = row.get(5)?;

        let mode: AttackMode = serde_json::from_str(&mode_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
        let archive_type: ArchiveType =
            serde_json::from_value(serde_json::Value::String(archive_type_str))
                .unwrap_or(ArchiveType::Unknown);
        let updated_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&updated_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        Ok(RecoveryCheckpoint {
            task_id: row.get(0)?,
            mode,
            archive_type,
            tried: tried as u64,
            total: total as u64,
            updated_at,
        })
    }

    pub fn upsert_recovery_checkpoint(&self, checkpoint: &RecoveryCheckpoint) -> Result<(), AppError> {
        let conn = self.conn.lock().unwrap();
        let mode_json = serde_json::to_string(&checkpoint.mode)?;
        let archive_type_str = serde_json::to_value(&checkpoint.archive_type)?
            .as_str()
            .unwrap()
            .to_string();

        conn.execute(
            "INSERT INTO recovery_checkpoints (task_id, mode_json, archive_type, tried, total, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(task_id) DO UPDATE SET
                mode_json = excluded.mode_json,
                archive_type = excluded.archive_type,
                tried = excluded.tried,
                total = excluded.total,
                updated_at = excluded.updated_at",
            params![
                checkpoint.task_id,
                mode_json,
                archive_type_str,
                checkpoint.tried as i64,
                checkpoint.total as i64,
                checkpoint.updated_at.to_rfc3339(),
            ],
        )?;

        Ok(())
    }

    pub fn get_recovery_checkpoint(
        &self,
        task_id: &str,
    ) -> Result<Option<RecoveryCheckpoint>, AppError> {
        // checkpoint 是“可选”的：新任务或从未开始过恢复的任务本来就没有断点。
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT task_id, mode_json, archive_type, tried, total, updated_at
             FROM recovery_checkpoints WHERE task_id = ?1",
        )?;

        let mut rows = stmt.query_map(params![task_id], Self::parse_recovery_checkpoint_row)?;

        match rows.next() {
            Some(checkpoint) => Ok(Some(checkpoint?)),
            None => Ok(None),
        }
    }

    pub fn delete_recovery_checkpoint(&self, task_id: &str) -> Result<(), AppError> {
        // 删除时不把“没找到记录”当成错误，因为成功/穷尽后的清理逻辑会重复调用这里。
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM recovery_checkpoints WHERE task_id = ?1",
            params![task_id],
        )?;
        Ok(())
    }

    // ─── Audit Events ───────────────────────────────────────────────

    /// 从 row 解析 AuditEvent（共享逻辑）
    fn parse_audit_event_row(row: &rusqlite::Row) -> rusqlite::Result<AuditEvent> {
        let event_type_str: String = row.get(1)?;
        let timestamp_str: String = row.get(4)?;

        let event_type = AuditEventType::parse_persisted(&event_type_str)
            .unwrap_or(AuditEventType::FileImported);
        let timestamp: DateTime<Utc> = DateTime::parse_from_rfc3339(&timestamp_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        Ok(AuditEvent {
            id: row.get(0)?,
            event_type,
            task_id: row.get(2)?,
            description: row.get(3)?,
            timestamp,
        })
    }

    /// 插入审计事件
    pub fn insert_audit_event(&self, event: &AuditEvent) -> Result<(), AppError> {
        let conn = self.conn.lock().unwrap();
        let event_type_str = event.event_type.as_str().to_string();
        let timestamp_str = event.timestamp.to_rfc3339();

        conn.execute(
            "INSERT INTO audit_events (id, event_type, task_id, description, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                event.id,
                event_type_str,
                event.task_id,
                event.description,
                timestamp_str,
            ],
        )?;
        Ok(())
    }

    /// 清除所有审计事件，并保留一条“已清理”审计记录
    pub fn clear_audit_events_and_record(&self, event: &AuditEvent) -> Result<u64, AppError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let cleared: i64 = tx.query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))?;
        let event_type_str = event.event_type.as_str().to_string();
        let timestamp_str = event.timestamp.to_rfc3339();

        tx.execute("DELETE FROM audit_events", [])?;
        tx.execute(
            "INSERT INTO audit_events (id, event_type, task_id, description, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                event.id,
                event_type_str,
                event.task_id,
                event.description,
                timestamp_str,
            ],
        )?;
        tx.commit()?;

        Ok(cleared as u64)
    }

    /// 获取审计事件，按时间戳降序，限制数量
    pub fn get_audit_events(&self, limit: usize) -> Result<Vec<AuditEvent>, AppError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, event_type, task_id, description, timestamp
             FROM audit_events ORDER BY timestamp DESC LIMIT ?1",
        )?;

        let events = stmt.query_map(params![limit as i64], Self::parse_audit_event_row)?;

        let mut result = Vec::new();
        for event in events {
            result.push(event?);
        }
        Ok(result)
    }

    /// 获取指定任务的审计事件
    pub fn get_audit_events_for_task(&self, task_id: &str) -> Result<Vec<AuditEvent>, AppError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, event_type, task_id, description, timestamp
             FROM audit_events WHERE task_id = ?1 ORDER BY timestamp DESC",
        )?;

        let events = stmt.query_map(params![task_id], Self::parse_audit_event_row)?;

        let mut result = Vec::new();
        for event in events {
            result.push(event?);
        }
        Ok(result)
    }

    /// 清除所有任务
    pub fn clear_all_tasks(&self) -> Result<u64, AppError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM recovery_checkpoints", [])?;
        let count = conn.execute("DELETE FROM tasks", [])?;
        Ok(count as u64)
    }

    /// 获取任务数量
    pub fn get_task_count(&self) -> Result<u64, AppError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// 获取审计事件数量
    pub fn get_audit_event_count(&self) -> Result<u64, AppError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// 清除所有审计事件
    #[cfg(test)]
    pub fn clear_audit_events(&self) -> Result<u64, AppError> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute("DELETE FROM audit_events", [])?;
        Ok(count as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn db_file_path(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("archiveflow.db")
    }

    fn schema_version(conn: &Connection) -> u32 {
        conn.query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap()
    }

    fn column_exists(conn: &Connection, table_name: &str, column_name: &str) -> bool {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table_name})"))
            .unwrap();
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap();

        let exists = columns
            .into_iter()
            .map(|column| column.unwrap())
            .any(|column| column == column_name);
        exists
    }

    fn create_v1_schema(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE tasks (
                id TEXT PRIMARY KEY,
                file_path TEXT NOT NULL,
                file_name TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                archive_type TEXT NOT NULL DEFAULT 'unknown',
                status TEXT NOT NULL DEFAULT 'ready',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                error_message TEXT
            );

            CREATE TABLE audit_events (
                id TEXT PRIMARY KEY,
                event_type TEXT NOT NULL,
                task_id TEXT,
                description TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );",
        )
        .unwrap();
    }

    fn insert_v1_task(
        conn: &Connection,
        id: &str,
        archive_type: &str,
        status: &str,
        error_message: Option<&str>,
    ) {
        conn.execute(
            "INSERT INTO tasks (
                id, file_path, file_name, file_size, archive_type, status, created_at, updated_at, error_message
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                format!("/tmp/{id}.zip"),
                format!("{id}.zip"),
                1_i64,
                archive_type,
                status,
                Utc::now().to_rfc3339(),
                Utc::now().to_rfc3339(),
                error_message,
            ],
        )
        .unwrap();
    }

    fn make_test_task(id: &str) -> Task {
        Task {
            id: id.to_string(),
            file_path: format!("/tmp/{}.zip", id),
            file_name: format!("{}.zip", id),
            file_size: 1024,
            archive_type: ArchiveType::Zip,
            status: TaskStatus::Ready,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            error_message: None,
            found_password: None,
            archive_info: None,
        }
    }

    fn make_test_checkpoint(task_id: &str) -> RecoveryCheckpoint {
        RecoveryCheckpoint {
            task_id: task_id.to_string(),
            mode: AttackMode::Mask {
                mask: "?d?d?d?d".to_string(),
            },
            archive_type: ArchiveType::Zip,
            tried: 123,
            total: 10_000,
            updated_at: Utc::now(),
        }
    }

    fn make_test_audit_event(id: &str, task_id: Option<&str>) -> AuditEvent {
        AuditEvent {
            id: id.to_string(),
            event_type: AuditEventType::FileImported,
            task_id: task_id.map(|s| s.to_string()),
            description: format!("Test event {}", id),
            timestamp: Utc::now(),
        }
    }

    // ─── Database creation ──────────────────────────────────────────

    #[test]
    fn database_new_succeeds() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf());
        assert!(db.is_ok());
    }

    #[test]
    fn database_new_sets_latest_schema_version() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let conn = db.conn.lock().unwrap();
        assert_eq!(schema_version(&conn), migrations::CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn database_tables_exist() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let conn = db.conn.lock().unwrap();

        // Check tasks table exists
        let tasks_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tasks'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tasks_count, 1);

        // Check audit_events table exists
        let audit_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='audit_events'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(audit_count, 1);

        let checkpoint_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='recovery_checkpoints'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(checkpoint_count, 1);
    }

    #[test]
    fn legacy_database_without_user_version_runs_full_migration() {
        let dir = tempdir().unwrap();
        let db_path = db_file_path(&dir);
        let conn = Connection::open(&db_path).unwrap();
        create_v1_schema(&conn);
        insert_v1_task(&conn, "legacy-imported", "zip", "imported", None);
        insert_v1_task(&conn, "legacy-verify", "zip", "verifying", None);
        drop(conn);

        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let conn = db.conn.lock().unwrap();

        assert_eq!(schema_version(&conn), migrations::CURRENT_SCHEMA_VERSION);
        assert!(column_exists(&conn, "tasks", "found_password"));
        assert!(column_exists(&conn, "tasks", "archive_info"));

        let imported_status: String = conn
            .query_row(
                "SELECT status FROM tasks WHERE id = 'legacy-imported'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(imported_status, "failed");

        let verify_status: String = conn
            .query_row(
                "SELECT status FROM tasks WHERE id = 'legacy-verify'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(verify_status, "processing");
    }

    #[test]
    fn versioned_database_migrates_incrementally_from_v2() {
        let dir = tempdir().unwrap();
        let db_path = db_file_path(&dir);
        let conn = Connection::open(&db_path).unwrap();
        create_v1_schema(&conn);
        conn.execute("ALTER TABLE tasks ADD COLUMN found_password TEXT", [])
            .unwrap();
        conn.pragma_update(None, "user_version", 2_u32).unwrap();
        drop(conn);

        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let conn = db.conn.lock().unwrap();

        assert_eq!(schema_version(&conn), migrations::CURRENT_SCHEMA_VERSION);
        assert!(column_exists(&conn, "tasks", "found_password"));
        assert!(column_exists(&conn, "tasks", "archive_info"));
    }

    // ─── Task CRUD ──────────────────────────────────────────────────

    #[test]
    fn insert_and_get_task_by_id() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let task = make_test_task("t1");
        db.insert_task(&task).unwrap();

        let fetched = db.get_task_by_id("t1").unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, "t1");
        assert_eq!(fetched.file_name, "t1.zip");
        assert_eq!(fetched.file_size, 1024);
        assert_eq!(fetched.archive_type, ArchiveType::Zip);
        assert_eq!(fetched.status, TaskStatus::Ready);
    }

    #[test]
    fn get_all_tasks_returns_ordered_by_created_at_desc() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();

        // Insert with increasing timestamps
        for (i, id) in ["t1", "t2", "t3"].iter().enumerate() {
            let mut task = make_test_task(id);
            task.created_at = Utc::now() + chrono::Duration::seconds(i as i64);
            db.insert_task(&task).unwrap();
        }

        let tasks = db.get_all_tasks().unwrap();
        assert_eq!(tasks.len(), 3);
        // DESC order: t3 first, t1 last
        assert_eq!(tasks[0].id, "t3");
        assert_eq!(tasks[2].id, "t1");
    }

    #[test]
    fn get_task_by_id_nonexistent_returns_none() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let result = db.get_task_by_id("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn update_task_status_succeeds() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let task = make_test_task("t1");
        db.insert_task(&task).unwrap();

        db.update_task_status("t1", "processing", None).unwrap();
        let fetched = db.get_task_by_id("t1").unwrap().unwrap();
        assert_eq!(fetched.status, TaskStatus::Processing);
    }

    #[test]
    fn update_task_status_nonexistent_returns_task_not_found() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let result = db.update_task_status("nonexistent", "processing", None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AppError::TaskNotFound(_)));
    }

    #[test]
    fn interrupt_processing_tasks_marks_residual_work_as_interrupted() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();

        let mut processing = make_test_task("processing-task");
        processing.status = TaskStatus::Processing;
        db.insert_task(&processing).unwrap();

        let ready = make_test_task("ready-task");
        db.insert_task(&ready).unwrap();

        let interrupted = db.interrupt_processing_tasks().unwrap();
        assert_eq!(interrupted.len(), 1);
        assert_eq!(interrupted[0].id, "processing-task");
        assert_eq!(interrupted[0].status, TaskStatus::Interrupted);
        assert_eq!(
            interrupted[0].error_message.as_deref(),
            Some(STARTUP_INTERRUPTED_MESSAGE)
        );

        let processing = db.get_task_by_id("processing-task").unwrap().unwrap();
        assert_eq!(processing.status, TaskStatus::Interrupted);

        let ready = db.get_task_by_id("ready-task").unwrap().unwrap();
        assert_eq!(ready.status, TaskStatus::Ready);
    }

    #[test]
    fn legacy_statuses_are_normalized_on_read() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let conn = db.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO tasks (
                id, file_path, file_name, file_size, archive_type, status, created_at, updated_at, error_message, found_password, archive_info
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                "legacy-rar",
                "/tmp/legacy.rar",
                "legacy.rar",
                1_i64,
                "rar",
                "imported",
                Utc::now().to_rfc3339(),
                Utc::now().to_rfc3339(),
                Option::<String>::None,
                Option::<String>::None,
                Option::<String>::None,
            ],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO tasks (
                id, file_path, file_name, file_size, archive_type, status, created_at, updated_at, error_message, found_password, archive_info
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                "legacy-verify",
                "/tmp/legacy.zip",
                "legacy.zip",
                1_i64,
                "zip",
                "verifying",
                Utc::now().to_rfc3339(),
                Utc::now().to_rfc3339(),
                Option::<String>::None,
                Option::<String>::None,
                Some("{\"total_entries\":0,\"total_size\":0,\"is_encrypted\":false,\"has_encrypted_filenames\":false,\"entries\":[]}".to_string()),
            ],
        )
        .unwrap();

        drop(conn);

        let rar = db.get_task_by_id("legacy-rar").unwrap().unwrap();
        assert_eq!(rar.status, TaskStatus::Failed);

        let verify = db.get_task_by_id("legacy-verify").unwrap().unwrap();
        assert_eq!(verify.status, TaskStatus::Processing);
    }

    #[test]
    fn delete_task_removes_it() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let task = make_test_task("t1");
        db.insert_task(&task).unwrap();

        db.delete_task("t1").unwrap();
        let fetched = db.get_task_by_id("t1").unwrap();
        assert!(fetched.is_none());
    }

    #[test]
    fn delete_task_nonexistent_returns_task_not_found() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let result = db.delete_task("nonexistent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::TaskNotFound(_)));
    }

    #[test]
    fn recovery_checkpoint_roundtrip_and_delete() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        db.insert_task(&make_test_task("t1")).unwrap();

        let checkpoint = make_test_checkpoint("t1");
        db.upsert_recovery_checkpoint(&checkpoint).unwrap();

        let fetched = db.get_recovery_checkpoint("t1").unwrap().unwrap();
        assert_eq!(fetched.task_id, "t1");
        assert_eq!(fetched.tried, 123);
        assert_eq!(
            fetched.mode,
            AttackMode::Mask {
                mask: "?d?d?d?d".to_string()
            }
        );

        db.delete_recovery_checkpoint("t1").unwrap();
        assert!(db.get_recovery_checkpoint("t1").unwrap().is_none());
    }

    #[test]
    fn clear_all_tasks_returns_count_and_empties() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();

        db.insert_task(&make_test_task("t1")).unwrap();
        db.insert_task(&make_test_task("t2")).unwrap();
        db.insert_task(&make_test_task("t3")).unwrap();

        let cleared = db.clear_all_tasks().unwrap();
        assert_eq!(cleared, 3);

        let tasks = db.get_all_tasks().unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn get_task_count_returns_correct_number() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();

        assert_eq!(db.get_task_count().unwrap(), 0);
        db.insert_task(&make_test_task("t1")).unwrap();
        assert_eq!(db.get_task_count().unwrap(), 1);
        db.insert_task(&make_test_task("t2")).unwrap();
        assert_eq!(db.get_task_count().unwrap(), 2);
    }

    // ─── Audit Event CRUD ───────────────────────────────────────────

    #[test]
    fn insert_and_get_audit_event() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let event = make_test_audit_event("e1", Some("t1"));
        db.insert_audit_event(&event).unwrap();

        let events = db.get_audit_events(100).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "e1");
        assert_eq!(events[0].task_id.as_deref(), Some("t1"));
        assert_eq!(events[0].description, "Test event e1");
    }

    #[test]
    fn get_audit_events_respects_limit() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();

        for i in 0..5 {
            let event = make_test_audit_event(&format!("e{}", i), None);
            db.insert_audit_event(&event).unwrap();
        }

        let events = db.get_audit_events(3).unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn get_audit_events_for_task_filters_by_task_id() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();

        db.insert_audit_event(&make_test_audit_event("e1", Some("t1")))
            .unwrap();
        db.insert_audit_event(&make_test_audit_event("e2", Some("t2")))
            .unwrap();
        db.insert_audit_event(&make_test_audit_event("e3", Some("t1")))
            .unwrap();

        let events = db.get_audit_events_for_task("t1").unwrap();
        assert_eq!(events.len(), 2);
        for ev in &events {
            assert_eq!(ev.task_id.as_deref(), Some("t1"));
        }
    }

    #[test]
    fn get_audit_event_count_returns_correct_number() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();

        assert_eq!(db.get_audit_event_count().unwrap(), 0);
        db.insert_audit_event(&make_test_audit_event("e1", None))
            .unwrap();
        db.insert_audit_event(&make_test_audit_event("e2", None))
            .unwrap();
        assert_eq!(db.get_audit_event_count().unwrap(), 2);
    }

    #[test]
    fn clear_audit_events_returns_count_and_empties() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();

        db.insert_audit_event(&make_test_audit_event("e1", None))
            .unwrap();
        db.insert_audit_event(&make_test_audit_event("e2", None))
            .unwrap();

        let cleared = db.clear_audit_events().unwrap();
        assert_eq!(cleared, 2);

        let events = db.get_audit_events(100).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn clear_audit_events_and_record_keeps_single_marker_event() {
        let dir = tempdir().unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();

        db.insert_audit_event(&make_test_audit_event("e1", None))
            .unwrap();
        db.insert_audit_event(&make_test_audit_event("e2", Some("t1")))
            .unwrap();

        let marker = AuditEvent {
            id: "marker".to_string(),
            event_type: AuditEventType::AuditLogsCleared,
            task_id: None,
            description: "清除审计日志并保留操作记录: 2 条".to_string(),
            timestamp: Utc::now(),
        };

        let cleared = db.clear_audit_events_and_record(&marker).unwrap();
        assert_eq!(cleared, 2);

        let events = db.get_audit_events(100).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "marker");
        assert_eq!(events[0].event_type, AuditEventType::AuditLogsCleared);
    }
}
