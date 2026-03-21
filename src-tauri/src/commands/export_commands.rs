// ============================================================
// export_commands.rs — 任务结果导出的 Tauri 命令
//
// 本文件实现了把任务数据（Task）连同其审计事件（AuditEvent）
// 一起导出为 CSV 或 JSON 格式的功能。
//
// 【数据模型层次】
//   ExportMetadata       — 导出元信息（时间、版本、任务数量）
//   TaskExportReport     — 单任务的摘要报告（状态、密码、事件数等）
//   TaskExportBundle     — 单任务的完整包（task + report + audit_events）
//   TaskExportDocument   — 整份导出文档（metadata + bundles[]）
//
// 暴露给前端的命令只有一个：export_tasks
// ============================================================

use chrono::{DateTime, Utc};
use tauri::State;

use crate::db::Database;
use crate::domain::audit::{AuditEvent, AuditEventType};
use crate::domain::task::{Task, TaskStatus};
use crate::errors::AppError;
use crate::services::audit_service;

/// 导出选项：控制结果是否脱敏、是否附带完整审计事件。
///
/// #[serde(default)] 让前端即使不传 options，也会自动回退到默认配置，
/// 这样旧版本前端依然可以调用这个命令，不会因为字段缺失而报错。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExportOptions {
    mask_passwords: bool,
    include_audit_events: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            mask_passwords: false,
            include_audit_events: true,
        }
    }
}

/// 导出文档的元信息，放在 JSON 顶层，方便接收方了解这份数据的来源。
///
/// 【Rust 概念：#[derive(Debug, Clone, serde::Serialize)]】
///   - `Debug`：允许用 `{:?}` 格式化打印，调试时有用
///   - `Clone`：允许调用 `.clone()` 深拷贝结构体（后面 `bundles.to_vec()` 需要）
///   - `serde::Serialize`：允许用 serde_json 将结构体序列化成 JSON 字符串
#[derive(Debug, Clone, serde::Serialize)]
struct ExportMetadata {
    exported_at: DateTime<Utc>,
    format_version: u32,
    task_count: usize,
}

/// 单任务的摘要报告，由 build_task_report 函数生成。
///
/// 包含人类可读的 summary 字段，适合直接展示或写入 CSV。
#[derive(Debug, Clone, serde::Serialize)]
struct TaskExportReport {
    status: String,
    found_password: Option<String>,
    recovery_parameters: Option<String>,
    failure_reason: Option<String>,
    audit_event_count: usize,
    latest_audit_at: Option<DateTime<Utc>>,
    /// 人类可读的一句话摘要，包含任务名、状态、密码情况、归档详情
    summary: String,
}

/// 单任务的完整导出包，包含原始任务数据 + 报告摘要 + 所有审计事件。
#[derive(Debug, Clone, serde::Serialize)]
struct TaskExportBundle {
    task: Task,
    report: TaskExportReport,
    audit_events: Vec<AuditEvent>,
}

/// 整份 JSON 导出文档的根结构。
///
/// 序列化后格式：
/// ```json
/// {
///   "metadata": { "exported_at": "...", "format_version": 1, "task_count": 3 },
///   "tasks": [ { "task": {...}, "report": {...}, "audit_events": [...] }, ... ]
/// }
/// ```
#[derive(Debug, Clone, serde::Serialize)]
struct TaskExportDocument {
    metadata: ExportMetadata,
    tasks: Vec<TaskExportBundle>,
}

/// CSV 字段转义：包含逗号、双引号或换行时用双引号包裹，内部双引号加倍。
///
/// 这是 RFC 4180 标准的 CSV 转义规则：
///   - 字段含 `,`、`"`、`\n`、`\r` 时，整个字段用 `"..."` 包裹
///   - 字段内部的 `"` 改写为 `""`（加倍转义）
///
/// 【Rust 概念：&str vs String 返回值】
///   输入是 `&str`（引用），输出是 `String`（拥有所有权的堆分配字符串），
///   因为转义后的字符串是新构造的，无法返回对已有数据的引用。
fn escape_csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        // replace('"', "\"\"") 把所有双引号变成两个双引号
        // format!("\"{}\"", ...) 在外面套一对双引号
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// 将 TaskStatus 枚举映射为 CSV/报告中显示的中文标签。
///
/// 【Rust 概念：&'static str 返回值】
///   `'static` 生命周期表示这些字符串字面量编译时就存在于程序的只读数据段，
///   整个程序运行期间都有效，无需运行时分配内存。
fn status_label(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Ready => "就绪",
        TaskStatus::Processing => "处理中",
        TaskStatus::Succeeded => "成功",
        TaskStatus::Exhausted => "已穷尽",
        TaskStatus::Cancelled => "已取消",
        TaskStatus::Failed => "失败",
        TaskStatus::Unsupported => "不支持",
        TaskStatus::Interrupted => "已中断",
    }
}

fn mask_password(password: &str) -> String {
    "•".repeat(password.chars().count())
}

fn export_password(password: Option<&str>, options: &ExportOptions) -> Option<String> {
    password.map(|value| {
        if options.mask_passwords {
            mask_password(value)
        } else {
            value.to_string()
        }
    })
}

fn extract_recovery_parameters(audit_events: &[AuditEvent]) -> Option<String> {
    audit_events
        .iter()
        .find(|event| {
            matches!(
                event.event_type,
                AuditEventType::RecoveryQueued
                    | AuditEventType::RecoveryStarted
                    | AuditEventType::RecoveryResumed
            )
        })
        .map(|event| event.description.clone())
}

fn sanitize_task_for_export(mut task: Task, options: &ExportOptions) -> Task {
    task.found_password = export_password(task.found_password.as_deref(), options);
    task
}

/// 为一个任务构建导出报告（TaskExportReport）。
///
/// 从任务和其审计事件中提炼出人类可读的摘要信息。
///
/// 【Rust 概念：.as_ref() + .map()】
///   `task.archive_info.as_ref()` 获取 `Option<ArchiveInfo>` 内部值的引用，
///   而不 move 所有权。然后 `.map(|info| ...)` 在有值时执行转换，
///   `.unwrap_or_else(|| ...)` 在 None 时提供默认字符串。
///
/// 【Rust 概念：.filter() + .count() 惰性链式迭代】
///   `info.entries.iter().filter(|entry| entry.is_encrypted).count()`
///   不创建临时集合，直接统计满足条件的元素数量，内存高效。
fn build_task_report(
    task: &Task,
    audit_events: &[AuditEvent],
    options: &ExportOptions,
) -> TaskExportReport {
    // 审计事件列表按时间倒序（最新在前），所以 .first() 即最近一次事件
    let latest_audit_at = audit_events.first().map(|event| event.timestamp);
    let recovery_parameters = extract_recovery_parameters(audit_events);
    let failure_reason = task.error_message.clone();
    let exported_password = export_password(task.found_password.as_deref(), options);

    // 密码摘要：有密码显示密码，无密码给出说明
    let password_summary = match exported_password.as_deref() {
        Some(password) if options.mask_passwords => format!("已导出脱敏密码 `{password}`"),
        Some(password) => format!("已恢复密码 `{password}`"),
        None => "未导出密码".to_string(),
    };

    // 归档内容摘要：统计总条目数和加密条目数
    let archive_summary = task
        .archive_info
        .as_ref()
        .map(|info| {
            format!(
                "共 {} 个条目，其中 {} 个加密",
                info.total_entries,
                info.entries
                    .iter()
                    .filter(|entry| entry.is_encrypted)
                    .count()
            )
        })
        .unwrap_or_else(|| "无归档检测详情".to_string());

    // 最近审计事件时间摘要
    let latest_event_summary = latest_audit_at
        .map(|ts| format!("最近审计时间 {}", ts.to_rfc3339()))
        .unwrap_or_else(|| "无审计事件".to_string());
    let recovery_parameter_summary = recovery_parameters
        .as_deref()
        .map(|summary| format!("恢复参数摘要：{summary}"))
        .unwrap_or_else(|| "无恢复参数摘要".to_string());
    let failure_summary = failure_reason
        .as_deref()
        .map(|reason| format!("失败原因：{reason}"))
        .unwrap_or_else(|| "无失败原因".to_string());

    TaskExportReport {
        status: task.status.as_str().to_string(),
        found_password: exported_password,
        recovery_parameters,
        failure_reason,
        audit_event_count: audit_events.len(),
        latest_audit_at,
        // 组合成一句完整的人类可读摘要
        // 注意：这里用了中文的弯引号 \u{201c}...\u{201d} 把文件名括起来，
        // 不能直接用 " 因为那是字符串边界符，需要转义或改用其他符号。
        summary: format!(
            "任务\u{201c}{}\u{201d}当前状态为{}；{}；{}；{}；{}；{}。",
            task.file_name,
            status_label(&task.status),
            password_summary,
            archive_summary,
            recovery_parameter_summary,
            failure_summary,
            latest_event_summary
        ),
    }
}

/// 将任务列表和对应的审计事件列表合并成 TaskExportBundle 列表。
///
/// 【Rust 概念：Iterator::zip】
///   `.zip()` 把两个迭代器"拉链"合并，每次产出一对 (item_a, item_b)。
///   要求两个迭代器长度相同；多余的元素会被丢弃（但这里由 load_export_bundles 保证等长）。
///
/// 【Rust 概念：.into_iter() vs .iter()】
///   `.into_iter()` 消耗集合并转移所有权（move），
///   `.iter()` 只借用产出引用（不转移所有权）。
///   这里用 `into_iter()` 是因为我们要把 task 的所有权移入 TaskExportBundle。
///
/// 【Rust 概念：.collect()】
///   `.collect()` 将迭代器消费成目标集合（这里是 `Vec<TaskExportBundle>`），
///   Rust 通过函数返回值类型自动推断目标类型。
fn build_export_bundles(
    tasks: Vec<Task>,
    audit_events_by_task: Vec<Vec<AuditEvent>>,
    options: &ExportOptions,
) -> Vec<TaskExportBundle> {
    tasks
        .into_iter()
        .zip(audit_events_by_task)
        .map(|(task, audit_events)| {
            let report = build_task_report(&task, &audit_events, options);
            let exported_audit_events = if options.include_audit_events {
                audit_events
            } else {
                Vec::new()
            };
            TaskExportBundle {
                task: sanitize_task_for_export(task, options),
                report,
                audit_events: exported_audit_events,
            }
        })
        .collect()
}

/// 从数据库加载任务和审计事件，构建完整的 TaskExportBundle 列表。
///
/// 【参数说明】
///   - `task_ids` 为空：导出所有任务
///   - `task_ids` 非空：只导出指定 ID 的任务
///
/// 【Rust 概念：Vec::with_capacity(n)】
///   预分配 n 个元素的堆内存，避免 push 时多次重新分配（reallocation），
///   适合在已知元素数量上限时使用。
///
/// 【Rust 概念：collect::<Result<Vec<_>, _>>()】
///   迭代器中每个 map 返回 `Result<Vec<AuditEvent>, AppError>`，
///   `.collect::<Result<Vec<_>, _>>()` 会：
///   - 如果所有操作都 Ok，收集成 `Ok(Vec<Vec<AuditEvent>>)`
///   - 如果任意一个 Err，立即短路并返回 `Err(...)`
///   `_` 让编译器自动推断元素类型，减少冗余代码。
fn load_export_bundles(
    db: &Database,
    task_ids: &[String],
    options: &ExportOptions,
) -> Result<Vec<TaskExportBundle>, AppError> {
    // 根据 task_ids 是否为空决定查询策略
    let tasks = if task_ids.is_empty() {
        db.get_all_tasks()?
    } else {
        let mut tasks = Vec::with_capacity(task_ids.len());
        for id in task_ids {
            // get_task_by_id 返回 Option<Task>，不存在的 ID 静默跳过
            if let Some(task) = db.get_task_by_id(id)? {
                tasks.push(task);
            }
        }
        tasks
    };

    // 为每个任务查询其审计事件，利用 collect::<Result<Vec<_>, _>>() 提前返回错误
    let audit_events_by_task = tasks
        .iter()
        .map(|task| db.get_audit_events_for_task(&task.id))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(build_export_bundles(tasks, audit_events_by_task, options))
}

/// 将 TaskExportBundle 列表序列化为 CSV 格式字符串。
///
/// CSV 格式说明：
///   - 第一行为固定的列标题行（header）
///   - 之后每行对应一个任务
///   - 所有字段都经过 escape_csv_field 处理，确保逗号、引号不破坏格式
///
/// 【Rust 概念：Vec::with_capacity(n + 1)】
///   预留 bundles.len() + 1 行（+1 为标题行），减少堆重新分配。
fn bundles_to_csv(bundles: &[TaskExportBundle]) -> String {
    let mut lines = Vec::with_capacity(bundles.len() + 1);
    // 标题行：列名固定，与下方 format! 中的字段顺序严格对应
    lines.push(
        "id,file_name,file_path,file_size,archive_type,status,created_at,updated_at,found_password,error_message,recovery_parameters,audit_event_count,latest_audit_at,report_summary"
            .to_string(),
    );

    for bundle in bundles {
        let task = &bundle.task;
        // archive_type 是枚举，需要先用 serde_json 序列化为字符串，
        // 再从 JSON Value 中提取 &str，最后转为 String
        let archive_type = serde_json::to_value(&task.archive_type)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        let row = format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            escape_csv_field(&task.id),
            escape_csv_field(&task.file_name),
            escape_csv_field(&task.file_path),
            task.file_size, // u64，纯数字，无需转义
            escape_csv_field(&archive_type),
            escape_csv_field(task.status.as_str()),
            escape_csv_field(&task.created_at.to_rfc3339()),
            escape_csv_field(&task.updated_at.to_rfc3339()),
            // found_password 为 None 时写空字符串
            escape_csv_field(task.found_password.as_deref().unwrap_or("")),
            escape_csv_field(task.error_message.as_deref().unwrap_or("")),
            escape_csv_field(bundle.report.recovery_parameters.as_deref().unwrap_or(""),),
            bundle.report.audit_event_count, // usize，纯数字
            escape_csv_field(
                // latest_audit_at 为 None 时写空字符串
                &bundle
                    .report
                    .latest_audit_at
                    .map(|ts| ts.to_rfc3339())
                    .unwrap_or_default(),
            ),
            escape_csv_field(&bundle.report.summary),
        );
        lines.push(row);
    }

    // 用换行符连接所有行（不在末尾添加多余换行）
    lines.join("\n")
}

/// 将 TaskExportBundle 列表序列化为 JSON 格式字符串。
///
/// 输出格式为带缩进的"美化 JSON"（pretty print），便于人类阅读。
///
/// 【Rust 概念：serde_json::to_string_pretty】
///   `to_string_pretty` 比 `to_string` 多添加缩进和换行，可读性更好。
///   两者都返回 `Result<String, serde_json::Error>`。
///   这里通过 `?` 将 serde_json 错误自动转换为 AppError（利用 From trait 实现）。
fn bundles_to_json(bundles: &[TaskExportBundle]) -> Result<String, AppError> {
    let document = TaskExportDocument {
        metadata: ExportMetadata {
            exported_at: Utc::now(),
            format_version: 1,
            task_count: bundles.len(),
        },
        // .to_vec() 对切片进行克隆，产出拥有所有权的 Vec
        // 这里需要 Clone 因为 bundles 是 &[TaskExportBundle]（借用）
        tasks: bundles.to_vec(),
    };

    Ok(serde_json::to_string_pretty(&document)?)
}

/// 导出任务结果（唯一公开命令）
///
/// 支持将任务数据导出为 CSV 或 JSON 格式，返回格式化后的字符串内容。
/// 前端收到后可以提示用户保存为文件，或直接展示在界面上。
///
/// 【参数说明】
///   - `task_ids`：要导出的任务 ID 列表；空列表表示导出全部
///   - `format`：导出格式，必须是 "csv" 或 "json"
///
/// 【Rust 概念：unreachable!() 宏】
///   在已经通过前置验证确认 format 只能是 "csv" 或 "json" 之后，
///   `_ =>` 分支理论上永远不会执行。用 `unreachable!()` 明确表达这一意图，
///   如果未来代码变更导致此处真的被执行，程序会 panic 并附带清晰的错误信息，
///   比静默执行错误逻辑要好得多。
#[tauri::command]
pub async fn export_tasks(
    db: State<'_, Database>,
    task_ids: Vec<String>,
    format: String,
    options: Option<ExportOptions>,
) -> Result<String, AppError> {
    // 提前验证格式参数，给前端明确的错误信息
    if format != "csv" && format != "json" {
        return Err(AppError::InvalidArgument(format!(
            "不支持的导出格式: {}，仅支持 csv 或 json",
            format
        )));
    }

    let options = options.unwrap_or_default();
    let bundles = load_export_bundles(&db, &task_ids, &options)?;
    // 如果指定了 task_ids 但一个都找不到，说明前端传入了无效 ID
    if !task_ids.is_empty() && bundles.is_empty() {
        return Err(AppError::InvalidArgument("未找到可导出的任务".to_string()));
    }

    // 根据格式选择序列化方式
    let content = match format.as_str() {
        "csv" => bundles_to_csv(&bundles),
        "json" => bundles_to_json(&bundles)?,
        _ => unreachable!(), // 已在上方验证，此分支不可能被执行
    };

    // 单任务导出时关联到该任务，多任务导出时不关联特定任务
    let target_task_id = if bundles.len() == 1 {
        Some(bundles[0].task.id.clone())
    } else {
        None
    };

    // 写审计日志，忽略失败（导出本身已成功，不应因日志失败而报错）
    let _ = audit_service::log_audit_event(
        &db,
        AuditEventType::ResultExported,
        target_task_id,
        format!(
            "导出 {} 条任务恢复结果 (格式: {}, 密码={}, 审计={})",
            bundles.len(),
            format,
            if options.mask_passwords {
                "脱敏"
            } else {
                "明文"
            },
            if options.include_audit_events {
                "包含"
            } else {
                "不包含"
            }
        ),
    );

    Ok(content)
}

// ============================================================
// 单元测试
//
// 这里对纯函数（不依赖数据库或 Tauri 运行时的函数）进行测试。
// 数据库相关的命令（如 export_tasks）需要集成测试，暂不在此处测试。
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::archive::ArchiveInfo;
    use crate::domain::task::ArchiveType;

    fn default_export_options() -> ExportOptions {
        ExportOptions::default()
    }

    /// 构造测试用 Task 的辅助函数，减少测试中的重复代码
    fn make_task(id: &str, name: &str, status: TaskStatus, password: Option<&str>) -> Task {
        Task {
            id: id.to_string(),
            file_path: format!("/tmp/{}", name),
            file_name: name.to_string(),
            file_size: 1024,
            archive_type: ArchiveType::Zip,
            status,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            error_message: None,
            // password.map(String::from) 把 Option<&str> 转为 Option<String>
            found_password: password.map(String::from),
            archive_info: Some(ArchiveInfo {
                total_entries: 2,
                total_size: 1024,
                is_encrypted: true,
                has_encrypted_filenames: false,
                entries: vec![],
            }),
        }
    }

    /// 构造测试用 AuditEvent 的辅助函数
    fn make_event(id: &str, task_id: &str, description: &str) -> AuditEvent {
        AuditEvent {
            id: id.to_string(),
            event_type: AuditEventType::RecoverySucceeded,
            task_id: Some(task_id.to_string()),
            description: description.to_string(),
            timestamp: Utc::now(),
        }
    }

    /// 验证 CSV 输出包含标题行和数据行，且内容正确
    #[test]
    fn csv_header_and_rows() {
        let bundles = vec![TaskExportBundle {
            task: make_task("t1", "demo.zip", TaskStatus::Succeeded, Some("secret")),
            report: build_task_report(
                &make_task("t1", "demo.zip", TaskStatus::Succeeded, Some("secret")),
                &[],
                &default_export_options(),
            ),
            audit_events: vec![],
        }];
        let csv = bundles_to_csv(&bundles);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2); // 1 标题 + 1 数据行
        assert_eq!(
            lines[0],
            "id,file_name,file_path,file_size,archive_type,status,created_at,updated_at,found_password,error_message,recovery_parameters,audit_event_count,latest_audit_at,report_summary"
        );
        assert!(lines[1].starts_with("t1,demo.zip,/tmp/demo.zip,1024,zip,succeeded,"));
        assert!(lines[1].contains(",secret,,,0,,"));
    }

    /// 验证含特殊字符的字段被正确转义
    #[test]
    fn csv_escapes_special_chars() {
        let mut task = make_task(
            "t2",
            "has,comma.zip",
            TaskStatus::Succeeded,
            Some("pass\"word"),
        );
        task.file_path = "/path/with,comma".to_string();
        let report = TaskExportReport {
            status: "succeeded".to_string(),
            found_password: task.found_password.clone(),
            recovery_parameters: Some("恢复参数摘要".to_string()),
            failure_reason: None,
            audit_event_count: 1,
            latest_audit_at: None,
            summary: "summary,with\"quotes".to_string(),
        };
        let csv = bundles_to_csv(&[TaskExportBundle {
            task,
            report,
            audit_events: vec![],
        }]);
        let lines: Vec<&str> = csv.lines().collect();
        assert!(lines[1].contains("\"has,comma.zip\"")); // 含逗号的字段被引号包裹
        assert!(lines[1].contains("\"pass\"\"word\"")); // 引号被加倍转义
        assert!(lines[1].contains("\"summary,with\"\"quotes\"")); // 同上
    }

    /// 验证无数据时 CSV 只包含标题行
    #[test]
    fn csv_empty_tasks() {
        let csv = bundles_to_csv(&[]);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("id,"));
    }

    /// 验证 JSON 输出是合法 JSON，且结构符合预期
    #[test]
    fn json_output_is_valid() {
        let task = make_task("t1", "demo.zip", TaskStatus::Succeeded, Some("secret"));
        let event = make_event("e1", "t1", "密码恢复成功");
        let bundles = vec![TaskExportBundle {
            report: build_task_report(
                &task,
                std::slice::from_ref(&event),
                &default_export_options(),
            ),
            task,
            audit_events: vec![event],
        }];
        let json = bundles_to_json(&bundles).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["metadata"]["task_count"], 1);
        assert_eq!(parsed["tasks"][0]["task"]["id"], "t1");
        assert_eq!(parsed["tasks"][0]["report"]["found_password"], "secret");
        assert_eq!(parsed["tasks"][0]["audit_events"][0]["id"], "e1");
    }

    /// 验证空数组导出的 JSON 结构正确
    #[test]
    fn json_empty_tasks() {
        let json = bundles_to_json(&[]).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["metadata"]["task_count"], 0);
        assert_eq!(parsed["tasks"].as_array().unwrap().len(), 0);
    }

    /// 验证 build_task_report 正确统计审计事件数并提取密码
    #[test]
    fn build_task_report_summarizes_audit_and_password() {
        let task = make_task("t1", "demo.zip", TaskStatus::Succeeded, Some("secret"));
        let event = make_event("e1", "t1", "密码恢复成功");
        let report = build_task_report(&task, &[event], &default_export_options());
        assert_eq!(report.audit_event_count, 1);
        assert_eq!(report.found_password.as_deref(), Some("secret"));
        assert!(report.summary.contains("demo.zip"));
        assert!(report.summary.contains("已恢复密码"));
    }

    #[test]
    fn build_task_report_masks_password_when_requested() {
        let task = make_task("t1", "demo.zip", TaskStatus::Succeeded, Some("secret"));
        let options = ExportOptions {
            mask_passwords: true,
            include_audit_events: true,
        };

        let report = build_task_report(&task, &[], &options);

        assert_eq!(report.found_password.as_deref(), Some("••••••"));
        assert!(report.summary.contains("已导出脱敏密码"));
    }

    #[test]
    fn build_export_bundles_can_omit_audit_payloads() {
        let task = make_task("t1", "demo.zip", TaskStatus::Succeeded, Some("secret"));
        let event = AuditEvent {
            id: "e1".to_string(),
            event_type: AuditEventType::RecoveryStarted,
            task_id: Some("t1".to_string()),
            description: "调度启动密码恢复: demo.zip".to_string(),
            timestamp: Utc::now(),
        };
        let options = ExportOptions {
            mask_passwords: false,
            include_audit_events: false,
        };

        let bundles = build_export_bundles(vec![task], vec![vec![event]], &options);

        assert_eq!(bundles.len(), 1);
        assert!(bundles[0].audit_events.is_empty());
        assert!(bundles[0]
            .report
            .recovery_parameters
            .as_deref()
            .unwrap_or_default()
            .contains("调度启动密码恢复"));
    }

    /// 验证无特殊字符的字段不被转义
    #[test]
    fn escape_csv_field_no_special_chars() {
        assert_eq!(escape_csv_field("hello"), "hello");
    }

    /// 含逗号的字段应被双引号包裹
    #[test]
    fn escape_csv_field_with_comma() {
        assert_eq!(escape_csv_field("a,b"), "\"a,b\"");
    }

    /// 含双引号的字段：外层加引号，内部引号加倍
    #[test]
    fn escape_csv_field_with_quotes() {
        assert_eq!(escape_csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    /// 含换行符的字段应被双引号包裹（保留换行符本身）
    #[test]
    fn escape_csv_field_with_newline() {
        assert_eq!(escape_csv_field("line1\nline2"), "\"line1\nline2\"");
    }
}
