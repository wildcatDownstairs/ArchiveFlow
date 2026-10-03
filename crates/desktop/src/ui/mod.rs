//! GPUI 视图保留输入实体；页面切换和进度刷新不会重建输入状态。
mod closing;
mod detail;
mod home;
mod pages;
mod settings;
mod shell;

use crate::{
    model::{self, Settings, text},
    worker::{Event, RecoveryInput, Request, Snapshot},
};
use archiveflow_core::{
    domain::task::Task as ArchiveTask, services::hashcat_service::HashcatDetectionResult,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Selectable, Sizable, Theme, ThemeMode,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::{Input, InputState, Textarea, TextareaState},
    progress::Progress,
};
use gpui_kit::{prelude::FluentBuilder, *};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc, LazyLock,
        mpsc::{Receiver, Sender},
    },
    time::Duration,
};

actions!(archiveflow, [ImportArchive, FindTask, SavePreferences]);

// 与桌面应用图标共用原始资源；嵌入程序后，分发时无需额外携带图片文件。
static APP_ICON: LazyLock<Arc<Image>> = LazyLock::new(|| {
    Arc::new(Image::from_bytes(
        ImageFormat::Png,
        include_bytes!("../../icons/icon.png").to_vec(),
    ))
});

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Home,
    Tasks,
    Detail,
    Audit,
    Settings,
}

#[derive(Clone)]
enum Confirmation {
    Delete(String),
    ClearTasks,
    ClearAudit,
}

pub struct ArchiveFlow {
    runtime: archiveflow_core::runtime::AppRuntime,
    worker_stopped: Arc<std::sync::atomic::AtomicBool>,
    closing: closing::Closing,
    page: Page,
    settings: Settings,
    data_dir: PathBuf,
    snapshot: Option<Box<Snapshot>>,
    requests: Sender<Request>,
    selected: Option<String>,
    search: Entity<InputState>,
    audit_search: Entity<InputState>,
    shell_focus: FocusHandle,
    dictionary: Entity<TextareaState>,
    charset: Entity<InputState>,
    min_length: Entity<InputState>,
    max_length: Entity<InputState>,
    priority: Entity<InputState>,
    mask: Entity<InputState>,
    hashcat: Entity<InputState>,
    concurrency: Entity<InputState>,
    default_charset: Entity<InputState>,
    default_min: Entity<InputState>,
    default_max: Entity<InputState>,
    default_priority: Entity<InputState>,
    task_filter: &'static str,
    audit_filter: &'static str,
    task_page: usize,
    audit_page: usize,
    tree_page: usize,
    attack: usize,
    gpu: bool,
    rules: [bool; 10],
    expanded: HashSet<String>,
    tree: model::FileNode,
    busy: bool,
    message: String,
    error: Option<String>,
    detection: Option<HashcatDetectionResult>,
    reveal: bool,
    settings_dirty: bool,
    settings_tab: usize,
    show_rules: bool,
    show_advanced: bool,
    notice_timeout: Option<gpui_kit::Task<()>>,
    latest_draft: String,
    draft_save: Option<gpui_kit::Task<()>>,
    _poll: gpui_kit::Task<()>,
    _subscriptions: Vec<Subscription>,
}

fn input(value: impl Into<SharedString>, window: &mut Window, cx: &mut App) -> Entity<InputState> {
    cx.new(|cx| InputState::new(window, cx).default_value(value))
}

pub fn apply_theme(dark: bool, window: Option<&mut Window>, cx: &mut App) {
    Theme::change(
        if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        window,
        cx,
    );
    Theme::update(cx, |theme| {
        theme.font_size = px(15.);
        #[cfg(target_os = "windows")]
        {
            theme.font_family = "Microsoft YaHei UI".into();
        }
        theme.radius = px(7.);
        let colors = &mut theme.colors;
        colors.background = rgb(if dark { 0x101a23 } else { 0xf5f5f1 }).into();
        colors.foreground = rgb(if dark { 0xe5edf3 } else { 0x1c3040 }).into();
        colors.group_box = rgb(if dark { 0x17252f } else { 0xffffff }).into();
        colors.sidebar = rgb(0x122733).into();
        colors.border = rgb(if dark { 0x2c3c48 } else { 0xdce2e5 }).into();
        colors.muted_foreground = rgb(if dark { 0xa1b2bf } else { 0x657887 }).into();
        colors.primary = rgb(if dark { 0x49c6db } else { 0x087f99 }).into();
        colors.primary_foreground = rgb(if dark { 0x102630 } else { 0xffffff }).into();
        colors.secondary = rgb(if dark { 0x203541 } else { 0xeaf2f5 }).into();
        colors.secondary_foreground = colors.foreground;
        colors.accent = colors.secondary;
        colors.accent_foreground = colors.foreground;
        colors.ring = colors.primary;
        colors.button_primary = colors.primary;
        colors.button_primary_foreground = colors.primary_foreground;
        colors.button_primary_hover = rgb(if dark { 0x71d7e7 } else { 0x056c83 }).into();
        colors.button_primary_active = rgb(if dark { 0x2cb3ca } else { 0x065d72 }).into();
        colors.button_secondary = colors.secondary;
        colors.button_secondary_foreground = colors.foreground;
        colors.button_secondary_hover = rgb(if dark { 0x2b4352 } else { 0xdcebf0 }).into();
        colors.button_secondary_active = colors.button_secondary_hover;
    });
}

impl ArchiveFlow {
    pub fn new(
        settings: Settings,
        runtime: archiveflow_core::runtime::AppRuntime,
        worker_stopped: Arc<std::sync::atomic::AtomicBool>,
        requests: Sender<Request>,
        events: Receiver<Event>,
        startup_error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(text(
                settings.english,
                "搜索文件名或路径…",
                "Search names or paths…",
            ))
        });
        let audit_search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(text(
                settings.english,
                "搜索操作记录…",
                "Search activity…",
            ))
        });
        let shell_focus = cx.focus_handle();
        shell_focus.focus(window, cx);
        #[cfg(target_os = "macos")]
        let modifier = "cmd";
        #[cfg(not(target_os = "macos"))]
        let modifier = "ctrl";
        cx.bind_keys([
            KeyBinding::new(&format!("{modifier}-o"), ImportArchive, Some("ArchiveFlow")),
            KeyBinding::new(&format!("{modifier}-f"), FindTask, Some("ArchiveFlow")),
            KeyBinding::new(
                &format!("{modifier}-s"),
                SavePreferences,
                Some("ArchiveFlow"),
            ),
        ]);
        let dictionary = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(5)
                .placeholder(text(
                    settings.english,
                    "每行一个候选密码",
                    "One password per line",
                ))
                .default_value(settings.dictionary_draft.clone())
        });
        let charset = input(settings.charset.clone(), window, cx);
        let min_length = input(settings.min_length.to_string(), window, cx);
        let max_length = input(settings.max_length.to_string(), window, cx);
        let priority = input(settings.priority.to_string(), window, cx);
        let mask = input("?d?d?d?d", window, cx);
        let hashcat = input(settings.hashcat_path.clone(), window, cx);
        let concurrency = input(settings.concurrency.to_string(), window, cx);
        let default_charset = input(settings.charset.clone(), window, cx);
        let default_min = input(settings.min_length.to_string(), window, cx);
        let default_max = input(settings.max_length.to_string(), window, cx);
        let default_priority = input(settings.priority.to_string(), window, cx);
        let mut subscriptions = vec![cx.observe(&search, |this, _, cx| {
            this.task_page = 0;
            cx.notify();
        })];
        subscriptions.push(cx.observe(&audit_search, |this, _, cx| {
            this.audit_page = 0;
            cx.notify();
        }));
        subscriptions.push(cx.subscribe(&mask, |_, _, event, cx| {
            if matches!(event, gpui_kit::component::input::InputEvent::Change) {
                cx.notify();
            }
        }));
        for state in [
            &hashcat,
            &concurrency,
            &default_charset,
            &default_min,
            &default_max,
            &default_priority,
        ] {
            subscriptions.push(cx.subscribe(state, |this, _, event, cx| {
                if matches!(event, gpui_kit::component::input::InputEvent::Change) {
                    this.settings_dirty = true;
                    cx.notify();
                }
            }));
        }
        subscriptions.push(cx.subscribe(&dictionary, |this, state, event, cx| {
            if matches!(event, gpui_kit::component::input::InputEvent::Change) {
                this.latest_draft = state.read(cx).value().into();
                if !this.settings.clear_dictionary {
                    this.draft_save = Some(cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(500))
                            .await;
                        let _ = this.update(cx, |this, _| {
                            let _ = this
                                .requests
                                .send(Request::SaveDraft(this.latest_draft.clone()));
                        });
                    }));
                }
                cx.notify();
            }
        }));
        // 宿主事件只在 GPUI 的前台 executor 中更新实体；后台线程不持有窗口。
        let poll = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let received: Vec<_> = events.try_iter().collect();
                if this
                    .update_in(cx, |this, window, cx| {
                        let changed = !received.is_empty();
                        for event in received {
                            this.receive(event, window, cx);
                        }
                        this.tick_close(window, cx);
                        if changed {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut rules = [false; 10];
        rules[9] = settings.filename_patterns;
        Self {
            data_dir: runtime.data_dir().to_path_buf(),
            runtime,
            worker_stopped,
            closing: closing::Closing::default(),
            page: Page::Home,
            latest_draft: settings.dictionary_draft.clone(),
            draft_save: None,
            settings,
            snapshot: None,
            requests,
            selected: None,
            search,
            audit_search,
            shell_focus,
            dictionary,
            charset,
            min_length,
            max_length,
            priority,
            mask,
            hashcat,
            concurrency,
            default_charset,
            default_min,
            default_max,
            default_priority,
            task_filter: "all",
            audit_filter: "all",
            task_page: 0,
            audit_page: 0,
            tree_page: 0,
            attack: 0,
            gpu: false,
            rules,
            expanded: HashSet::new(),
            tree: model::FileNode::default(),
            busy: false,
            message: String::new(),
            error: startup_error,
            detection: None,
            reveal: false,
            settings_dirty: false,
            settings_tab: 0,
            show_rules: false,
            show_advanced: false,
            notice_timeout: None,
            _poll: poll,
            _subscriptions: subscriptions,
        }
    }

    fn t(&self, zh: &'static str, en: &'static str) -> &'static str {
        text(self.settings.english, zh, en)
    }

    fn receive(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        let feedback = !matches!(
            &event,
            Event::Snapshot(_) | Event::CloseReady | Event::CloseFailed(_)
        );
        match event {
            Event::CloseReady => self.closing.ready = true,
            Event::CloseFailed(error) => self.closing.error = Some(error),
            Event::SnapshotError(error) => self.error = Some(error),
            Event::Started => {
                self.busy = false;
                self.message = self.t("恢复任务已提交", "Recovery scheduled").into();
                if self.settings.clear_dictionary {
                    self.dictionary
                        .update(cx, |state, cx| state.set_value("", window, cx));
                    self.latest_draft.clear();
                }
            }
            Event::Snapshot(snapshot) => {
                if let Some(id) = &self.selected {
                    if !snapshot.tasks.iter().any(|task| &task.id == id) {
                        self.selected = None;
                        if self.page == Page::Detail {
                            self.page = Page::Tasks;
                        }
                    }
                }
                self.snapshot = Some(snapshot);
            }
            Event::Error(error) => {
                self.error = Some(error);
                self.busy = false;
                if self.closing.phase == closing::Phase::Saving {
                    self.closing = closing::Closing::default();
                    self.page = Page::Settings;
                }
            }
            Event::Message(message) => {
                self.message = message;
                self.busy = false;
            }
            Event::Dictionary(value) => {
                self.dictionary
                    .update(cx, |state, cx| state.set_value(value, window, cx));
                self.message = self.t("字典已载入", "Dictionary loaded").into();
                self.busy = false;
            }
            Event::Detection(result) => {
                self.detection = Some(result);
                self.busy = false;
            }
            Event::Saved(settings) => {
                if self.settings.charset != settings.charset {
                    self.charset.update(cx, |state, cx| {
                        state.set_value(settings.charset.clone(), window, cx)
                    });
                }
                if self.settings.min_length != settings.min_length {
                    self.min_length.update(cx, |state, cx| {
                        state.set_value(settings.min_length.to_string(), window, cx)
                    });
                }
                if self.settings.max_length != settings.max_length {
                    self.max_length.update(cx, |state, cx| {
                        state.set_value(settings.max_length.to_string(), window, cx)
                    });
                }
                if self.settings.priority != settings.priority {
                    self.priority.update(cx, |state, cx| {
                        state.set_value(settings.priority.to_string(), window, cx)
                    });
                }
                self.rules[9] = settings.filename_patterns;
                if settings.mask_results {
                    self.reveal = false;
                }
                self.settings = settings;
                self.settings_dirty = false;
                self.busy = false;
                self.message = self.t("设置已保存", "Settings saved").into();
                if self.closing.phase == closing::Phase::Saving {
                    self.begin_shutdown(cx);
                }
            }
        }
        if feedback && !self.message.is_empty() && self.error.is_none() {
            self.show_message(self.message.clone(), cx);
        }
    }

    fn send(&mut self, request: Request, cx: &mut Context<Self>) {
        self.error = None;
        self.message.clear();
        self.busy = !matches!(&request, Request::OpenData | Request::OpenLogs);
        if let Err(error) = self.requests.send(request) {
            self.error = Some(error.to_string());
            self.busy = false;
        }
        cx.notify();
    }

    fn navigate(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;

        self.error = None;
        cx.notify();
    }

    fn select_task(&mut self, task: &ArchiveTask, cx: &mut Context<Self>) {
        self.selected = Some(task.id.clone());
        self.reveal = !self.settings.mask_results;
        self.gpu = false;
        self.expanded.clear();
        self.tree_page = 0;
        self.show_rules = false;
        self.show_advanced = false;
        self.tree = model::file_tree(
            task.archive_info
                .as_ref()
                .map_or(&[], |info| info.entries.as_slice()),
        );
        self.navigate(Page::Detail, cx);
    }

    fn current_task(&self) -> Option<ArchiveTask> {
        self.snapshot
            .as_ref()?
            .tasks
            .iter()
            .find(|t| Some(&t.id) == self.selected.as_ref())
            .cloned()
    }

    fn export(&mut self, ids: Vec<String>, format: &str, cx: &mut Context<Self>) {
        self.send(
            Request::Export {
                ids,
                format: format.into(),
                options: archiveflow_core::commands::export_commands::ExportOptions {
                    mask_passwords: self.settings.mask_exports,
                    include_audit_events: self.settings.export_audit,
                },
            },
            cx,
        );
    }

    fn confirm_action(
        &mut self,
        action: Confirmation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use gpui_kit::component::{WindowExt, button::ButtonVariant, dialog::DialogButtonProps};
        let title = self.t("确认操作", "Confirm action");
        let label = match &action {
            Confirmation::Delete(_) => self.t(
                "删除这条任务记录？原始压缩包会保留。",
                "Delete this task record? The source archive will remain.",
            ),
            Confirmation::ClearTasks => self.t(
                "清空全部任务与恢复断点？原始压缩包会保留。",
                "Clear all tasks and checkpoints? Source archives will remain.",
            ),
            Confirmation::ClearAudit => self.t(
                "清空操作记录？会保留本次清空的记录。",
                "Clear activity history? A record of this action will remain.",
            ),
        };
        let accept = self.t("确认删除", "Delete");
        let cancel = self.t("取消", "Cancel");
        let view = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            let action = action.clone();
            dialog
                .title(title)
                .description(label)
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text(accept)
                        .ok_variant(ButtonVariant::Danger)
                        .cancel_text(cancel)
                        .show_cancel(true),
                )
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        let request = match &action {
                            Confirmation::Delete(id) => Request::Delete(id.clone()),
                            Confirmation::ClearTasks => Request::ClearTasks,
                            Confirmation::ClearAudit => Request::ClearAudit,
                        };
                        this.send(request, cx);
                    });
                    true
                })
        });
    }
}

impl Drop for ArchiveFlow {
    fn drop(&mut self) {
        let _ = self.requests.send(Request::Shutdown);
    }
}

fn panel(cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_4()
        .p_5()
        .rounded_xl()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().group_box)
        .shadow_sm()
}
fn section_heading(icon: IconName, title: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .gap_3()
        .child(Icon::new(icon).size(px(19.)).text_color(cx.theme().primary))
        .child(
            div()
                .text_base()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.into()),
        )
}
fn muted(value: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(value.into())
}
fn heading(title: impl Into<SharedString>, description: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .gap_2()
        .child(
            div()
                .text_2xl()
                .font_weight(FontWeight::SEMIBOLD)
                .text_ellipsis()
                .child(title.into()),
        )
        .child(muted(description, cx).text_ellipsis())
}
fn field(title: impl Into<SharedString>, state: &Entity<InputState>, cx: &App) -> Div {
    let title = title.into();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .flex_1()
        .min_w_0()
        .child(muted(title.clone(), cx))
        .child(Input::new(state).aria_label(title))
}
