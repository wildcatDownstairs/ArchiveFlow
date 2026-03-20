use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::domain::archive::ArchiveInfo;
use crate::domain::audit::{AuditEvent, AuditEventType};
use crate::domain::task::{ArchiveType, Task, TaskStatus};
use crate::errors::AppError;
use chrono::{DateTime, Utc};

pub struct Database {
    pub conn: Mutex<Connection>,
}

impl Database {
    pub fn new(app_dir: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let db_path = app_dir.join("archiveflow.db");
        let conn = Connection::open(&db_path)?;

        // 创建表
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tasks (
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
            );

            CREATE TABLE IF NOT EXISTS audit_events (
                id TEXT PRIMARY KEY,
                event_type TEXT NOT NULL,
                task_id TEXT,
                description TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
            CREATE INDEX IF NOT EXISTS idx_audit_task_id ON audit_events(task_id);
            CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_events(timestamp);",
        )?;

        // 数据库迁移: 添加 archive_info 列（如果不存在）
        let has_archive_info: bool = conn
            .prepare("SELECT COUNT(*) FROM pragma_table_info('tasks') WHERE name='archive_info'")?
            .query_row([], |row| row.get::<_, i64>(0))
            .map(|count| count > 0)?;

        if !has_archive_info {
            conn.execute("ALTER TABLE tasks ADD COLUMN archive_info TEXT", [])?;
            log::info!("数据库迁移: 已添加 archive_info 列");
        }

        let has_found_password: bool = conn
            .prepare("SELECT COUNT(*) FROM pragma_table_info('tasks') WHERE name='found_password'")?
            .query_row([], |row| row.get::<_, i64>(0))
            .map(|count| count > 0)?;

        if !has_found_password {
            conn.execute("ALTER TABLE tasks ADD COLUMN found_password TEXT", [])?;
            log::info!("数据库迁移: 已添加 found_password 列");
        }

        Self::normalize_task_statuses(&conn)?;

        log::info!("数据库初始化完成: {:?}", db_path);
        Ok(Self {
            conn: Mutex::new(conn),
        })
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
        let deleted = conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
        if deleted == 0 {
            return Err(AppError::TaskNotFound(id.to_string()));
        }
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
}
