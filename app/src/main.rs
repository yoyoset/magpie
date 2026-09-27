//! Magpie desktop shell.
//!
//! Only wiring lives here: tray, window lifecycle, toasts, and starting the
//! API server + runtime from the library crates. No business rules.

// Release builds are GUI apps with no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bootstrap;
mod channels;
mod logging;
mod services;
mod toast;
mod tray;

use tauri::{Manager, WindowEvent};

fn main() {
    tauri::Builder::default()
        // Must be first: a second launch hands over to the running instance.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let paths = magpie_config::paths::Paths::default_location()?;
            app.manage(logging::init(&paths)?);
            tray::build(app.handle())?;
            services::start(app.handle().clone(), paths);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window hides it; Magpie keeps running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("tauri runtime failed to start");
}
