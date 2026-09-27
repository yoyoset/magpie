//! Full-screen takeover for critical events.
//!
//! The queue of pending critical events lives here in Rust, not in the page:
//! the page fetches it on load and whenever `takeover://changed` fires, so an
//! event delivered before the window finished loading is never lost.
//!
//! Escape hatches, deliberately: Alt+F4 hides the window (see main.rs), the
//! tray has 关闭全屏提醒, and the next escalation round brings it back anyway.
//! A broken page must never lock the user out of their desktop.

use std::sync::Mutex;

use magpie_channels::{BoxFuture, Channel, ChannelError, Message};
use magpie_config::settings::TAKEOVER;
use magpie_core::Reach;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "takeover";
pub const CHANGED_EVENT: &str = "takeover://changed";

#[derive(Debug, Clone, Serialize)]
pub struct Pending {
    pub event_id: String,
    pub title: String,
    pub body: String,
    pub source: String,
    pub attempt: u32,
}

#[derive(Default)]
pub struct Queue(Mutex<Vec<Pending>>);

impl Queue {
    pub fn snapshot(&self) -> Vec<Pending> {
        self.0.lock().map(|q| q.clone()).unwrap_or_default()
    }

    /// Adds or refreshes an event; returns the queue length.
    fn upsert(&self, p: Pending) -> usize {
        let Ok(mut q) = self.0.lock() else { return 0 };
        match q.iter_mut().find(|x| x.event_id == p.event_id) {
            Some(existing) => *existing = p,
            None => q.push(p),
        }
        q.len()
    }

    /// Removes an event; returns how many remain.
    pub fn remove(&self, event_id: &str) -> usize {
        let Ok(mut q) = self.0.lock() else { return 0 };
        q.retain(|x| x.event_id != event_id);
        q.len()
    }
}

fn show(app: &AppHandle) -> tauri::Result<()> {
    let window = match app.get_webview_window(LABEL) {
        Some(w) => w,
        None => WebviewWindowBuilder::new(
            app,
            LABEL,
            WebviewUrl::App("index.html?view=takeover".into()),
        )
        .title("Magpie · 紧急")
        .fullscreen(true)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .build()?,
    };
    window.show()?;
    window.set_focus()
}

pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
}

pub struct TakeoverChannel {
    app: AppHandle,
}

impl TakeoverChannel {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl Channel for TakeoverChannel {
    fn id(&self) -> &'static str {
        TAKEOVER
    }

    fn reach(&self) -> Reach {
        Reach::Takeover
    }

    fn deliver<'a>(&'a self, msg: &'a Message) -> BoxFuture<'a, Result<(), ChannelError>> {
        Box::pin(async move {
            self.app.state::<Queue>().upsert(Pending {
                event_id: msg.event_id.to_string(),
                title: msg.title.clone(),
                body: msg.body.clone(),
                source: msg.source.clone(),
                attempt: msg.attempt,
            });
            // Window work must happen on the main thread.
            let app = self.app.clone();
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.app
                .run_on_main_thread(move || {
                    let _ = tx.send(show(&app));
                })
                .map_err(|e| ChannelError::Transient(format!("takeover: {e}")))?;
            rx.await
                .map_err(|_| ChannelError::Transient("takeover: main thread dropped".into()))?
                .map_err(|e| ChannelError::Transient(format!("takeover: {e}")))?;
            let _ = self.app.emit_to(LABEL, CHANGED_EVENT, ());
            Ok(())
        })
    }
}
