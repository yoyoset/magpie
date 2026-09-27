//! Tauri commands: the UI's adapter over `magpie_server::ops`.
//!
//! Errors cross to JS as plain strings (the human message). Every command
//! works only after services are up; before that it says so instead of panicking.

// `#[tauri::command]` deserializes every argument into an owned value, so the
// by-value parameters are dictated by the macro, not a choice.
#![allow(clippy::needless_pass_by_value)]

use chrono::{DateTime, Utc};
use magpie_config::secrets::{OsKeyring, SecretStore};
use magpie_config::settings::secret_keys;
use magpie_core::{AckKind, EventId, Reminder, ReminderId};
use magpie_server::ops::{self, EventDetail, EventView, ListQuery, NewReminder};
use magpie_server::{ApiError, AppState};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

/// Registered by `services` once the store and API are up.
pub struct Services {
    pub state: AppState,
    pub info: RuntimeInfo,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeInfo {
    pub port: u16,
    pub data_dir: String,
    pub config_file: String,
    pub channels: Vec<String>,
    /// Webhook ids from config, so their secrets can be set from the UI.
    pub webhook_ids: Vec<String>,
}

type CmdResult<T> = Result<T, String>;

fn services(app: &AppHandle) -> CmdResult<State<'_, Services>> {
    app.try_state::<Services>()
        .ok_or_else(|| "Magpie 还在启动，或启动失败（看托盘菜单第一行）。".to_owned())
}

fn err(e: ApiError) -> String {
    e.to_string()
}

fn parse_id(id: &str) -> CmdResult<Uuid> {
    Uuid::parse_str(id).map_err(|_| format!("`{id}` 不是有效的 id"))
}

#[tauri::command]
pub async fn list_events(app: AppHandle, query: ListQuery) -> CmdResult<Vec<EventView>> {
    let s = services(&app)?;
    ops::list_events(&s.state, query).await.map_err(err)
}

#[tauri::command]
pub async fn event_detail(app: AppHandle, id: String) -> CmdResult<EventDetail> {
    let s = services(&app)?;
    ops::event_detail(&s.state, EventId(parse_id(&id)?))
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn ack_event(app: AppHandle, id: String, kind: AckKind) -> CmdResult<()> {
    let s = services(&app)?;
    ops::ack_event(&s.state, EventId(parse_id(&id)?), kind, "desktop")
        .await
        .map_err(err)
}

#[derive(Debug, Serialize)]
pub struct ReminderView {
    #[serde(flatten)]
    reminder: Reminder,
    next_fire: Option<DateTime<Utc>>,
}

impl From<Reminder> for ReminderView {
    fn from(reminder: Reminder) -> Self {
        let next_fire = reminder
            .enabled
            .then(|| reminder.schedule.next_fire(Utc::now(), &chrono::Local))
            .flatten();
        Self {
            reminder,
            next_fire,
        }
    }
}

#[tauri::command]
pub async fn list_reminders(app: AppHandle) -> CmdResult<Vec<ReminderView>> {
    let s = services(&app)?;
    let rs = ops::list_reminders(&s.state).await.map_err(err)?;
    Ok(rs.into_iter().map(ReminderView::from).collect())
}

#[tauri::command]
pub async fn create_reminder(app: AppHandle, reminder: NewReminder) -> CmdResult<ReminderView> {
    let s = services(&app)?;
    ops::create_reminder(&s.state, reminder)
        .await
        .map(ReminderView::from)
        .map_err(err)
}

#[tauri::command]
pub async fn delete_reminder(app: AppHandle, id: String) -> CmdResult<()> {
    let s = services(&app)?;
    ops::delete_reminder(&s.state, ReminderId(parse_id(&id)?))
        .await
        .map_err(err)
}

#[derive(Debug, Serialize)]
pub struct SecretStatus {
    key: String,
    label: String,
    present: bool,
}

#[derive(Debug, Serialize)]
pub struct AppInfo {
    version: &'static str,
    #[serde(flatten)]
    runtime: RuntimeInfo,
    secrets: Vec<SecretStatus>,
}

/// Secrets the UI may set. The API token is deliberately not in this list.
fn settable_secrets(info: &RuntimeInfo) -> Vec<(String, String)> {
    let mut keys = vec![
        (
            secret_keys::TELEGRAM_BOT_TOKEN.to_owned(),
            "Telegram bot token".to_owned(),
        ),
        (
            secret_keys::NTFY_TOKEN.to_owned(),
            "ntfy access token".to_owned(),
        ),
    ];
    keys.extend(info.webhook_ids.iter().map(|id| {
        (
            secret_keys::webhook_secret(id),
            format!("Webhook `{id}` secret"),
        )
    }));
    keys
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> CmdResult<AppInfo> {
    let s = services(&app)?;
    let secrets = settable_secrets(&s.info)
        .into_iter()
        .map(|(key, label)| {
            let present = OsKeyring
                .get(&key)
                .ok()
                .flatten()
                .is_some_and(|v| !v.is_empty());
            SecretStatus {
                key,
                label,
                present,
            }
        })
        .collect();
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        runtime: s.info.clone(),
        secrets,
    })
}

#[tauri::command]
pub fn set_secret(app: AppHandle, key: String, value: String) -> CmdResult<()> {
    let s = services(&app)?;
    if !settable_secrets(&s.info).iter().any(|(k, _)| *k == key) {
        return Err(format!("不允许从界面设置 `{key}`"));
    }
    let value = value.trim();
    if value.is_empty() {
        return Err("值不能为空".into());
    }
    OsKeyring.set(&key, value).map_err(|e| e.to_string())?;
    tracing::info!(%key, "secret updated from UI"); // key only, never the value
    Ok(())
}

#[tauri::command]
pub fn open_data_folder() {
    crate::tray::open_data_folder();
}
