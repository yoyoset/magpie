//! Starts the API server and the runtime, and announces the endpoint.

use std::sync::Arc;

use magpie_config::endpoint::Endpoint;
use magpie_config::paths::Paths;
use magpie_core::{EventDraft, EventId, Severity};
use magpie_runtime::Dispatcher;
use magpie_server::{AppState, SystemClock};
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;
use tokio::sync::Notify;

use crate::{bootstrap, channels, commands, tray};

pub const SELF_SOURCE: &str = "magpie";

pub fn start(app: AppHandle, paths: Paths) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run(&app, paths).await {
            // Startup failed (bad config, port, credential store). The tray is
            // up, so say so there and in a toast; don't die silently.
            tracing::error!(error = format!("{e:#}"), "startup failed");
            tray::set_status(&app, &format!("启动失败：{e:#}"));
            let _ = app
                .notification()
                .builder()
                .title("Magpie 启动失败")
                .body(format!("{e:#}"))
                .show();
        }
    });
}

async fn run(app: &AppHandle, paths: Paths) -> anyhow::Result<()> {
    let boot = bootstrap::boot(paths)?;
    let built = channels::build(app, &boot.settings);
    report_warnings(&boot, &built.warnings)?;

    let changed = Arc::new(Notify::new());
    let listener = bind(boot.settings.api.port).await?;
    let port = listener.local_addr()?.port();
    Endpoint { port }.write(&boot.paths)?;
    tracing::info!(port, channels = built.channels.len(), "services up");

    let state = AppState::new(
        Arc::clone(&boot.store),
        boot.api_token.clone(),
        Arc::new(SystemClock),
        Arc::clone(&changed),
    );
    let channel_ids = built.channels.iter().map(|c| c.id().to_owned()).collect();
    app.manage(commands::Services {
        state: state.clone(),
        info: commands::RuntimeInfo {
            port,
            data_dir: boot.paths.root().display().to_string(),
            config_file: boot.paths.config_file().display().to_string(),
            channels: channel_ids,
            webhook_ids: boot
                .settings
                .webhooks
                .iter()
                .map(|w| w.id.clone())
                .collect(),
        },
    });
    tauri::async_runtime::spawn(async move {
        if let Err(e) = magpie_server::serve(listener, state).await {
            tracing::error!(error = %e, "API server stopped");
        }
    });

    let dispatcher = Arc::new(Dispatcher::new(
        Arc::clone(&boot.store),
        built.channels,
        boot.settings.routes.clone(),
        boot.settings.escalation,
    ));
    tauri::async_runtime::spawn(magpie_runtime::run(dispatcher, changed, chrono::Local));

    if boot.settings.desktop.pet {
        if let Err(e) = crate::pet::create(app) {
            tracing::warn!(error = %e, "could not create pet window");
        }
    }
    apply_autostart(app, boot.settings.desktop.autostart);
    tray::set_status(app, &format!("运行中 · 127.0.0.1:{port}"));
    Ok(())
}

/// The configured port, or any free one if it is taken (clients read
/// `endpoint.json`, so the exact number does not matter to them).
async fn bind(port: u16) -> std::io::Result<tokio::net::TcpListener> {
    match magpie_server::bind_loopback(port).await {
        Ok(l) => Ok(l),
        Err(e) if port != 0 => {
            tracing::warn!(port, error = %e, "configured port unavailable, using a free one");
            magpie_server::bind_loopback(0).await
        }
        Err(e) => Err(e),
    }
}

/// Configuration problems become inbox events so they are seen, not just logged.
fn report_warnings(boot: &bootstrap::Boot, warnings: &[String]) -> anyhow::Result<()> {
    let store = boot
        .store
        .lock()
        .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
    for w in warnings {
        tracing::warn!(warning = %w, "configuration");
        let event = EventDraft {
            source: SELF_SOURCE.into(),
            title: "Magpie 配置问题".into(),
            body: w.clone(),
            severity: Severity::Normal,
            tags: vec!["config".into()],
        }
        .validate(EventId::new(), chrono::Utc::now())?;
        store.insert_event(&event)?;
    }
    Ok(())
}

/// Only release builds touch the login items: a debug build would register
/// `target\debug\magpie-app.exe` to start at every login.
fn apply_autostart(app: &AppHandle, wanted: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    let launcher = app.autolaunch();
    let result = match (wanted, launcher.is_enabled()) {
        (true, Ok(false)) => launcher.enable(),
        (false, Ok(true)) => launcher.disable(),
        (_, Err(e)) => Err(e),
        _ => Ok(()),
    };
    if let Err(e) = result {
        tracing::warn!(error = %e, "could not update autostart");
    }
}
