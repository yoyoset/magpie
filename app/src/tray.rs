//! Tray icon and menu.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const TRAY_ID: &str = "magpie";
const STATUS_ID: &str = "status";

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, STATUS_ID, "启动中…", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "打开收件箱", true, None::<&str>)?;
    let pet = MenuItem::with_id(app, "pet", "显示 / 隐藏桌宠", true, None::<&str>)?;
    let folder = MenuItem::with_id(app, "folder", "打开配置与日志目录", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 Magpie", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&status, &sep, &open, &pet, &folder, &sep, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Magpie 喜鹊")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "pet" => crate::pet::toggle(app),
            "folder" => open_data_folder(),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    // Keep the status item reachable for set_status.
    app.manage(StatusItem(status));
    Ok(())
}

struct StatusItem(MenuItem<tauri::Wry>);

/// One-line state shown at the top of the tray menu and in the tooltip.
pub fn set_status(app: &AppHandle, text: &str) {
    if let Some(item) = app.try_state::<StatusItem>() {
        let _ = item.0.set_text(text);
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(format!("Magpie 喜鹊 · {text}")));
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn open_data_folder() {
    match magpie_config::paths::Paths::default_location() {
        Ok(p) => {
            if let Err(e) = std::process::Command::new("explorer").arg(p.root()).spawn() {
                tracing::warn!(error = %e, "could not open data folder");
            }
        }
        Err(e) => tracing::warn!(error = %e, "no data folder"),
    }
}
