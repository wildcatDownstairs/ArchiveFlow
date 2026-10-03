use super::*;

impl ArchiveFlow {
    pub(super) fn save_preferences(&mut self, cx: &mut Context<Self>) -> bool {
        let result = (|| -> anyhow::Result<Settings> {
            let mut settings = self.settings.clone();
            settings.hashcat_path = self.hashcat.read(cx).value().trim().into();
            settings.charset = self.default_charset.read(cx).value().into();
            settings.min_length = self.default_min.read(cx).value().parse().map_err(|_| {
                anyhow::anyhow!(self.t("最小长度必须是整数", "Minimum length must be an integer"))
            })?;
            settings.max_length = self.default_max.read(cx).value().parse().map_err(|_| {
                anyhow::anyhow!(self.t("最大长度必须是整数", "Maximum length must be an integer"))
            })?;
            settings.priority = self
                .default_priority
                .read(cx)
                .value()
                .parse()
                .map_err(|_| {
                    anyhow::anyhow!(self.t("优先级必须是整数", "Priority must be an integer"))
                })?;
            settings.concurrency = self.concurrency.read(cx).value().parse().map_err(|_| {
                anyhow::anyhow!(self.t(
                    "并发数量必须是 1–16 的整数",
                    "Concurrent tasks must be an integer from 1 to 16"
                ))
            })?;
            settings.dictionary_draft = if settings.clear_dictionary {
                String::new()
            } else {
                self.dictionary.read(cx).value().into()
            };
            settings.validate()?;
            Ok(settings)
        })();
        match result {
            Ok(settings) => {
                self.send(Request::Save(settings), cx);
                self.error.is_none()
            }
            Err(error) => {
                self.error = Some(format!(
                    "{}: {error}",
                    self.t("设置未保存", "Settings not saved")
                ));
                cx.notify();
                false
            }
        }
    }

    fn toggle_setting(
        &self,
        id: &'static str,
        title: &'static str,
        description: &'static str,
        value: bool,
        setter: fn(&mut Settings, bool),
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .py_2()
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(title)
                    .child(muted(description, cx)),
            )
            .child(
                gpui_kit::component::switch::Switch::new(id)
                    .accessibility_label(title)
                    .checked(value)
                    .on_change(cx.listener(move |this, value, _, cx| {
                        setter(&mut this.settings, *value);
                        this.settings_dirty = true;
                        cx.notify();
                    })),
            )
    }

    pub(super) fn settings_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let tabs = [
            (IconName::Sun, self.t("外观与语言", "Appearance")),
            (IconName::SlidersHorizontal, self.t("恢复参数", "Recovery")),
            (
                IconName::ShieldCheck,
                self.t("结果与导出", "Results & exports"),
            ),
            (IconName::FolderOpen, self.t("本地数据", "Local data")),
        ];
        let content = match self.settings_tab {
            0 => self.appearance_settings(cx),
            1 => self.recovery_settings(cx),
            2 => self.export_settings(cx),
            _ => self.data_settings(cx),
        };
        div()
            .flex()
            .flex_col()
            .gap_5()
            .max_w(px(1040.))
            .mx_auto()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(heading(
                        self.t("偏好设置", "Preferences"),
                        self.t(
                            "为你的工作方式，留一点自己的习惯。",
                            "A workspace that fits the way you work.",
                        ),
                        cx,
                    ))
                    .child(
                        Button::new("save-settings")
                            .primary()
                            .h(px(38.))
                            .icon(IconName::Save)
                            .label(if self.settings_dirty {
                                self.t("保存更改", "Save changes")
                            } else {
                                self.t("已保存", "Saved")
                            })
                            .disabled(self.busy || !self.settings_dirty)
                            .tooltip(self.t("保存更改 · Ctrl/⌘ S", "Save changes · Ctrl/⌘ S"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.save_preferences(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .children(tabs.into_iter().enumerate().map(|(index, (icon, title))| {
                        Button::new(("settings-tab", index))
                            .ghost()
                            .h(px(42.))
                            .px_4()
                            .rounded_none()
                            .icon(icon)
                            .label(title)
                            .selected(self.settings_tab == index)
                            .border_b_2()
                            .border_color(if self.settings_tab == index {
                                cx.theme().primary
                            } else {
                                cx.theme().border.opacity(0.)
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.settings_tab = index;
                                cx.notify();
                            }))
                    })),
            )
            .when(self.settings_dirty, |view| {
                view.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(rgb(0xb38733))
                        .child(div().size(px(6.)).rounded_full().bg(rgb(0xd6ad53)))
                        .child(self.t(
                            "有未保存的更改。切换页面不会丢失当前编辑。",
                            "Unsaved changes. Your edits stay here while you browse.",
                        )),
                )
            })
            .child(content)
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "ArchiveFlow {}  ·  {}",
                        env!("CARGO_PKG_VERSION"),
                        self.t("偏好保存在本机", "Preferences stay on this device")
                    )),
            )
            .into_any_element()
    }

    fn appearance_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        panel(cx)
            .gap_6()
            .child(heading(
                self.t("熟悉的外观，顺手的语言", "Make it feel like home"),
                self.t(
                    "外观切换即时保存，界面语言在保存更改后保留。",
                    "Appearance saves immediately. Save changes to keep your language choice.",
                ),
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(muted(self.t("外观", "Appearance"), cx))
                    .child(
                        div().flex().gap_3().children(
                            [
                                (false, IconName::Sun, self.t("浅色 · 纸白", "Light · Paper")),
                                (true, IconName::Moon, self.t("深色 · 墨蓝", "Dark · Ink")),
                            ]
                            .into_iter()
                            .map(|(dark, icon, title)| {
                                Button::new(if dark {
                                    "appearance-dark"
                                } else {
                                    "appearance-light"
                                })
                                .h(px(56.))
                                .px_5()
                                .icon(icon)
                                .label(title)
                                .selected(self.settings.dark == dark)
                                .disabled(self.busy)
                                .on_click(cx.listener(
                                    move |this, _, window, cx| this.change_theme(dark, window, cx),
                                ))
                            }),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .pt_4()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(muted(self.t("界面语言", "Language"), cx))
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(
                                Button::new("locale-zh")
                                    .h(px(38.))
                                    .label("简体中文")
                                    .selected(!self.settings.english)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.change_language(false, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("locale-en")
                                    .h(px(38.))
                                    .label("English")
                                    .selected(self.settings.english)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.change_language(true, window, cx)
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_6()
                    .pt_4()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .children(
                        [
                            ("Ctrl / ⌘ O", self.t("导入归档", "Import archive")),
                            ("Ctrl / ⌘ F", self.t("搜索任务与记录", "Search")),
                            ("Ctrl / ⌘ S", self.t("保存设置", "Save preferences")),
                        ]
                        .into_iter()
                        .map(|(key, label)| {
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(div().text_xs().text_color(cx.theme().primary).child(key))
                                .child(muted(label, cx))
                        }),
                    ),
            )
            .into_any_element()
    }

    fn recovery_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let detection =
            self.detection.as_ref().map(|status| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded_md()
                    .bg(cx.theme().secondary)
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(if status.available {
                                self.t("GPU 已就绪", "GPU ready")
                            } else {
                                self.t("GPU 不可用", "GPU unavailable")
                            }),
                    )
                    .child(muted(
                        format!(
                            "{} {}",
                            status.version.clone().unwrap_or_default(),
                            status.path.clone().unwrap_or_default()
                        ),
                        cx,
                    ))
                    .children(status.devices.iter().map(|device| {
                        muted(format!("{} · {}", device.name, device.device_type), cx)
                    }))
                    .when_some(status.error.clone(), |view, error| {
                        view.child(muted(error, cx))
                    })
            });
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                panel(cx)
                    .child(section_heading(
                        IconName::SlidersHorizontal,
                        self.t("默认恢复参数", "Recovery defaults"),
                        cx,
                    ))
                    .child(field(
                        self.t("默认字符集", "Default character set"),
                        &self.default_charset,
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .gap_4()
                            .child(field(
                                self.t("最小长度", "Minimum length"),
                                &self.default_min,
                                cx,
                            ))
                            .child(field(
                                self.t("最大长度", "Maximum length"),
                                &self.default_max,
                                cx,
                            ))
                            .child(field(
                                self.t("任务优先级", "Task priority"),
                                &self.default_priority,
                                cx,
                            ))
                            .child(field(
                                self.t("最大并发数", "Concurrent tasks"),
                                &self.concurrency,
                                cx,
                            )),
                    )
                    .child(self.toggle_setting(
                        "filename-default",
                        self.t("加入文件名模式", "Include filename patterns"),
                        self.t(
                            "将文件名中的词语加入默认候选集。",
                            "Use words from the archive name as additional candidates.",
                        ),
                        self.settings.filename_patterns,
                        |s, v| s.filename_patterns = v,
                        cx,
                    ))
                    .child(self.toggle_setting(
                        "clear-dictionary",
                        self.t("提交后清空字典输入", "Clear dictionary after starting"),
                        self.t(
                            "启用后不保留字典输入草稿。",
                            "Do not retain a dictionary draft when enabled.",
                        ),
                        self.settings.clear_dictionary,
                        |s, v| s.clear_dictionary = v,
                        cx,
                    ))
                    .child(muted(
                        self.t(
                            "参数与并发上限在保存后生效。",
                            "Defaults and concurrency take effect when saved.",
                        ),
                        cx,
                    )),
            )
            .child(
                panel(cx)
                    .child(section_heading(
                        IconName::Cpu,
                        self.t("GPU 加速", "GPU acceleration"),
                        cx,
                    ))
                    .child(muted(
                        self.t(
                            "使用本机 hashcat。路径留空时自动检测。",
                            "Use local hashcat. Leave the path empty for automatic detection.",
                        ),
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .gap_3()
                            .child(field("hashcat", &self.hashcat, cx))
                            .child(
                                Button::new("detect-hashcat")
                                    .icon(IconName::Cpu)
                                    .label(self.t("检测设备", "Detect devices"))
                                    .disabled(self.busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let path = this.hashcat.read(cx).value().to_string();
                                        this.send(Request::Detect(path), cx);
                                    })),
                            ),
                    )
                    .when_some(detection, |view, result| view.child(result)),
            )
            .into_any_element()
    }

    fn export_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        panel(cx)
            .child(section_heading(
                IconName::ShieldCheck,
                self.t("结果与导出", "Results & exports"),
                cx,
            ))
            .child(self.toggle_setting(
                "mask-results",
                self.t("隐藏界面中的密码", "Mask displayed passwords"),
                self.t(
                    "恢复结果仍保存在本地，可在详情中临时显示。",
                    "Results remain on this device and can be revealed in task details.",
                ),
                self.settings.mask_results,
                |s, v| s.mask_results = v,
                cx,
            ))
            .child(self.toggle_setting(
                "mask-exports",
                self.t("导出时脱敏密码", "Mask passwords in exports"),
                self.t(
                    "CSV 与 JSON 中的密码将替换为掩码。",
                    "Replace passwords with a mask in CSV and JSON.",
                ),
                self.settings.mask_exports,
                |s, v| s.mask_exports = v,
                cx,
            ))
            .child(self.toggle_setting(
                "audit-exports",
                self.t("导出包含操作记录", "Include activity in exports"),
                self.t(
                    "附带任务的完整审计记录。",
                    "Include the task's full audit trail.",
                ),
                self.settings.export_audit,
                |s, v| s.export_audit = v,
                cx,
            ))
            .into_any_element()
    }

    fn data_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        div().flex().flex_col().gap_4()
            .child(panel(cx).child(section_heading(IconName::FolderOpen, self.t("本地数据目录", "Local data folder"), cx))
                .child(muted(self.data_dir.to_string_lossy().to_string(), cx))
                .child(div().child(Button::new("open-data").icon(IconName::FolderOpen).label(self.t("打开数据目录", "Open data folder"))
                    .on_click(cx.listener(|this, _, _, cx| this.send(Request::OpenData, cx))))))
            .child(panel(cx).border_color(cx.theme().danger.opacity(0.3))
                .child(section_heading(IconName::Trash, self.t("记录清理", "Clear records"), cx))
                .child(muted(self.t("清理应用记录与断点，原始压缩包会保留。操作需要再次确认。", "Clear application records and checkpoints. Source archives stay intact. Confirmation is required."), cx))
                .child(div().flex().gap_3()
                    .child(Button::new("clear-tasks").danger().label(self.t("清空任务", "Clear tasks")).disabled(self.busy)
                        .on_click(cx.listener(|this, _, window, cx| this.confirm_action(Confirmation::ClearTasks, window, cx))))
                    .child(Button::new("clear-audit").label(self.t("清空操作记录", "Clear activity")).disabled(self.busy)
                        .on_click(cx.listener(|this, _, window, cx| this.confirm_action(Confirmation::ClearAudit, window, cx))))))
            .into_any_element()
    }
}
