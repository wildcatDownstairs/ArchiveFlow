// migrations.rs 负责 SQLite 数据库的 schema 版本管理。
// 每次应用升级需要修改数据库结构时，都在这里添加一个新的迁移步骤（migrate_to_vN）。
// 这样用户升级应用后，旧数据库会被自动升级到新结构，而不需要重建数据库。

use rusqlite::Connection;
use thiserror::Error;

// pub(crate) 意味着这个常量只在当前 crate（整个 src-tauri 库）内可见，
// 不会暴露给外部。这是比 pub 更精细的可见性控制。
pub(crate) const CURRENT_SCHEMA_VERSION: u32 = 5;

// &'static str 是字符串字面量类型：
//   - 'static 生命周期表示这个字符串在整个程序运行期间都存在（编译进二进制文件）
//   - &str 是"不可变字符串借用"（切片），不拥有数据
//
// v1 版本的 tasks 表定义（不含后来新增的 found_password 和 archive_info 列）
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

// 最新版本的 tasks 表定义，包含所有历史累计的列。
// 全新安装时直接使用这个 SQL，不需要一步步迁移。
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

// 审计日志表（所有版本通用）
const CREATE_AUDIT_EVENTS_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS audit_events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    task_id TEXT,
    description TEXT NOT NULL,
    timestamp TEXT NOT NULL
);";

// 恢复断点表（v5 新增）：存储密码恢复的进度，用于断点续传
const CREATE_RECOVERY_CHECKPOINTS_TABLE_SQL: &str =
    "CREATE TABLE IF NOT EXISTS recovery_checkpoints (
    task_id TEXT PRIMARY KEY,
    mode_json TEXT NOT NULL,
    archive_type TEXT NOT NULL,
    tried INTEGER NOT NULL,
    total INTEGER NOT NULL,
    updated_at TEXT NOT NULL
);";

// 索引可以显著加速按某列查询（代价是写入时稍慢，占用少量额外空间）。
// IF NOT EXISTS 保证重复执行不报错。
const CREATE_INDEXES_SQL: &str = "CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_audit_task_id ON audit_events(task_id);
CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_events(timestamp);
CREATE INDEX IF NOT EXISTS idx_recovery_checkpoints_updated_at ON recovery_checkpoints(updated_at);";

// 迁移专用的错误类型
// #[error(transparent)] 表示直接透传内部错误的 Display 实现，
// 不添加额外的前缀文字。
#[derive(Debug, Error)]
pub(crate) enum MigrationError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("数据库 schema 版本 {found} 高于当前应用支持的版本 {supported}")]
    FutureSchemaVersion { found: u32, supported: u32 },
}

/// 执行数据库迁移的入口函数。
///
/// 流程：
///   1. 开启事务（保证迁移要么全部成功，要么全部回滚）
///   2. 检测当前数据库版本
///   3. 全新数据库：直接建最新 schema
///   4. 旧数据库：逐版本迁移（v1→v2→...→CURRENT）
///   5. 提交事务
pub(crate) fn migrate(conn: &mut Connection) -> Result<(), MigrationError> {
    // conn.transaction() 开启一个 SQLite 事务。
    // Rust 的 ? 操作符：如果返回 Err，立即从当前函数返回该错误（错误传播）。
    // 这等价于其他语言的 try/catch，但更轻量且无运行时开销。
    let tx = conn.transaction()?;
    let detected_version = detect_schema_version(&tx)?;

    // 数据库版本比代码支持的版本还新 → 说明用户用了旧版本应用打开新版本数据库，
    // 这是危险操作，直接返回错误。
    if detected_version > CURRENT_SCHEMA_VERSION {
        return Err(MigrationError::FutureSchemaVersion {
            found: detected_version,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }

    if is_database_empty(&tx)? {
        // 全新安装：直接建完整的最新 schema，不需要经历历史迁移步骤
        create_latest_schema(&tx)?;
        set_user_version(&tx, CURRENT_SCHEMA_VERSION)?;
        log::info!("数据库初始化完成: schema v{}", CURRENT_SCHEMA_VERSION);
    } else {
        // 已有数据库：逐步升级（while 循环保证每个版本都被处理）
        let mut version = detected_version;
        while version < CURRENT_SCHEMA_VERSION {
            let next_version = version + 1;
            apply_migration(&tx, next_version)?;
            // 每个版本迁移成功后立刻更新 user_version，
            // 这样即使中途出错，下次也知道从哪里继续。
            set_user_version(&tx, next_version)?;
            log::info!("数据库迁移完成: v{} -> v{}", version, next_version);
            version = next_version;
        }
    }

    // 提交事务：所有 SQL 变更一次性写入磁盘，保证原子性。
    tx.commit()?;
    Ok(())
}

/// 检测数据库当前 schema 版本。
///
/// 优先读取 SQLite 的 user_version PRAGMA（v1+ 版本的数据库都会设置）。
/// 如果 user_version 为 0（非常老的数据库没有设置），则通过表结构推断。
fn detect_schema_version(conn: &Connection) -> Result<u32, rusqlite::Error> {
    let user_version = get_user_version(conn)?;
    if user_version > 0 {
        return Ok(user_version);
    }

    infer_legacy_version(conn)
}

/// 通过表和列的存在性推断旧版本数据库的 schema 版本。
///
/// 历史对应关系：
///   - v1: tasks 表存在，但无 found_password、archive_info 列
///   - v2: 新增 found_password 列
///   - v3: 新增 archive_info 列（可推断为 v3+）
fn infer_legacy_version(conn: &Connection) -> Result<u32, rusqlite::Error> {
    // 连 tasks 表都没有 → 空数据库，版本 0
    if !table_exists(conn, "tasks")? {
        return Ok(0);
    }

    let has_found_password = column_exists(conn, "tasks", "found_password")?;
    let has_archive_info = column_exists(conn, "tasks", "archive_info")?;

    // if/else if/else 表达式：Rust 中 if 是表达式，有返回值
    let version = if has_archive_info {
        3
    } else if has_found_password {
        2
    } else {
        1
    };

    Ok(version)
}

/// 检查数据库是否完全空（没有用户表）
fn is_database_empty(conn: &Connection) -> Result<bool, rusqlite::Error> {
    // query_row 执行一个返回单行的 SQL 查询。
    // 闭包 |row| row.get(0) 从结果行中读取第一列的值（COUNT(*)）。
    // sqlite_master 是 SQLite 的内置系统表，记录所有对象（表、索引等）。
    let table_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    Ok(table_count == 0)
}

/// 根据目标版本分发到对应的迁移函数
fn apply_migration(conn: &Connection, target_version: u32) -> Result<(), rusqlite::Error> {
    match target_version {
        1 => migrate_to_v1(conn),
        2 => migrate_to_v2(conn),
        3 => migrate_to_v3(conn),
        4 => migrate_to_v4(conn),
        5 => migrate_to_v5(conn),
        // unreachable!() 宏：告诉编译器和读者"这里在逻辑上不可能到达"。
        // 如果真的到达了（说明代码有 bug），程序会 panic 并打印消息。
        v => unreachable!("未注册的迁移版本 v{v}；请在 apply_migration 中添加对应分支"),
    }
}

/// 全新数据库：一次性建所有表和索引
fn create_latest_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    // execute_batch 可以一次执行多条 SQL 语句（用分号分隔）
    conn.execute_batch(CREATE_TASKS_TABLE_LATEST_SQL)?;
    conn.execute_batch(CREATE_AUDIT_EVENTS_TABLE_SQL)?;
    conn.execute_batch(CREATE_RECOVERY_CHECKPOINTS_TABLE_SQL)?;
    conn.execute_batch(CREATE_INDEXES_SQL)?;
    Ok(())
}

/// v1 迁移：建立基础 tasks 表和审计日志表
fn migrate_to_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(CREATE_TASKS_TABLE_V1_SQL)?;
    conn.execute_batch(CREATE_AUDIT_EVENTS_TABLE_SQL)?;
    conn.execute_batch(CREATE_INDEXES_SQL)?;
    Ok(())
}

/// v2 迁移：给 tasks 表新增 found_password 列（记录找到的密码）
fn migrate_to_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    // 先检查列是否已存在，防止重复添加（ALTER TABLE 如果列已存在会报错）。
    // 这种幂等性设计让迁移函数可以安全地重复执行。
    if !column_exists(conn, "tasks", "found_password")? {
        conn.execute("ALTER TABLE tasks ADD COLUMN found_password TEXT", [])?;
        log::info!("数据库迁移: 已添加 tasks.found_password");
    }

    Ok(())
}

/// v3 迁移：给 tasks 表新增 archive_info 列（存储压缩包元信息 JSON）
fn migrate_to_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    if !column_exists(conn, "tasks", "archive_info")? {
        conn.execute("ALTER TABLE tasks ADD COLUMN archive_info TEXT", [])?;
        log::info!("数据库迁移: 已添加 tasks.archive_info");
    }

    Ok(())
}

/// v4 迁移：规范化旧版本遗留的任务状态字符串
///
/// 旧版本数据库中存在 "verifying"、"cleaned"、"imported" 等非标准状态，
/// 本次迁移把它们统一转换为当前标准的状态值。
fn migrate_to_v4(conn: &Connection) -> Result<(), rusqlite::Error> {
    normalize_task_statuses(conn)?;
    Ok(())
}

/// v5 迁移：新增恢复断点表，支持密码恢复的断点续传功能
fn migrate_to_v5(conn: &Connection) -> Result<(), rusqlite::Error> {
    // v5 新增恢复断点表。
    // 这里把"上次跑到哪里"的状态从内存搬到数据库里，这样应用重启后还能继续。
    conn.execute_batch(CREATE_RECOVERY_CHECKPOINTS_TABLE_SQL)?;
    Ok(())
}

/// 执行一系列 SQL UPDATE，把旧版本的非标准状态值转换为标准值。
///
/// 使用数组存储 (目标状态名, SQL语句) 对，统一循环处理，避免重复代码。
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

    // for...in 遍历数组，(normalized, sql) 是元组解构（tuple destructuring）：
    // 把元组的两个元素分别绑定到 normalized 和 sql 变量。
    for (normalized, sql) in updates {
        // execute 返回受影响的行数
        let affected = conn.execute(sql, [])?;
        if affected > 0 {
            log::info!("数据库状态正规化: {} 条任务 -> {}", affected, normalized);
        }
    }

    Ok(())
}

/// 读取 SQLite 的 user_version PRAGMA（用于追踪 schema 版本）
fn get_user_version(conn: &Connection) -> Result<u32, rusqlite::Error> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

/// 设置 SQLite 的 user_version PRAGMA
fn set_user_version(conn: &Connection, version: u32) -> Result<(), rusqlite::Error> {
    conn.pragma_update(None, "user_version", version)
}

/// 检查指定表是否在数据库中存在
fn table_exists(conn: &Connection, table_name: &str) -> Result<bool, rusqlite::Error> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1",
        [table_name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// 检查指定表中是否存在某列
///
/// PRAGMA table_info(table_name) 返回该表所有列的元信息，
/// 第 2 列（索引 1）是列名。
fn column_exists(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<bool, rusqlite::Error> {
    // prepare 编译 SQL 语句，生成可重复执行的预处理语句（PreparedStatement）。
    // format! 宏：字符串插值，生成 "PRAGMA table_info(tasks)" 这样的字符串。
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table_name})"))?;
    // query_map 对每一行结果执行闭包，返回迭代器。
    // row.get::<_, String>(1) 读取第 1 列（0-indexed）作为 String 类型。
    // ::<_, String> 是 Rust 的"turbofish"语法，用来消除类型推断的歧义。
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;

    for column in columns {
        // column? 解开 Result：成功则得到列名字符串，失败则传播错误
        if column? == column_name {
            return Ok(true);
        }
    }

    Ok(false)
}
