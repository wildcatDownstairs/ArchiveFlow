//! 应用外壳：导航、快捷键、全局反馈与页面容器。
use super::*;
use gpui_kit::component::button::ButtonCustomVariant;

fn navigation_style(active: bool, cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(if active {
            rgb(0x203f4f).into()
        } else {
            cx.theme().transparent
        })
        .foreground(rgb(if active { 0x73d9e8 } else { 0xb9cad5 }).into())
        .hover(rgb(0x203845).into())
        .active(rgb(0x294b5c).into())
}

impl ArchiveFlow {
    pub(super) fn change_language(
        &mut self,
        english: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.english = english;
        self.settings_dirty = true;
        self.search.update(cx, |state, cx| {
            state.set_placeholder(
                text(english, "搜索文件名或路径…", "Search names or paths…"),
                window,
                cx,
            )
        });
        self.audit_search.update(cx, |state, cx| {
            state.set_placeholder(
                text(english, "搜索操作记录…", "Search activity…"),
                window,
                cx,
            )
        });
        self.dictionary.update(cx, |state, cx| {
            state.set_placeholder(
                text(english, "每行一个候选密码", "One password per line"),
                window,
                cx,
            )
        });
        cx.notify();
    }
    pub(super) fn show_message(&mut self, message: String, cx: &mut Context<Self>) {
        self.message = message;
        self.notice_timeout = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(4)).await;
            let _ = this.update(cx, |this, cx| {
                this.message.clear();
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn change_theme(&mut self, dark: bool, window: &mut Window, cx: &mut Context<Self>) {
        if dark == self.settings.dark || self.busy {
            return;
        }
        self.settings.dark = dark;
        apply_theme(dark, Some(window), cx);
        // 外观立即生效，只写入外观字段，不提交设置表单中尚未保存的内容。
        self.send(Request::SaveAppearance(dark), cx);
    }

    pub(super) fn show_tasks(
        &mut self,
        filter: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.task_filter = filter;
        self.task_page = 0;
        self.navigate(Page::Tasks, cx);
    }

    fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page == Page::Audit {
            self.audit_search
                .read(cx)
                .focus_handle(cx)
                .focus(window, cx);
        } else {
            self.navigate(Page::Tasks, cx);
            self.search.read(cx).focus_handle(cx).focus(window, cx);
        }
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = [
            (Page::Home, IconName::House, self.t("工作台", "Overview")),
            (
                Page::Tasks,
                IconName::ListTodo,
                self.t("归档任务", "Archives"),
            ),
            (
                Page::Audit,
                IconName::Activity,
                self.t("操作记录", "Activity"),
            ),
            (
                Page::Settings,
                IconName::Settings2,
                self.t("偏好设置", "Preferences"),
            ),
        ];
        div()
            .w(px(208.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .p_4()
            .gap_2()
            .bg(cx.theme().sidebar)
            .text_color(rgb(0xe5eef3))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .pt_5()
                    .pb_7()
                    .px_1()
                    .child(
                        div()
                            .size(px(40.))
                            .flex_shrink_0()
                            .rounded_xl()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(rgb(0x1c3f50))
                            .border_1()
                            .border_color(rgb(0x37c2da).opacity(0.35))
                            .child(
                                img(APP_ICON.clone())
                                    .size(px(28.))
                                    .object_fit(ObjectFit::Contain),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_base()
                                    .child("ArchiveFlow"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x8fa8b8))
                                    .child(self.t("归档 · 恢复 · 继续", "ARCHIVE WORKSPACE")),
                            ),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .mb_2()
                    .text_xs()
                    .text_color(rgb(0x7e9bac))
                    .child(self.t("工作空间", "WORKSPACE")),
            )
            .children(
                entries
                    .into_iter()
                    .enumerate()
                    .map(|(index, (page, icon, title))| {
                        let active =
                            self.page == page || (page == Page::Tasks && self.page == Page::Detail);
                        Button::new(("navigation", index))
                            .custom(navigation_style(active, cx))
                            .selected(active)
                            .w_full()
                            .h(px(42.))
                            .px_3()
                            .text_color(rgb(if active { 0x73d9e8 } else { 0xb9cad5 }))
                            .when(active, |button| {
                                button
                                    .bg(linear_gradient(
                                        90.,
                                        linear_color_stop(rgb(0x244b5d), 0.),
                                        linear_color_stop(rgb(0x1b3a49), 1.),
                                    ))
                                    .border_l_2()
                                    .border_color(rgb(0x37c2da))
                            })
                            .accessibility_label(title)
                            .child(
                                div()
                                    .flex()
                                    .w_full()
                                    .items_center()
                                    .gap_3()
                                    .child(Icon::new(icon).size(px(18.)))
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_sm()
                                            .when(active, |d| d.font_weight(FontWeight::MEDIUM))
                                            .child(title),
                                    )
                                    .when(page == Page::Tasks, |row| {
                                        row.child(
                                            div()
                                                .min_w(px(22.))
                                                .px_1p5()
                                                .py_0p5()
                                                .rounded_full()
                                                .bg(if active {
                                                    rgb(0x37c2da).opacity(0.22)
                                                } else {
                                                    rgb(0xffffff).opacity(0.08)
                                                })
                                                .text_xs()
                                                .text_center()
                                                .child(
                                                    self.snapshot
                                                        .as_ref()
                                                        .map_or(0, |s| s.tasks.len())
                                                        .to_string(),
                                                ),
                                        )
                                    })
                                    .when(page == Page::Settings && self.settings_dirty, |row| {
                                        row.child(
                                            div().size(px(6.)).rounded_full().bg(rgb(0xf2be4d)),
                                        )
                                    }),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| this.navigate(page, cx)))
                    }),
            )
            .child(div().flex_1())
            .child(
                div()
                    .px_3()
                    .py_4()
                    .mb_2()
                    .border_t_1()
                    .border_color(rgb(0x2c414f))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .text_color(rgb(0xa7becb))
                            .child(div().size(px(6.)).rounded_full().bg(rgb(0x6ac5a9)))
                            .child(self.t("本地工作空间", "Local workspace")),
                    )
                    .child(div().mt_2().text_xs().text_color(rgb(0x7893a5)).child(
                        self.t("文件与记录保存在这台设备", "Your files stay on this device"),
                    )),
            )
            .child(
                Button::new("theme")
                    .custom(navigation_style(false, cx))
                    .h(px(36.))
                    .text_color(rgb(0xb9cad5))
                    .icon(if self.settings.dark {
                        IconName::Sun
                    } else {
                        IconName::Moon
                    })
                    .label(if self.settings.dark {
                        self.t("浅色外观", "Light appearance")
                    } else {
                        self.t("深色外观", "Dark appearance")
                    })
                    .child(div().flex_1())
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.change_theme(!this.settings.dark, window, cx)
                    })),
            )
            .child(
                Button::new("logs")
                    .custom(navigation_style(false, cx))
                    .h(px(36.))
                    .text_color(rgb(0xb9cad5))
                    .icon(IconName::FolderOpen)
                    .label(self.t("打开日志", "Open logs"))
                    .child(div().flex_1())
                    .on_click(cx.listener(|this, _, _, cx| this.send(Request::OpenLogs, cx))),
            )
            .child(
                div()
                    .px_3()
                    .pt_3()
                    .text_xs()
                    .text_color(rgb(0x6f8a9d))
                    .child(format!("ArchiveFlow  /  {}", env!("CARGO_PKG_VERSION"))),
            )
    }
}

impl Render for ArchiveFlow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.closing.active() {
            return self.closing_page(cx);
        }
        let content = match self.page {
            Page::Home => self.home(cx),
            Page::Tasks => self.tasks(cx),
            Page::Detail => self.detail(window, cx),
            Page::Audit => self.audit(cx),
            Page::Settings => self.settings_page(cx),
        };
        let page_label = match self.page {
            Page::Home => self.t("工作台", "Overview"),
            Page::Tasks => self.t("归档任务", "Archives"),
            Page::Detail => self.t("任务详情", "Task details"),
            Page::Audit => self.t("操作记录", "Activity"),
            Page::Settings => self.t("偏好设置", "Preferences"),
        };
        let running = self
            .snapshot
            .as_ref()
            .map_or(0, |s| s.scheduler.running_count);
        div()
            .id("archiveflow")
            .key_context("ArchiveFlow")
            .track_focus(&self.shell_focus)
            .relative()
            .size_full()
            .flex()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .font_family(cx.theme().font_family.clone())
            .text_sm()
            .on_action(cx.listener(|this, _: &ImportArchive, _, cx| {
                if !this.busy {
                    this.send(Request::Import(None), cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FindTask, window, cx| this.focus_search(window, cx)))
            .on_action(cx.listener(|this, _: &SavePreferences, _, cx| {
                if this.page == Page::Settings && !this.busy && this.settings_dirty {
                    this.save_preferences(cx);
                }
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                if !this.busy {
                    this.send(Request::Import(Some(paths.paths().to_vec())), cx);
                }
            }))
            .child(self.sidebar(cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(52.))
                            .flex_shrink_0()
                            .px_6()
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .text_xs()
                                    .child(
                                        div()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("ArchiveFlow"),
                                    )
                                    .child(
                                        Icon::new(IconName::ChevronRight)
                                            .size(px(12.))
                                            .text_color(cx.theme().muted_foreground),
                                    )
                                    .child(page_label),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .when(self.busy || running > 0, |row| {
                                        row.child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_2()
                                                .px_2p5()
                                                .py_1()
                                                .rounded_full()
                                                .bg(cx.theme().primary.opacity(0.12))
                                                .border_1()
                                                .border_color(cx.theme().primary.opacity(0.25))
                                                .text_xs()
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(cx.theme().primary)
                                                .child(
                                                    div()
                                                        .size(px(6.))
                                                        .rounded_full()
                                                        .bg(cx.theme().primary),
                                                )
                                                .child(if self.busy {
                                                    self.t("正在处理…", "Working…").into()
                                                } else {
                                                    format!(
                                                        "{} · {running}",
                                                        self.t("恢复中", "Running")
                                                    )
                                                }),
                                        )
                                    })
                                    .child(
                                        Button::new("global-search")
                                            .ghost()
                                            .small()
                                            .icon(IconName::Search)
                                            .accessibility_label(self.t("搜索", "Search"))
                                            .tooltip(self.t(
                                                "搜索任务或记录 · Ctrl/⌘ F",
                                                "Search tasks or activity · Ctrl/⌘ F",
                                            ))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.focus_search(window, cx)
                                            })),
                                    )
                                    .when(self.page != Page::Home, |row| {
                                        row.child(
                                            Button::new("global-import")
                                                .primary()
                                                .icon(IconName::Plus)
                                                .label(self.t("导入归档", "Import archive"))
                                                .disabled(self.busy)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.send(Request::Import(None), cx)
                                                })),
                                        )
                                    }),
                            ),
                    )
                    .when_some(self.error.clone(), |view, error| {
                        view.child(
                            div()
                                .mx_6()
                                .mt_4()
                                .p_3()
                                .rounded_lg()
                                .border_1()
                                .border_color(cx.theme().danger.opacity(0.4))
                                .bg(cx.theme().danger.opacity(0.07))
                                .flex()
                                .items_center()
                                .gap_3()
                                .text_color(cx.theme().danger)
                                .child(Icon::new(IconName::Info).size(px(18.)))
                                .child(div().flex_1().min_w_0().child(error))
                                .child(
                                    Button::new("dismiss-error")
                                        .ghost()
                                        .small()
                                        .icon(IconName::X)
                                        .accessibility_label(self.t("关闭提示", "Dismiss error"))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.error = None;
                                            cx.notify();
                                        })),
                                ),
                        )
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "page-content-{}",
                                self.page as usize
                            )))
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p_6()
                            .child(content),
                    ),
            )
            .when(
                !self.message.is_empty() && !self.busy && self.error.is_none(),
                |view| {
                    view.child(
                        div()
                            .absolute()
                            .bottom_5()
                            .right_5()
                            .max_w(px(440.))
                            .px_4()
                            .py_3()
                            .rounded_xl()
                            .shadow_lg()
                            .bg(cx.theme().group_box)
                            .border_1()
                            .border_color(cx.theme().primary.opacity(0.4))
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .size(px(28.))
                                    .flex_shrink_0()
                                    .rounded_lg()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(cx.theme().primary.opacity(0.14))
                                    .child(
                                        Icon::new(IconName::Check)
                                            .size(px(16.))
                                            .text_color(cx.theme().primary),
                                    ),
                            )
                            .child(div().flex_1().min_w_0().child(self.message.clone()))
                            .child(
                                Button::new("dismiss-notice")
                                    .ghost()
                                    .small()
                                    .icon(IconName::X)
                                    .accessibility_label(self.t("关闭提示", "Dismiss notification"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.message.clear();
                                        cx.notify();
                                    })),
                            ),
                    )
                },
            )
            .into_any_element()
    }
}
