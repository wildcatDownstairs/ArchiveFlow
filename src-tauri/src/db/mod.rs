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
                status TEXT NOT NULL DEFAULT 'imported',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                error_message TEXT,
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
        let archive_info_json: Option<String> = row.get(9)?;

        let archive_type: ArchiveType =
            serde_json::from_value(serde_json::Value::String(archive_type_str))
                .unwrap_or(ArchiveType::Unknown);
        let status: TaskStatus = serde_json::from_value(serde_json::Value::String(status_str))
            .unwrap_or(TaskStatus::Imported);
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
            archive_info,
        })
    }

    /// 插入新任务
    pub fn insert_task(&self, task: &Task) -> Result<(), AppError> {
        let conn = self.conn.lock().unwrap();
        let status_str = serde_json::to_value(&task.status)?
            .as_str()
            .unwrap()
            .to_string();
        let archive_type_str = serde_json::to_value(&task.archive_type)?
            .as_str()
            .unwrap()
            .to_string();
        let created_at_str = task.created_at.to_rfc3339();
        let updated_at_str = task.updated_at.to_rfc3339();
        let archive_info_json = task
            .archive_info
            .as_ref()
            .map(|info| serde_json::to_string(info))
            .transpose()?;

        conn.execute(
            "INSERT INTO tasks (id, file_path, file_name, file_size, archive_type, status, created_at, updated_at, error_message, archive_info)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
                archive_info_json,
            ],
        )?;
        Ok(())
    }

    /// 获取所有任务，按创建时间降序
    pub fn get_all_tasks(&self) -> Result<Vec<Task>, AppError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, file_path, file_name, file_size, archive_type, status, created_at, updated_at, error_message, archive_info
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
            "SELECT id, file_path, file_name, file_size, archive_type, status, created_at, updated_at, error_message, archive_info
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
        conn.execute(
            "UPDATE tasks SET status = ?1, updated_at = ?2, error_message = ?3 WHERE id = ?4",
            params![status, updated_at, error_message, id],
        )?;
        Ok(())
    }

    /// 更新任务的 archive_info
    pub fn update_task_archive_info(
        &self,
        id: &str,
        archive_type: &str,
        archive_info: &ArchiveInfo,
    ) -> Result<(), AppError> {
        let conn = self.conn.lock().unwrap();
        let updated_at = Utc::now().to_rfc3339();
        let archive_info_json = serde_json::to_string(archive_info)?;
        conn.execute(
            "UPDATE tasks SET archive_type = ?1, archive_info = ?2, updated_at = ?3 WHERE id = ?4",
            params![archive_type, archive_info_json, updated_at, id],
        )?;
        Ok(())
    }

    /// 删除任务
    pub fn delete_task(&self, id: &str) -> Result<(), AppError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ─── Audit Events ───────────────────────────────────────────────

    /// 从 row 解析 AuditEvent（共享逻辑）
    fn parse_audit_event_row(row: &rusqlite::Row) -> rusqlite::Result<AuditEvent> {
        let event_type_str: String = row.get(1)?;
        let timestamp_str: String = row.get(4)?;

        let event_type: AuditEventType =
            serde_json::from_value(serde_json::Value::String(event_type_str))
                .unwrap_or(AuditEventType::TaskCreated);
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
        let event_type_str = serde_json::to_value(&event.event_type)?
            .as_str()
            .unwrap()
            .to_string();
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
}
