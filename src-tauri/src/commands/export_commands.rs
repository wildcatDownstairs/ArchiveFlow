use chrono::{DateTime, Utc};
use tauri::State;

use crate::db::Database;
use crate::domain::audit::{AuditEvent, AuditEventType};
use crate::domain::task::{Task, TaskStatus};
use crate::errors::AppError;
use crate::services::audit_service;

#[derive(Debug, Clone, serde::Serialize)]
struct ExportMetadata {
    exported_at: DateTime<Utc>,
    format_version: u32,
    task_count: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TaskExportReport {
    status: String,
    found_password: Option<String>,
    audit_event_count: usize,
    latest_audit_at: Option<DateTime<Utc>>,
    summary: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TaskExportBundle {
    task: Task,
    report: TaskExportReport,
    audit_events: Vec<AuditEvent>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TaskExportDocument {
    metadata: ExportMetadata,
    tasks: Vec<TaskExportBundle>,
}

/// CSV 字段转义：包含逗号、双引号或换行时用双引号包裹，内部双引号加倍
fn escape_csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

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

fn build_task_report(task: &Task, audit_events: &[AuditEvent]) -> TaskExportReport {
    let latest_audit_at = audit_events.first().map(|event| event.timestamp);
    let password_summary = match task.found_password.as_deref() {
        Some(password) => format!("已恢复密码 `{password}`"),
        None => "未导出明文密码".to_string(),
    };
    let archive_summary = task
        .archive_info
        .as_ref()
        .map(|info| {
            format!(
                "共 {} 个条目，其中 {} 个加密",
                info.total_entries,
                info.entries.iter().filter(|entry| entry.is_encrypted).count()
            )
        })
        .unwrap_or_else(|| "无归档检测详情".to_string());
    let latest_event_summary = latest_audit_at
        .map(|ts| format!("最近审计时间 {}", ts.to_rfc3339()))
        .unwrap_or_else(|| "无审计事件".to_string());

    TaskExportReport {
        status: task.status.as_str().to_string(),
        found_password: task.found_password.clone(),
        audit_event_count: audit_events.len(),
        latest_audit_at,
        summary: format!(
            "任务“{}”当前状态为{}；{}；{}；{}。",
            task.file_name,
            status_label(&task.status),
            password_summary,
            archive_summary,
            latest_event_summary
        ),
    }
}

fn build_export_bundles(tasks: Vec<Task>, audit_events_by_task: Vec<Vec<AuditEvent>>) -> Vec<TaskExportBundle> {
    tasks.into_iter()
        .zip(audit_events_by_task)
        .map(|(task, audit_events)| {
            let report = build_task_report(&task, &audit_events);
            TaskExportBundle {
                task,
                report,
                audit_events,
            }
        })
        .collect()
}

fn load_export_bundles(db: &Database, task_ids: &[String]) -> Result<Vec<TaskExportBundle>, AppError> {
    let tasks = if task_ids.is_empty() {
        db.get_all_tasks()?
    } else {
        let mut tasks = Vec::with_capacity(task_ids.len());
        for id in task_ids {
            if let Some(task) = db.get_task_by_id(id)? {
                tasks.push(task);
            }
        }
        tasks
    };

    let audit_events_by_task = tasks
        .iter()
        .map(|task| db.get_audit_events_for_task(&task.id))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(build_export_bundles(tasks, audit_events_by_task))
}

fn bundles_to_csv(bundles: &[TaskExportBundle]) -> String {
    let mut lines = Vec::with_capacity(bundles.len() + 1);
    lines.push(
        "id,file_name,file_path,file_size,archive_type,status,created_at,updated_at,found_password,audit_event_count,latest_audit_at,report_summary"
            .to_string(),
    );

    for bundle in bundles {
        let task = &bundle.task;
        let archive_type = serde_json::to_value(&task.archive_type)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        let row = format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}",
            escape_csv_field(&task.id),
            escape_csv_field(&task.file_name),
            escape_csv_field(&task.file_path),
            task.file_size,
            escape_csv_field(&archive_type),
            escape_csv_field(task.status.as_str()),
            escape_csv_field(&task.created_at.to_rfc3339()),
            escape_csv_field(&task.updated_at.to_rfc3339()),
            escape_csv_field(task.found_password.as_deref().unwrap_or("")),
            bundle.report.audit_event_count,
            escape_csv_field(
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

    lines.join("\n")
}

fn bundles_to_json(bundles: &[TaskExportBundle]) -> Result<String, AppError> {
    let document = TaskExportDocument {
        metadata: ExportMetadata {
            exported_at: Utc::now(),
            format_version: 1,
            task_count: bundles.len(),
        },
        tasks: bundles.to_vec(),
    };

    Ok(serde_json::to_string_pretty(&document)?)
}

/// 导出任务结果
#[tauri::command]
pub async fn export_tasks(
    db: State<'_, Database>,
    task_ids: Vec<String>,
    format: String,
) -> Result<String, AppError> {
    if format != "csv" && format != "json" {
        return Err(AppError::InvalidArgument(format!(
            "不支持的导出格式: {}，仅支持 csv 或 json",
            format
        )));
    }

    let bundles = load_export_bundles(&db, &task_ids)?;
    if !task_ids.is_empty() && bundles.is_empty() {
        return Err(AppError::InvalidArgument("未找到可导出的任务".to_string()));
    }

    let content = match format.as_str() {
        "csv" => bundles_to_csv(&bundles),
        "json" => bundles_to_json(&bundles)?,
        _ => unreachable!(),
    };

    let target_task_id = if bundles.len() == 1 {
        Some(bundles[0].task.id.clone())
    } else {
        None
    };

    let _ = audit_service::log_audit_event(
        &db,
        AuditEventType::ResultExported,
        target_task_id,
        format!(
            "导出 {} 条任务恢复结果 (格式: {}, 含报告与审计记录)",
            bundles.len(),
            format
        ),
    );

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::archive::ArchiveInfo;
    use crate::domain::task::ArchiveType;

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

    fn make_event(id: &str, task_id: &str, description: &str) -> AuditEvent {
        AuditEvent {
            id: id.to_string(),
            event_type: AuditEventType::RecoverySucceeded,
            task_id: Some(task_id.to_string()),
            description: description.to_string(),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn csv_header_and_rows() {
        let bundles = vec![TaskExportBundle {
            task: make_task("t1", "demo.zip", TaskStatus::Succeeded, Some("secret")),
            report: build_task_report(
                &make_task("t1", "demo.zip", TaskStatus::Succeeded, Some("secret")),
                &[],
            ),
            audit_events: vec![],
        }];
        let csv = bundles_to_csv(&bundles);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            "id,file_name,file_path,file_size,archive_type,status,created_at,updated_at,found_password,audit_event_count,latest_audit_at,report_summary"
        );
        assert!(lines[1].starts_with("t1,demo.zip,/tmp/demo.zip,1024,zip,succeeded,"));
        assert!(lines[1].contains(",secret,0,,"));
    }

    #[test]
    fn csv_escapes_special_chars() {
        let mut task = make_task("t2", "has,comma.zip", TaskStatus::Succeeded, Some("pass\"word"));
        task.file_path = "/path/with,comma".to_string();
        let report = TaskExportReport {
            status: "succeeded".to_string(),
            found_password: task.found_password.clone(),
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
        assert!(lines[1].contains("\"has,comma.zip\""));
        assert!(lines[1].contains("\"pass\"\"word\""));
        assert!(lines[1].contains("\"summary,with\"\"quotes\""));
    }

    #[test]
    fn csv_empty_tasks() {
        let csv = bundles_to_csv(&[]);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("id,"));
    }

    #[test]
    fn json_output_is_valid() {
        let task = make_task("t1", "demo.zip", TaskStatus::Succeeded, Some("secret"));
        let event = make_event("e1", "t1", "密码恢复成功");
        let bundles = vec![TaskExportBundle {
            report: build_task_report(&task, std::slice::from_ref(&event)),
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

    #[test]
    fn json_empty_tasks() {
        let json = bundles_to_json(&[]).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["metadata"]["task_count"], 0);
        assert_eq!(parsed["tasks"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn build_task_report_summarizes_audit_and_password() {
        let task = make_task("t1", "demo.zip", TaskStatus::Succeeded, Some("secret"));
        let event = make_event("e1", "t1", "密码恢复成功");
        let report = build_task_report(&task, &[event]);
        assert_eq!(report.audit_event_count, 1);
        assert_eq!(report.found_password.as_deref(), Some("secret"));
        assert!(report.summary.contains("demo.zip"));
        assert!(report.summary.contains("已恢复密码"));
    }

    #[test]
    fn escape_csv_field_no_special_chars() {
        assert_eq!(escape_csv_field("hello"), "hello");
    }

    #[test]
    fn escape_csv_field_with_comma() {
        assert_eq!(escape_csv_field("a,b"), "\"a,b\"");
    }

    #[test]
    fn escape_csv_field_with_quotes() {
        assert_eq!(escape_csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn escape_csv_field_with_newline() {
        assert_eq!(escape_csv_field("line1\nline2"), "\"line1\nline2\"");
    }
}
