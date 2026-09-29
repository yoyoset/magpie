//! Magpie desktop shell.
//!
//! Only wiring lives here: tray, window lifecycle, toasts, and starting the
//! API server + runtime from the library crates. No business rules.

// Release builds are GUI apps with no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bootstrap;
mod channels;
mod commands;
mod logging;
mod pet;
mod services;
mod takeover;
mod telegram_inbox;
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
            app.manage(takeover::Queue::default());
            tray::build(app.handle())?;
            services::start(app.handle().clone(), paths);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_events,
            commands::event_detail,
            commands::ack_event,
            commands::list_reminders,
            commands::create_reminder,
            commands::delete_reminder,
            commands::app_info,
            commands::set_secret,
            commands::open_data_folder,
            commands::show_inbox,
            commands::hide_pet,
            commands::pet_hit_rects,
            commands::takeover_pending,
            commands::takeover_done,
            commands::list_items,
            commands::create_item,
            commands::change_item,
            commands::plan_day,
        ])
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
