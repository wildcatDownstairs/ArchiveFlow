use super::*;
use archiveflow_core::domain::{
    recovery::{RecoveryBackend, ScheduledRecoveryState},
    task::{ArchiveType, TaskStatus},
};

impl ArchiveFlow {
    pub(super) fn detail(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(task) = self.current_task() else {
            return self.empty(
                self.t("任务不存在", "Task unavailable"),
                self.t(
                    "返回任务列表选择一个归档。",
                    "Select an archive from the task list.",
                ),
                cx,
            );
        };
        let export_id = task.id.clone();
        let archive_path = PathBuf::from(&task.file_path);
        let wide = window.viewport_size().width > px(1180.);
        div()
            .flex()
            .flex_col()
            .gap_5()
            .max_w(px(1400.))
            .mx_auto()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        Button::new("back-tasks")
                            .ghost()
                            .icon(IconName::ArrowLeft)
                            .label(self.t("返回任务", "Back to tasks"))
                            .on_click(cx.listener(|this, _, _, cx| this.navigate(Page::Tasks, cx))),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("open-archive-folder")
                                    .ghost()
                                    .icon(IconName::FolderOpen)
                                    .label(self.t("所在文件夹", "Show folder"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.send(
                                            Request::OpenArchiveFolder(archive_path.clone()),
                                            cx,
                                        )
                                    })),
                            )
                            .children(["json", "csv"].into_iter().map(|format| {
                                let id = export_id.clone();
                                Button::new(format)
                                    .icon(IconName::Download)
                                    .label(format.to_uppercase())
                                    .disabled(self.busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.export(vec![id.clone()], format, cx)
                                    }))
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .size(px(52.))
                            .flex_shrink_0()
                            .rounded_xl()
                            .bg(cx.theme().primary.opacity(0.12))
                            .border_1()
                            .border_color(cx.theme().primary.opacity(0.22))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(IconName::FileArchive)
                                    .size(px(27.))
                                    .text_color(cx.theme().primary),
                            ),
                    )
                    .child(heading(task.file_name.clone(), task.file_path.clone(), cx)),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .flex_wrap()
                    .child(muted(
                        format!(
                            "{} · {} · {}",
                            model::size(task.file_size),
                            model::archive_type_label(&task.archive_type),
                            model::status_label(&task.status, self.settings.english)
                        ),
                        cx,
                    ))
                    .when_some(task.archive_info.as_ref(), |view, info| {
                        view.child(muted(
                            format!(
                                "{} {} · {}",
                                info.total_entries,
                                self.t("个条目", "entries"),
                                if info.is_encrypted {
                                    self.t("已加密", "Encrypted")
                                } else {
                                    self.t("未加密", "Unencrypted")
                                }
                            ),
                            cx,
                        ))
                    }),
            )
            .when_some(task.error_message.clone(), |view, error| {
                view.child(panel(cx).child(muted(error, cx)))
            })
            .child(
                div()
                    .flex()
                    .gap_5()
                    .when(!wide, |view| view.flex_col())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.recovery_panel(&task, cx)),
                    )
                    .child(
                        div()
                            .when(wide, |view| view.w(px(380.)))
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_5()
                            .child(self.file_panel(&task, cx))
                            .child(self.task_history(&task, cx)),
                    ),
            )
            .into_any_element()
    }

    fn recovery_panel(&self, task: &ArchiveTask, cx: &mut Context<Self>) -> AnyElement {
        let mask_error = if self.attack == 2 {
            archiveflow_core::services::recovery_service::validate_mask(&self.mask.read(cx).value())
                .err()
        } else {
            None
        };
        let scheduled = self
            .snapshot
            .as_ref()
            .and_then(|s| s.scheduler.tasks.iter().find(|s| s.task_id == task.id));
        let checkpoint = self
            .snapshot
            .as_ref()
            .and_then(|s| s.checkpoints.get(&task.id));
        let progress = self
            .snapshot
            .as_ref()
            .and_then(|s| s.progress.get(&task.id));
        let running = task.status == TaskStatus::Processing;
        let queued = scheduled.is_some_and(|s| s.state == ScheduledRecoveryState::Queued);
        let paused = scheduled.is_some_and(|s| s.state == ScheduledRecoveryState::Paused);
        let can_start = !running
            && !queued
            && !paused
            && matches!(
                task.status,
                TaskStatus::Ready
                    | TaskStatus::Failed
                    | TaskStatus::Exhausted
                    | TaskStatus::Cancelled
                    | TaskStatus::Interrupted
            )
            && task.archive_info.as_ref().is_some_and(|a| a.is_encrypted);
        let gpu_available = cfg!(windows) && task.archive_type == ArchiveType::Zip;
        let id_pause = task.id.clone();
        let id_cancel = task.id.clone();
        let id_resume = task.id.clone();
        let mut body = panel(cx).child(section_heading(
            IconName::KeyRound,
            self.t("密码恢复", "Password recovery"),
            cx,
        ));
        if let Some(password) = &task.found_password {
            let value = password.clone();
            body = body.child(
                div()
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x258567).opacity(if self.settings.dark { 0.15 } else { 0.07 }))
                    .border_1()
                    .border_color(rgb(0x258567).opacity(0.25))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_color(rgb(if self.settings.dark {
                                0x71cbae
                            } else {
                                0x258567
                            }))
                            .child(Icon::new(IconName::Check).size(px(18.)))
                            .child(self.t("恢复成功 · 密码已保存", "Recovered · Password saved")),
                    )
                    .child(
                        div()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_xl()
                            .child(if !self.reveal {
                                "••••••••".into()
                            } else {
                                password.clone()
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("reveal-password")
                                    .ghost()
                                    .icon(if self.reveal {
                                        IconName::EyeOff
                                    } else {
                                        IconName::Eye
                                    })
                                    .label(if self.reveal {
                                        self.t("隐藏密码", "Hide password")
                                    } else {
                                        self.t("显示密码", "Show password")
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.reveal = !this.reveal;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("copy-password")
                                    .primary()
                                    .icon(IconName::Copy)
                                    .label(self.t("复制密码", "Copy password"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            value.clone(),
                                        ));
                                        this.show_message(
                                            this.t("密码已复制", "Password copied").into(),
                                            cx,
                                        );
                                    })),
                            ),
                    ),
            );
            return body.child(muted(self.t("可以复制密码打开原始归档，或从页面右上方导出结果。", "Copy the password to open your archive, or export the result from the top of this page."), cx)).into_any_element();
        }
        if task
            .archive_info
            .as_ref()
            .is_some_and(|info| !info.is_encrypted)
        {
            return body.child(div().flex().items_center().gap_3().py_4()
                .child(Icon::new(IconName::ShieldCheck).size(px(30.)).text_color(cx.theme().primary))
                .child(heading(self.t("这份归档无需密码", "No password needed"), self.t("文件内容已可查看，直接打开原始归档即可使用。", "Its contents are available. Open the original archive to use the files."), cx)))
                .into_any_element();
        }
        if let Some(p) = progress {
            let percent = if p.total == 0 {
                0.
            } else {
                p.tried as f32 / p.total as f32 * 100.
            };
            let eta = if running && p.speed > 0. {
                format!("{:.0}s", p.total.saturating_sub(p.tried) as f64 / p.speed)
            } else {
                "—".into()
            };
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child(
                                div()
                                    .text_3xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().primary)
                                    .child(format!("{percent:.1}%")),
                            )
                            .child(muted(model::speed(p.speed), cx)),
                    )
                    .child(
                        Progress::new("recovery-progress")
                            .value(percent)
                            .loading(running && p.total == 0),
                    )
                    .child(muted(
                        format!(
                            "{} / {} · {} {:.1}s · ETA {}",
                            p.tried,
                            p.total,
                            self.t("已用时", "Elapsed"),
                            p.elapsed_seconds,
                            eta
                        ),
                        cx,
                    ))
                    .child(muted(
                        format!(
                            "{} {} · {} {}",
                            self.t("线程 / 设备", "Workers / devices"),
                            p.worker_count,
                            self.t("最近断点", "Checkpoint"),
                            p.last_checkpoint_at
                                .map(|d| d
                                    .with_timezone(&chrono::Local)
                                    .format("%H:%M:%S")
                                    .to_string())
                                .unwrap_or_else(|| "—".into())
                        ),
                        cx,
                    )),
            );
        }
        if running || queued || paused {
            body = body
                .child(muted(
                    if paused {
                        self.t(
                            "已暂停，可从断点继续。",
                            "Paused. Resume from the saved checkpoint.",
                        )
                    } else if queued {
                        self.t(
                            "已进入队列，等待空闲执行槽位。",
                            "Queued, waiting for an available worker slot.",
                        )
                    } else {
                        self.t(
                            "恢复正在后台运行，可以切换到其他页面。",
                            "Recovery is running in the background. You can browse other pages.",
                        )
                    },
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .when(
                            !paused && scheduled.is_some_and(|s| s.backend == RecoveryBackend::Cpu),
                            |view| {
                                view.child(
                                    Button::new("pause")
                                        .label(self.t("暂停", "Pause"))
                                        .icon(IconName::Pause)
                                        .disabled(self.busy)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.send(Request::Pause(id_pause.clone()), cx)
                                        })),
                                )
                            },
                        )
                        .when(paused && !running && checkpoint.is_some(), |view| {
                            view.child(
                                Button::new("resume-paused")
                                    .primary()
                                    .label(self.t("继续", "Resume"))
                                    .icon(IconName::Play)
                                    .disabled(self.busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.send(Request::Resume(id_resume.clone()), cx)
                                    })),
                            )
                        })
                        .child(
                            Button::new("cancel-recovery")
                                .label(self.t("取消恢复", "Cancel recovery"))
                                .disabled(self.busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.send(Request::Cancel(id_cancel.clone()), cx)
                                })),
                        ),
                );
            return body.into_any_element();
        } else if let Some(checkpoint) = checkpoint {
            let id = task.id.clone();
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(muted(
                        format!(
                            "{}: {} / {}",
                            self.t("已有断点", "Saved checkpoint"),
                            checkpoint.tried,
                            checkpoint.total
                        ),
                        cx,
                    ))
                    .child(
                        Button::new("resume-checkpoint")
                            .label(self.t("从断点继续", "Resume checkpoint"))
                            .icon(IconName::Play)
                            .disabled(self.busy || !can_start)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.send(Request::Resume(id.clone()), cx)
                            })),
                    ),
            );
        }
        if !can_start {
            return body.child(muted(self.t("当前归档无法开始恢复，请检查上方状态与文件信息。", "This archive cannot start recovery. Check its status and file information above."), cx)).into_any_element();
        }
        body = body.child(
            div()
                .flex()
                .gap_1()
                .p_1()
                .rounded_lg()
                .bg(cx.theme().secondary)
                .children(
                    [
                        (0, self.t("字典", "Dictionary")),
                        (1, self.t("暴力", "Brute force")),
                        (2, self.t("掩码", "Mask")),
                    ]
                    .into_iter()
                    .map(|(index, title)| {
                        Button::new(("attack", index))
                            .ghost()
                            .flex_1()
                            .h(px(36.))
                            .label(title)
                            .selected(self.attack == index)
                            .disabled(running || queued || paused)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.attack = index;
                                cx.notify();
                            }))
                    }),
                ),
        );
        body = body.child(muted(
            match self.attack {
                0 => self.t(
                    "有可能的密码？每行填写一个，或载入本地字典。",
                    "Have possible passwords? Enter one per line or load a local dictionary.",
                ),
                1 => self.t(
                    "知道密码的字符范围和长度时，可以限定搜索空间。",
                    "Narrow the search using the characters and lengths you know.",
                ),
                _ => self.t(
                    "用固定字符和占位符描述你记得的密码结构。",
                    "Describe the structure you remember with fixed characters and placeholders.",
                ),
            },
            cx,
        ));
        body = match self.attack {
            0 => body.child(Textarea::new(&self.dictionary).h(px(160.)).disabled(running || queued || paused))
                .child(Button::new("load-dictionary").icon(IconName::FileText).label(self.t("导入 UTF-8 字典", "Import UTF-8 dictionary")).disabled(self.busy || running || queued || paused)
                    .on_click(cx.listener(|this, _, _, cx| this.send(Request::Dictionary, cx))))
                .child(Button::new("toggle-rules").ghost().icon(if self.show_rules { IconName::ChevronDown } else { IconName::ChevronRight })
                    .label(format!("{} · {} {}", self.t("候选扩展规则", "Candidate rules"), self.rules.iter().filter(|v| **v).count(), self.t("项启用", "enabled")))
                    .on_click(cx.listener(|this, _, _, cx| { this.show_rules = !this.show_rules; cx.notify(); })))
                .when(self.show_rules, |view| view.child(div().flex().flex_wrap().gap_3().p_3().rounded_md().bg(cx.theme().secondary).children(model::RULES.into_iter().enumerate().map(|(index, (zh, en))| {
                    Checkbox::new(("rule", index)).label(self.t(zh, en)).checked(self.rules[index]).disabled(running || queued || paused || (index == 6 && !self.rules[8]))
                        .on_change(cx.listener(move |this, value, _, cx| {
                            this.rules[index] = *value;
                            if index == 8 && !value { this.rules[6] = false; }
                            cx.notify();
                        }))
                }))))
                .when(self.show_rules, |view| view.child(muted(self.t("组合分隔符需先启用“词语组合”。", "Enable Combine words to use separators."), cx)))
                .child(muted(self.t("自动去重，最多生成 50,000 个候选。", "Candidates are deduplicated and limited to 50,000."), cx)),
            1 => body.child(field(self.t("字符集", "Character set"), &self.charset, cx))
                .child(div().flex().gap_2().children([("lower", "a–z", "abcdefghijklmnopqrstuvwxyz"), ("upper", "A–Z", "ABCDEFGHIJKLMNOPQRSTUVWXYZ"), ("digits", "0–9", "0123456789"), ("special", "!@#", "!@#$%^&*()-_=+[]{};:,.?")].into_iter().map(|(id, title, chars)| {
                    Button::new(id).small().label(title).disabled(running || queued || paused).on_click(cx.listener(move |this, _, window, cx| {
                        let mut value = this.charset.read(cx).value().to_string();
                        if chars.chars().all(|c| value.contains(c)) { value.retain(|c| !chars.contains(c)); }
                        else { for c in chars.chars() { if !value.contains(c) { value.push(c); } } }
                        this.charset.update(cx, |state, cx| state.set_value(value, window, cx));
                    }))
                })))
                .child(div().flex().gap_4().child(field(self.t("最小长度", "Minimum length"), &self.min_length, cx)).child(field(self.t("最大长度", "Maximum length"), &self.max_length, cx))),
            _ => body.child(field(self.t("掩码模式", "Mask pattern"), &self.mask, cx))
                .when_some(mask_error.clone(), |view, error| view.child(div().text_sm().text_color(cx.theme().danger).child(error)))
                .child(muted(self.t("?l 小写 · ?u 大写 · ?d 数字 · ?s 符号 · ?a 全部 · ?? 问号", "?l lowercase · ?u uppercase · ?d digits · ?s symbols · ?a all · ?? literal ?"), cx)),
        };
        body.child(Button::new("toggle-advanced").ghost().icon(if self.show_advanced { IconName::ChevronDown } else { IconName::ChevronRight })
                .label(format!("{} · {}", self.t("高级选项", "Advanced options"), if self.gpu { "GPU" } else { "CPU" }))
                .on_click(cx.listener(|this, _, _, cx| { this.show_advanced = !this.show_advanced; cx.notify(); })))
            .when(self.show_advanced, |view| view.child(div().flex().gap_4().items_end()
                .child(field(self.t("优先级（数值越大越优先）", "Priority (higher runs first)"), &self.priority, cx))
                .child(Button::new("backend-cpu").label("CPU").icon(IconName::Cpu).selected(!self.gpu).disabled(running || queued || paused)
                    .on_click(cx.listener(|this, _, _, cx| { this.gpu = false; cx.notify(); })))
                .child(Button::new("backend-gpu").label("GPU").selected(self.gpu).disabled(!gpu_available || running || queued || paused)
                    .on_click(cx.listener(|this, _, _, cx| { this.gpu = true; cx.notify(); })))))
            .when(self.gpu, |view| view.child(muted(self.t("GPU 使用 hashcat，目前支持 Windows ZIP；支持取消，不支持暂停。", "GPU uses hashcat for ZIP on Windows. Cancellation is supported; pause is unavailable."), cx)))
            .child(Button::new("start-recovery").primary().h(px(42.)).icon(IconName::Play).label(self.t("开始恢复", "Start recovery")).w_full().disabled(self.busy || !can_start || mask_error.is_some())
                .on_click(cx.listener(|this, _, window, cx| this.start_recovery(window, cx))))
            .into_any_element()
    }

    fn start_recovery(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(task) = self.current_task() else {
            return;
        };
        let result = (|| -> anyhow::Result<Request> {
            let priority: i32 = self.priority.read(cx).value().parse()?;
            let input = match self.attack {
                0 => RecoveryInput::Dictionary {
                    text: self.dictionary.read(cx).value().to_string(),
                    filename: task.file_name.clone(),
                    rules: self.rules,
                },
                1 => {
                    let min: usize = self.min_length.read(cx).value().parse()?;
                    let max: usize = self.max_length.read(cx).value().parse()?;
                    anyhow::ensure!(
                        min > 0 && max >= min && max <= 32,
                        "长度范围必须在 1–32 以内 / Length must be 1–32"
                    );
                    let charset = self.charset.read(cx).value().to_string();
                    anyhow::ensure!(
                        !charset.is_empty(),
                        "字符集不能为空 / Character set is empty"
                    );
                    RecoveryInput::Config {
                        mode: "bruteforce".into(),
                        json: serde_json::json!({"charset": charset, "min_length": min, "max_length": max}).to_string(),
                    }
                }
                _ => {
                    let mask = self.mask.read(cx).value().to_string();
                    archiveflow_core::services::recovery_service::validate_mask(&mask)
                        .map_err(anyhow::Error::msg)?;
                    RecoveryInput::Config {
                        mode: "mask".into(),
                        json: serde_json::json!({"mask": mask}).to_string(),
                    }
                }
            };
            Ok(Request::Start {
                id: task.id,
                input,
                priority,
                gpu: self.gpu,
                hashcat: self.settings.hashcat_path.clone(),
            })
        })();
        match result {
            Ok(request) => {
                self.send(request, cx);
            }
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
            }
        }
    }

    fn file_panel(&mut self, task: &ArchiveTask, cx: &mut Context<Self>) -> AnyElement {
        fn flatten(
            node: &model::FileNode,
            depth: usize,
            expanded: &HashSet<String>,
            rows: &mut Vec<(model::FileNode, usize)>,
        ) {
            for child in node.children.values() {
                rows.push((
                    model::FileNode {
                        children: Default::default(),
                        ..child.clone()
                    },
                    depth,
                ));
                if child.directory && expanded.contains(&child.path) {
                    flatten(child, depth + 1, expanded, rows);
                }
            }
        }
        let mut rows = Vec::new();
        flatten(&self.tree, 0, &self.expanded, &mut rows);
        let pages = rows.len().div_ceil(80).max(1);
        let page = self.tree_page.min(pages - 1);
        self.tree_page = page;
        panel(cx).gap_3().child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(self.t("归档内容", "Archive contents")))
            .when(task.archive_info.as_ref().is_some_and(|a| a.has_encrypted_filenames), |view| view.child(muted(self.t("文件名已加密，恢复密码后才能读取目录。", "Filenames are encrypted; directory contents are unavailable until recovery."), cx)))
            .when(rows.is_empty(), |view| view.child(muted(self.t("没有可显示的文件条目。", "No entries available."), cx)))
            .children(rows.into_iter().skip(page * 80).take(80).map(|(node, depth)| {
                let path = node.path.clone(); let expanded = self.expanded.contains(&node.path);
                div().flex().items_center().gap_2().pl(px((depth.min(10) * 12) as f32))
                    .child(Button::new(SharedString::from(format!("tree-{path}"))).ghost().small().justify_start().child(div().flex_1()).flex_1().min_w_0()
                        .icon(if node.directory { if expanded { IconName::FolderOpen } else { IconName::Folder } } else { IconName::File })
                        .label(node.name).tooltip(node.path).on_click(cx.listener(move |this, _, _, cx| {
                            if node.directory { if !this.expanded.remove(&path) { this.expanded.insert(path.clone()); } cx.notify(); }
                        })))
                    .child(div().text_xs().text_color(cx.theme().muted_foreground).child(if node.directory { String::new() } else { model::size(node.size) }))
                    .when(node.encrypted, |view| view.child(Icon::new(IconName::Lock).small()))
            }))
            .when(pages > 1, |view| view.child(div().flex().items_center().gap_2().child(muted(format!("{}/{}", page+1, pages), cx))
                .child(Button::new("tree-prev").small().icon(IconName::ChevronLeft).disabled(page == 0).on_click(cx.listener(|this, _, _, cx| { this.tree_page = this.tree_page.saturating_sub(1); cx.notify(); })))
                .child(Button::new("tree-next").small().icon(IconName::ChevronRight).disabled(page+1 >= pages).on_click(cx.listener(|this, _, _, cx| { this.tree_page += 1; cx.notify(); })))))
            .into_any_element()
    }

    fn task_history(&self, task: &ArchiveTask, cx: &App) -> AnyElement {
        let history = self
            .snapshot
            .as_ref()
            .map(|s| {
                s.audit
                    .iter()
                    .filter(|e| e.task_id.as_deref() == Some(&task.id))
                    .take(8)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        panel(cx)
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.t("最近活动", "Recent activity")),
            )
            .children(history.into_iter().map(|event| {
                let (icon, label) =
                    super::pages::activity_label(event.event_type.as_str(), self.settings.english);
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .text_color(cx.theme().primary)
                            .child(Icon::new(icon).size(px(14.)))
                            .child(label),
                    )
                    .child(muted(
                        event
                            .timestamp
                            .with_timezone(&chrono::Local)
                            .format("%m-%d %H:%M:%S")
                            .to_string(),
                        cx,
                    ))
                    .child(event.description.clone())
            }))
            .into_any_element()
    }
}
