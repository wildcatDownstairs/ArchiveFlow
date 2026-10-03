use super::*;
use archiveflow_core::domain::{recovery::ScheduledRecoveryState, task::TaskStatus};

impl ArchiveFlow {
    pub(super) fn tasks(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let query = self.search.read(cx).value();
        let filtered = self
            .snapshot
            .as_ref()
            .map(|s| {
                s.tasks
                    .iter()
                    .filter(|t| model::matches_task(t, &query, self.task_filter))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let count = filtered.len();
        let export_ids = filtered
            .iter()
            .map(|task| task.id.clone())
            .collect::<Vec<_>>();
        let pages = count.div_ceil(30).max(1);
        let page = self.task_page.min(pages - 1);
        self.task_page = page;
        let filters = [
            ("all", self.t("全部", "All")),
            ("ready", self.t("就绪", "Ready")),
            ("processing", self.t("恢复中", "Running")),
            ("succeeded", self.t("成功", "Succeeded")),
            ("interrupted", self.t("已中断", "Interrupted")),
            ("failed", self.t("失败", "Failed")),
            ("cancelled", self.t("已取消", "Cancelled")),
            ("exhausted", self.t("已穷尽", "Exhausted")),
            ("unsupported", self.t("不支持", "Unsupported")),
        ];
        div()
            .flex()
            .flex_col()
            .gap_6()
            .max_w(px(1180.))
            .mx_auto()
            .child(heading(
                self.t("归档任务", "Your archives"),
                self.t(
                    "从导入到恢复，所有进展都在这里。",
                    "Follow every archive from import to recovery.",
                ),
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .items_center()
                    .child(
                        div().flex_1().child(
                            Input::new(&self.search)
                                .cleanable(true)
                                .aria_label(self.t("搜索文件名或路径", "Search names or paths"))
                                .prefix(Icon::new(IconName::Search)),
                        ),
                    )
                    .children(["json", "csv"].into_iter().map(|format| {
                        let ids = export_ids.clone();
                        Button::new(format)
                            .label(format!(
                                "{} {}",
                                self.t("导出", "Export"),
                                format.to_uppercase()
                            ))
                            .icon(IconName::Download)
                            .tooltip(self.t("导出当前筛选结果", "Export the filtered archives"))
                            .disabled(self.busy || count == 0)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.export(ids.clone(), format, cx)
                            }))
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(filters.into_iter().map(|(key, title)| {
                        let total = self.snapshot.as_ref().map_or(0, |s| {
                            s.tasks
                                .iter()
                                .filter(|task| model::matches_task(task, "", key))
                                .count()
                        });
                        Button::new(key)
                            .small()
                            .ghost()
                            .h(px(34.))
                            .rounded_none()
                            .border_b_2()
                            .border_color(if self.task_filter == key {
                                cx.theme().primary
                            } else {
                                cx.theme().border.opacity(0.)
                            })
                            .label(format!("{title}  {total}"))
                            .selected(self.task_filter == key)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.task_filter = key;
                                this.task_page = 0;
                                cx.notify();
                            }))
                    })),
            )
            .child(if filtered.is_empty() {
                self.empty(
                    self.t("没有匹配的任务", "No matching tasks"),
                    self.t(
                        "导入归档，或调整搜索与筛选条件。",
                        "Import an archive or change your search and filters.",
                    ),
                    cx,
                )
                .into_any_element()
            } else {
                self.task_list(filtered.into_iter().skip(page * 30).take(30), true, cx)
                    .into_any_element()
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(muted(
                        format!(
                            "{} {} · {} / {}",
                            count,
                            self.t("个归档", "archives"),
                            page + 1,
                            pages
                        ),
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("tasks-prev")
                                    .label(self.t("上一页", "Previous"))
                                    .disabled(page == 0)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.task_page = this.task_page.saturating_sub(1);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("tasks-next")
                                    .label(self.t("下一页", "Next"))
                                    .disabled(page + 1 >= pages)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.task_page += 1;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn task_list(
        &self,
        tasks: impl IntoIterator<Item = ArchiveTask>,
        controls: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .rounded_xl()
            .overflow_hidden()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().group_box)
            .shadow_sm()
            .children(
                tasks
                    .into_iter()
                    .enumerate()
                    .map(|(index, task)| self.task_row(task, controls, index > 0, cx)),
            )
    }

    fn task_row(
        &self,
        task: ArchiveTask,
        controls: bool,
        divider: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = task.file_name.clone();
        let id = task.id.clone();
        let select = task.clone();
        let schedule = self
            .snapshot
            .as_ref()
            .and_then(|s| s.scheduler.tasks.iter().find(|s| s.task_id == task.id));
        let label = match schedule.map(|s| &s.state) {
            Some(ScheduledRecoveryState::Queued) => self.t("排队中", "Queued"),
            Some(ScheduledRecoveryState::Paused) => self.t("已暂停", "Paused"),
            _ => model::status_label(&task.status, self.settings.english),
        };
        let status_color: Hsla = match schedule.map(|s| &s.state) {
            Some(ScheduledRecoveryState::Paused) => rgb(0xb68b40).into(),
            Some(ScheduledRecoveryState::Queued) => cx.theme().muted_foreground,
            _ => match task.status {
                TaskStatus::Succeeded => rgb(if self.settings.dark {
                    0x71cbae
                } else {
                    0x258567
                })
                .into(),
                TaskStatus::Processing => cx.theme().primary,
                TaskStatus::Interrupted => rgb(if self.settings.dark {
                    0xe2bb69
                } else {
                    0xa57521
                })
                .into(),
                TaskStatus::Failed => cx.theme().danger,
                _ => cx.theme().muted_foreground,
            },
        };
        let format = model::archive_type_label(&task.archive_type);
        let encrypted = task.archive_info.as_ref().is_some_and(|a| a.is_encrypted);
        div()
            .flex()
            .items_center()
            .when(divider, |row| {
                row.border_t_1()
                    .border_color(cx.theme().border.opacity(0.55))
            })
            .child(
                Button::new(SharedString::from(format!("open-{id}")))
                    .ghost()
                    .flex_1()
                    .min_w_0()
                    .h(px(68.))
                    .px_4()
                    .rounded_none()
                    .accessibility_label(title.clone())
                    .tooltip(task.file_path.clone())
                    .child(
                        div()
                            .flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .gap_4()
                            .child(
                                div()
                                    .size(px(40.))
                                    .flex_shrink_0()
                                    .rounded_lg()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(status_color.opacity(0.12))
                                    .child(
                                        Icon::new(IconName::FileArchive)
                                            .size(px(20.))
                                            .text_color(status_color),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(cx.theme().foreground)
                                            .text_ellipsis()
                                            .child(title),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .text_ellipsis()
                                            .child(format!(
                                                "{}  ·  {}  ·  {}  ·  {}",
                                                format,
                                                model::size(task.file_size),
                                                if encrypted {
                                                    self.t("已加密", "Encrypted")
                                                } else {
                                                    self.t("未加密", "Unencrypted")
                                                },
                                                task.created_at
                                                    .with_timezone(&chrono::Local)
                                                    .format("%m-%d %H:%M")
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_shrink_0()
                                    .items_center()
                                    .gap_2()
                                    .px_2p5()
                                    .py_1()
                                    .rounded_full()
                                    .bg(status_color.opacity(0.1))
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(status_color)
                                    .child(div().size(px(6.)).rounded_full().bg(status_color))
                                    .child(label),
                            )
                            .when(!controls, |row| {
                                row.child(
                                    Icon::new(IconName::ChevronRight)
                                        .size(px(14.))
                                        .text_color(cx.theme().muted_foreground.opacity(0.65)),
                                )
                            }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.select_task(&select, cx))),
            )
            .when(controls, |row| {
                row.child(
                    Button::new(SharedString::from(format!("delete-{id}")))
                        .ghost()
                        .small()
                        .mx_3()
                        .text_color(cx.theme().muted_foreground)
                        .icon(IconName::Trash)
                        .disabled(
                            self.busy
                                || task.status == TaskStatus::Processing
                                || schedule.is_some(),
                        )
                        .tooltip(self.t("删除任务", "Delete task"))
                        .accessibility_label(self.t("删除任务", "Delete task"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.confirm_action(Confirmation::Delete(id.clone()), window, cx);
                        })),
                )
            })
            .into_any_element()
    }

    pub(super) fn audit(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let query = self.audit_search.read(cx).value().to_lowercase();
        let events = self
            .snapshot
            .as_ref()
            .map(|s| {
                s.audit
                    .iter()
                    .filter(|e| {
                        (self.audit_filter == "all"
                            || e.event_type.as_str().starts_with(self.audit_filter))
                            && (query.is_empty()
                                || e.description.to_lowercase().contains(&query)
                                || e.event_type.as_str().contains(&query))
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let pages = events.len().div_ceil(40).max(1);
        let page = self.audit_page.min(pages - 1);
        self.audit_page = page;
        let total = self.snapshot.as_ref().map_or(0, |s| s.audit_count);
        div()
            .flex()
            .flex_col()
            .gap_6()
            .max_w(px(1180.))
            .mx_auto()
            .child(heading(
                self.t("操作记录", "Activity history"),
                self.t(
                    "每一次导入、恢复和设置变更，都有迹可循。",
                    "A local record of imports, recovery and settings changes.",
                ),
                cx,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div().flex_1().child(
                            Input::new(&self.audit_search)
                                .cleanable(true)
                                .prefix(Icon::new(IconName::Search)),
                        ),
                    )
                    .children(
                        [
                            ("all", self.t("全部", "All")),
                            ("file_", self.t("导入", "Imports")),
                            ("recovery_", self.t("恢复", "Recovery")),
                            ("setting_", self.t("设置", "Settings")),
                            ("task", self.t("任务", "Tasks")),
                        ]
                        .into_iter()
                        .map(|(key, label)| {
                            Button::new(key)
                                .ghost()
                                .label(label)
                                .selected(self.audit_filter == key)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.audit_filter = key;
                                    this.audit_page = 0;
                                    cx.notify();
                                }))
                        }),
                    ),
            )
            .child(muted(
                format!(
                    "{} {total} · {}",
                    self.t("累计记录", "Total events"),
                    self.t(
                        "浏览最近 1,000 条；导出可包含完整任务审计",
                        "Browsing the latest 1,000; exports include full task history"
                    )
                ),
                cx,
            ))
            .child(if events.is_empty() {
                self.empty(
                    self.t("还没有匹配的记录", "No matching activity"),
                    self.t(
                        "任务操作会自动记录在这里。",
                        "Your task activity will appear here.",
                    ),
                    cx,
                )
            } else {
                panel(cx)
                    .p_0()
                    .gap_0()
                    .children(events.into_iter().skip(page * 40).take(40).map(|event| {
                        let (icon, label) =
                            activity_label(event.event_type.as_str(), self.settings.english);
                        div()
                            .p_4()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .flex()
                            .gap_4()
                            .child(
                                div()
                                    .size(px(34.))
                                    .flex_shrink_0()
                                    .rounded_lg()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(cx.theme().primary.opacity(0.1))
                                    .text_color(cx.theme().primary)
                                    .child(Icon::new(icon).size(px(17.))),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(div().font_weight(FontWeight::MEDIUM).child(label))
                                    .child(muted(event.description, cx)),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(
                                        event
                                            .timestamp
                                            .with_timezone(&chrono::Local)
                                            .format("%m-%d %H:%M:%S")
                                            .to_string(),
                                    ),
                            )
                    }))
                    .into_any_element()
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .items_center()
                    .gap_3()
                    .child(muted(format!("{}/{}", page + 1, pages), cx))
                    .child(
                        Button::new("audit-prev")
                            .label(self.t("上一页", "Previous"))
                            .disabled(page == 0)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.audit_page = this.audit_page.saturating_sub(1);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("audit-next")
                            .label(self.t("下一页", "Next"))
                            .disabled(page + 1 >= pages)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.audit_page += 1;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn empty(&self, title: &str, description: &str, cx: &App) -> AnyElement {
        panel(cx)
            .py_10()
            .items_center()
            .gap_3()
            .child(
                div()
                    .size(px(56.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(cx.theme().secondary.opacity(0.7))
                    .child(
                        Icon::new(IconName::Inbox)
                            .size(px(26.))
                            .text_color(cx.theme().muted_foreground),
                    ),
            )
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_owned()),
            )
            .child(muted(description.to_owned(), cx))
            .into_any_element()
    }
}

pub(super) fn activity_label(kind: &str, english: bool) -> (IconName, &'static str) {
    let (icon, zh, en) = match kind {
        "file_imported" => (IconName::FileArchive, "导入归档", "Archive imported"),
        "recovery_succeeded" => (IconName::Check, "恢复成功", "Recovery completed"),
        "recovery_started" | "recovery_resumed" => (IconName::Play, "开始恢复", "Recovery started"),
        "recovery_paused" | "task_interrupted" => (
            IconName::Pause,
            "任务已暂停或中断",
            "Task paused or interrupted",
        ),
        "recovery_queued" => (IconName::Clock, "任务已排队", "Task queued"),
        "recovery_failed" | "task_failed" => (IconName::Info, "任务出现错误", "Task error"),
        "recovery_exhausted" => (IconName::Search, "候选已尝试完毕", "Candidates exhausted"),
        "recovery_cancelled" => (IconName::X, "任务已取消", "Task cancelled"),
        "task_deleted" | "tasks_cleared" | "audit_logs_cleared" | "cache_cleared" => {
            (IconName::Trash, "清理记录", "Records cleared")
        }
        "setting_changed" => (IconName::Settings2, "设置已更新", "Preferences updated"),
        "result_exported" => (IconName::Download, "结果已导出", "Results exported"),
        "task_unsupported" => (IconName::Info, "不支持的归档", "Unsupported archive"),
        "authorization_granted" => (
            IconName::ShieldCheck,
            "授权已确认",
            "Authorization confirmed",
        ),
        _ => (IconName::Activity, "任务状态更新", "Task updated"),
    };
    (icon, text(english, zh, en))
}
