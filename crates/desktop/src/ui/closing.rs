//! 关闭先处理未保存设置，再等待草稿和恢复断点落盘；超时由用户决定是否强制退出。
use super::*;
use gpui_kit::component::WindowExt;
use std::{sync::atomic::Ordering, time::Instant};

#[derive(Default, PartialEq, Eq)]
pub(super) enum Phase {
    #[default]
    Idle,
    Confirming,
    Saving,
    Draining,
}

#[derive(Default)]
pub(super) struct Closing {
    pub phase: Phase,
    pub ready: bool,
    pub error: Option<String>,
    started: Option<Instant>,
    timed_out: bool,
}

impl Closing {
    pub fn active(&self) -> bool {
        matches!(self.phase, Phase::Saving | Phase::Draining)
    }
}

impl ArchiveFlow {
    pub fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing.phase != Phase::Idle {
            return;
        }
        if !self.settings_dirty {
            self.begin_shutdown(cx);
            return;
        }
        // 窗口关闭与删任务等对话框不能叠在一起。
        window.close_all_dialogs(cx);
        self.closing.phase = Phase::Confirming;
        let english = self.settings.english;
        let view = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let cancel = view.clone();
            let discard = view.clone();
            let save = view.clone();
            let dismissed = view.clone();
            dialog
                .title(text(english, "保存更改后退出？", "Save changes before closing?"))
                .description(text(english, "偏好设置有未保存的更改。你可以保存、放弃本次设置修改，或继续编辑。", "Your preferences have unsaved changes. Save them, discard these edits, or keep editing."))
                .on_close(move |_, _, cx| {
                    let _ = dismissed.update(cx, |this, cx| {
                        if this.closing.phase == Phase::Confirming {
                            this.closing = Closing::default();
                        }
                        cx.notify();
                    });
                })
                .footer(div().flex().justify_end().gap_2()
                    .child(Button::new("cancel-close").label(text(english, "继续编辑", "Keep editing"))
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            let _ = cancel.update(cx, |this, cx| {
                                this.closing = Closing::default();
                                cx.notify();
                            });
                        }))
                    .child(Button::new("discard-and-close").label(text(english, "不保存并退出", "Discard & close"))
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            let _ = discard.update(cx, |this, cx| this.begin_shutdown(cx));
                        }))
                    .child(Button::new("save-and-close").primary().label(text(english, "保存并退出", "Save & close"))
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            let _ = save.update(cx, |this, cx| {
                                this.closing.phase = Phase::Saving;
                                this.closing.started = Some(Instant::now());
                                if !this.save_preferences(cx) {
                                    this.closing = Closing::default();
                                    this.page = Page::Settings;
                                }
                                cx.notify();
                            });
                        })))
        });
        cx.notify();
    }

    pub(super) fn begin_shutdown(&mut self, cx: &mut Context<Self>) {
        self.draft_save = None;
        self.closing = Closing {
            phase: Phase::Draining,
            started: Some(Instant::now()),
            ..Default::default()
        };
        self.runtime.request_shutdown();
        self.flush_close(cx);
    }

    fn flush_close(&mut self, cx: &mut Context<Self>) {
        self.closing.error = self
            .requests
            .send(Request::PrepareClose(self.latest_draft.clone()))
            .err()
            .map(|error| error.to_string());
        cx.notify();
    }

    pub(super) fn tick_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.closing.active() {
            return;
        }
        if self.closing.ready
            && self.worker_stopped.load(Ordering::Acquire)
            && !self.runtime.recovery_manager().has_running_tasks()
        {
            window.remove_window();
            return;
        }
        if !self.closing.timed_out
            && self
                .closing
                .started
                .is_some_and(|start| start.elapsed() >= Duration::from_secs(8))
        {
            self.closing.timed_out = true;
            let runtime = self.runtime.clone();
            std::thread::spawn(move || {
                let _ = archiveflow_core::services::app_log_service::append_app_log(
                    &runtime,
                    "WARN",
                    Some("shutdown"),
                    "关闭等待超过 8 秒，界面已提供继续等待与强制退出选项",
                );
            });
            cx.notify();
        }
    }

    pub(super) fn closing_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let stalled = self.closing.timed_out || self.closing.error.is_some();
        div().size_full().flex().items_center().justify_center()
            .bg(cx.theme().background).text_color(cx.theme().foreground)
            .font_family(cx.theme().font_family.clone())
            .child(panel(cx).w(px(540.)).gap_5()
                .child(section_heading(IconName::Save, if stalled {
                    self.t("退出尚未完成", "Closing is taking longer")
                } else if self.closing.phase == Phase::Saving {
                    self.t("正在保存设置", "Saving preferences")
                } else {
                    self.t("正在安全退出", "Closing safely")
                }, cx))
                .child(muted(if stalled {
                    self.t("仍在等待后台操作结束。强制退出可能丢失尚未保存的设置、字典草稿或最新恢复进度。", "Still waiting for background work. Force closing may lose unsaved preferences, dictionary edits, or recent recovery progress.")
                } else {
                    self.t("正在保存字典草稿并等待恢复任务写入断点…", "Saving your dictionary draft and waiting for recovery checkpoints…")
                }, cx))
                .when_some(self.closing.error.clone(), |view, error| view.child(
                    div().text_sm().text_color(cx.theme().danger).child(error)))
                .when(stalled, |view| view.child(div().flex().justify_end().gap_3()
                    .child(Button::new("wait-for-close").label(self.t("继续等待", "Keep waiting"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.closing.timed_out = false;
                            this.closing.started = Some(Instant::now());
                            cx.notify();
                        })))
                    .when(self.closing.error.is_some() && !self.worker_stopped.load(Ordering::Acquire), |row| row.child(
                        Button::new("retry-close-save").primary().label(self.t("重试保存", "Retry save"))
                            .on_click(cx.listener(|this, _, _, cx| this.flush_close(cx)))))
                    .child(Button::new("force-close").danger().label(self.t("强制退出", "Force close"))
                        .on_click(|_, window, _| window.remove_window())))))
            .into_any_element()
    }
}
