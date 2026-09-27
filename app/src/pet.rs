//! The desktop pet: a small transparent always-on-top window, plus the `pet`
//! channel that pops speech bubbles on it.

use magpie_channels::{BoxFuture, Channel, ChannelError, Message};
use magpie_config::settings::PET;
use magpie_core::Reach;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "pet";
pub const MESSAGE_EVENT: &str = "pet://message";
const WIDTH: f64 = 300.0;
const HEIGHT: f64 = 320.0;
/// Gap between the pet and the screen's work-area edge, in logical pixels.
const MARGIN: f64 = 16.0;

/// Creates the pet window (hidden until positioned) bottom-right above the taskbar.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(LABEL).is_some() {
        return Ok(());
    }
    let window =
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html?view=pet".into()))
            .title("Magpie")
            .inner_size(WIDTH, HEIGHT)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .visible(false)
            .build()?;

    if let Some(monitor) = window.primary_monitor()? {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let w = (WIDTH + MARGIN) * scale;
        let h = (HEIGHT + MARGIN) * scale;
        #[allow(clippy::cast_possible_truncation)] // screen coordinates fit in i32
        let pos = PhysicalPosition::new(
            area.position.x + i32::try_from(area.size.width).unwrap_or(i32::MAX) - w as i32,
            area.position.y + i32::try_from(area.size.height).unwrap_or(i32::MAX) - h as i32,
        );
        window.set_position(pos)?;
    }
    window.show()
}

pub fn toggle(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let visible = w.is_visible().unwrap_or(false);
        let _ = if visible { w.hide() } else { w.show() };
    } else if let Err(e) = create(app) {
        tracing::warn!(error = %e, "could not create pet window");
    }
}

#[derive(Debug, Clone, Serialize)]
struct Bubble<'a> {
    event_id: String,
    title: &'a str,
    body: &'a str,
    source: &'a str,
    severity: magpie_core::Severity,
    attempt: u32,
}

/// Delivers by emitting to the pet window. Fails (and is recorded as a failed
/// delivery) when the pet window is closed, rather than pretending it was seen.
pub struct PetChannel {
    app: AppHandle,
}

impl PetChannel {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl Channel for PetChannel {
    fn id(&self) -> &'static str {
        PET
    }

    fn reach(&self) -> Reach {
        Reach::Local
    }

    fn deliver<'a>(&'a self, msg: &'a Message) -> BoxFuture<'a, Result<(), ChannelError>> {
        Box::pin(async move {
            let visible = self
                .app
                .get_webview_window(LABEL)
                .is_some_and(|w| w.is_visible().unwrap_or(false));
            if !visible {
                return Err(ChannelError::Permanent("桌宠已隐藏".into()));
            }
            let bubble = Bubble {
                event_id: msg.event_id.to_string(),
                title: &msg.title,
                body: &msg.body,
                source: &msg.source,
                severity: msg.severity,
                attempt: msg.attempt,
            };
            self.app
                .emit_to(LABEL, MESSAGE_EVENT, bubble)
                .map_err(|e| ChannelError::Transient(format!("pet: {e}")))
        })
    }
}
