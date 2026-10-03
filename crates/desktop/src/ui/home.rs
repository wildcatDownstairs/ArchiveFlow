use super::*;
use archiveflow_core::domain::task::TaskStatus;

impl ArchiveFlow {
    pub(super) fn home(&self, cx: &mut Context<Self>) -> AnyElement {
        let tasks = self
            .snapshot
            .as_ref()
            .map(|s| s.tasks.as_slice())
            .unwrap_or(&[]);
        let recent = tasks.iter().take(5).cloned().collect::<Vec<_>>();
        let suggested = tasks
            .iter()
            .find(|t| t.status == TaskStatus::Processing)
            .or_else(|| tasks.iter().find(|t| t.status == TaskStatus::Interrupted))
            .or_else(|| {
                tasks.iter().find(|t| {
                    t.status == TaskStatus::Ready
                        && t.archive_info.as_ref().is_some_and(|a| a.is_encrypted)
                })
            })
            .or_else(|| tasks.iter().find(|t| t.status == TaskStatus::Succeeded))
            .cloned();
        let metrics = [
            (
                "all",
                self.t("全部归档", "All archives"),
                tasks.len(),
                IconName::Archive,
            ),
            (
                "processing",
                self.t("正在恢复", "In progress"),
                tasks
                    .iter()
                    .filter(|t| t.status == TaskStatus::Processing)
                    .count(),
                IconName::Activity,
            ),
            (
                "succeeded",
                self.t("恢复成功", "Recovered"),
                tasks
                    .iter()
                    .filter(|t| t.status == TaskStatus::Succeeded)
                    .count(),
                IconName::Check,
            ),
            (
                "interrupted",
                self.t("等待继续", "To resume"),
                tasks
                    .iter()
                    .filter(|t| t.status == TaskStatus::Interrupted)
                    .count(),
                IconName::Pause,
            ),
        ];
        div().flex().flex_col().gap_5().max_w(px(1240.)).mx_auto()
            .child(div().relative().overflow_hidden().rounded_xl()
                .bg(linear_gradient(130., linear_color_stop(rgb(0x0e3140), 0.), linear_color_stop(rgb(0x1d5467), 1.)))
                .border_1().border_color(rgb(0x3a7285).opacity(0.5)).shadow_lg()
                .p_6().flex().items_center().gap_6()
                // 柔和光晕，让英雄区更有层次而不喧宾夺主。
                .child(div().absolute().top(px(-70.)).right(px(-30.)).size(px(230.)).rounded_full().bg(rgb(0x37c2da).opacity(0.16)))
                .child(div().absolute().bottom(px(-90.)).right(px(150.)).size(px(170.)).rounded_full().bg(rgb(0xf3c24f).opacity(0.08)))
                .child(div().relative().flex_1().min_w_0().flex().flex_col().gap_3()
                    .child(div().flex().items_center().gap_2().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(rgb(0x9fd0de))
                        .child(div().w(px(20.)).h(px(2.)).rounded_full().bg(rgb(0xf3c24f)))
                        .child(self.t("从归档，到结果", "FROM ARCHIVE TO ANSWER")))
                    .child(div().text_3xl().font_weight(FontWeight::BOLD).text_color(rgb(0xf4fafb))
                        .child(self.t("让归档，重新可用。", "Your archives. Accessible again.")))
                    .child(div().text_sm().text_color(rgb(0xbcd2dc))
                        .child(self.t("把 ZIP、7Z 或 RAR 拖进窗口，剩下的交给工作空间。", "Drop a ZIP, 7Z or RAR into this window to get started.")))
                    .child(div().flex().items_center().gap_4().mt_2()
                        .child(Button::new("import-main").primary().h(px(40.)).px_5().icon(IconName::Plus)
                            .label(self.t("导入新归档", "Import archive")).disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.send(Request::Import(None), cx))))
                        .child(div().flex().items_center().gap_1p5().text_xs().text_color(rgb(0x9fbccb))
                            .child(Icon::new(IconName::CornerDownLeft).size(px(13.)))
                            .child("Ctrl / ⌘ O"))))
                .child(div().relative().flex_shrink_0().size(px(108.)).rounded_2xl()
                    .bg(rgb(0xffffff).opacity(0.06)).border_1().border_color(rgb(0xffffff).opacity(0.14))
                    .flex().items_center().justify_center()
                    .child(img(APP_ICON.clone()).size(px(66.)).object_fit(ObjectFit::Contain))))
            .child(div().flex().rounded_xl().border_1().border_color(cx.theme().border).bg(cx.theme().group_box).overflow_hidden().shadow_sm()
                .children(metrics.into_iter().enumerate().map(|(index, (filter, title, value, icon))| {
                    let accent = match filter {
                        "succeeded" => rgb(0x3fae84),
                        "interrupted" => rgb(0xcf9a3c),
                        "processing" => rgb(0x37c2da),
                        _ => cx.theme().primary.into(),
                    };
                    Button::new(("metric", index)).ghost().flex_1().min_w_0().h(px(96.)).px_5().rounded_none()
                        .accessibility_label(format!("{title}: {value}"))
                        .when(index > 0, |button| button.border_l_1().border_color(cx.theme().border))
                        .child(div().flex().w_full().items_center().gap_3p5()
                            .child(div().size(px(42.)).rounded_lg().flex_shrink_0().flex().items_center().justify_center()
                                .bg(accent.opacity(0.14)).border_1().border_color(accent.opacity(0.22))
                                .child(Icon::new(icon).size(px(20.)).text_color(accent)))
                            .child(div().flex_1().min_w_0().flex().flex_col().gap_0p5()
                                .child(div().text_3xl().font_weight(FontWeight::BOLD).line_height(px(34.)).child(value.to_string()))
                                .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(cx.theme().muted_foreground).text_ellipsis().child(title))))
                        .on_click(cx.listener(move |this, _, window, cx| this.show_tasks(filter, window, cx)))
                })))
            .when_some(suggested, |view, task| {
                let (label, action, icon, accent) = match task.status {
                    TaskStatus::Processing => (self.t("任务正在后台运行", "Your task is running"), self.t("查看进度", "View progress"), IconName::LoaderCircle, rgb(0x37c2da)),
                    TaskStatus::Succeeded => (self.t("一个已完成的结果", "A result is ready"), self.t("查看结果", "View result"), IconName::CircleCheck, rgb(0x3fae84)),
                    _ => (self.t("接着上次的工作", "Pick up where you left off"), self.t("继续处理", "Continue"), IconName::RotateCcw, rgb(0xcf9a3c)),
                };
                view.child(div().relative().overflow_hidden().flex().items_center().gap_4().px_5().py_4().rounded_xl().bg(cx.theme().group_box).shadow_sm()
                    .border_1().border_color(accent.opacity(0.4)).border_l_3().border_color(accent)
                    .child(div().size(px(40.)).rounded_lg().flex_shrink_0().flex().items_center().justify_center()
                        .bg(accent.opacity(0.14)).child(Icon::new(icon).size(px(20.)).text_color(accent)))
                    .child(div().flex_1().min_w_0().flex().flex_col().gap_1()
                        .child(div().flex().items_center().gap_2()
                            .child(div().size(px(6.)).rounded_full().bg(accent))
                            .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(cx.theme().muted_foreground).child(label)))
                        .child(div().font_weight(FontWeight::SEMIBOLD).text_ellipsis().child(task.file_name.clone())))
                    .child(Button::new("suggested-task").primary().label(action).icon(IconName::ArrowRight)
                        .on_click(cx.listener(move |this, _, _, cx| this.select_task(&task, cx)))))
            })
            .child(div().flex().items_center().justify_between()
                .child(div().flex().items_center().gap_3()
                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(self.t("最近归档", "Recent archives")))
                    .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("{:02}", recent.len()))))
                .child(Button::new("all-tasks").ghost().small().label(self.t("全部任务", "All archives")).icon(IconName::ArrowRight)
                    .on_click(cx.listener(|this, _, window, cx| this.show_tasks("all", window, cx)))))
            .child(if recent.is_empty() {
                panel(cx).gap_5().child(heading(self.t("从第一份归档开始", "Start with your first archive"),
                    self.t("导入后可以先检查内容，再决定是否需要恢复密码。", "Inspect its contents, then decide whether password recovery is needed."), cx))
                    .child(div().flex().gap_3().children([
                        ("1", self.t("导入文件", "Import a file")), ("2", self.t("检查与配置", "Inspect & configure")), ("3", self.t("查看与导出结果", "Review & export")),
                    ].into_iter().map(|(number, title)| div().flex_1().flex().items_center().gap_3().px_4().py_3().rounded_lg()
                        .bg(cx.theme().secondary.opacity(0.5)).border_1().border_color(cx.theme().border)
                        .child(div().size(px(28.)).flex_shrink_0().rounded_full().flex().items_center().justify_center()
                            .bg(cx.theme().primary.opacity(0.14)).text_sm().font_weight(FontWeight::BOLD).text_color(cx.theme().primary).child(number))
                        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title)))))
                    .into_any_element()
            } else { self.task_list(recent, false, cx).into_any_element() })
            .into_any_element()
    }
}
