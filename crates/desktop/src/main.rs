#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod model;
mod ui;
mod worker;

use archiveflow_core::runtime::AppRuntime;
use gpui_kit::*;
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("ArchiveFlow: {error:#}");
        rfd::MessageDialog::new()
            .set_title("ArchiveFlow")
            .set_description(format!("{error:#}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
}

fn run() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut data_dir = None;
    let mut imports = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                data_dir =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        anyhow::anyhow!("--data-dir requires a directory")
                    })?))
            }
            "--import" => imports.push(PathBuf::from(
                args.next()
                    .ok_or_else(|| anyhow::anyhow!("--import requires a path"))?,
            )),
            "--help" | "-h" => {
                println!("ArchiveFlow [--data-dir DIRECTORY] [--import ARCHIVE]...");
                return Ok(());
            }
            _ => anyhow::bail!("Unknown argument: {arg}"),
        }
    }
    let data_dir = data_dir.unwrap_or(AppRuntime::default_data_dir()?);
    let runtime = AppRuntime::new(data_dir.clone())?;
    archiveflow_core::services::app_log_service::install_logger(&runtime)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let (settings, startup_error) = match model::Settings::load(&data_dir) {
        Ok(settings) => (settings, None),
        Err(error) => (
            model::Settings::default(),
            Some(format!(
                "偏好设置读取失败，原文件已保留 / Could not load preferences: {error}"
            )),
        ),
    };
    let (requests, events, worker_stopped) = worker::start(runtime.clone(), settings.clone());
    if !imports.is_empty() {
        requests.send(worker::Request::Import(Some(imports)))?;
    }
    application().with_assets(assets::AllAssets).run(move |cx| {
        init(cx);
        ui::apply_theme(settings.dark, None, cx);
        let bounds = Bounds::centered(None, size(px(1280.), px(860.)), cx);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("ArchiveFlow".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(960.), px(680.))),
                app_id: Some("com.archiveflow.app".into()),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                let view = cx.new(|cx| {
                    ui::ArchiveFlow::new(
                        settings,
                        runtime,
                        worker_stopped,
                        requests,
                        events,
                        startup_error,
                        window,
                        cx,
                    )
                });
                let closing_view = view.downgrade();
                window.on_window_should_close(cx, move |window, cx| {
                    let _ = closing_view.update(cx, |view, cx| view.request_close(window, cx));
                    false
                });
                view
            },
        )
        .expect("无法打开 ArchiveFlow 窗口");
        cx.activate(true);
    });
    Ok(())
}
