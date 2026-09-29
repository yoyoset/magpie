//! The desktop pet: a small transparent always-on-top window, plus the `pet`
//! channel that pops speech bubbles on it.

use magpie_channels::{BoxFuture, Channel, ChannelError, Message};
use magpie_config::settings::PET;
use magpie_core::Reach;
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "pet";
pub const MESSAGE_EVENT: &str = "pet://message";
const WIDTH: f64 = 300.0;
const HEIGHT: f64 = 320.0;
/// Gap between the pet and the screen's work-area edge, in logical pixels.
const MARGIN: f64 = 16.0;

/// How often the cursor is checked against the interactive regions.
const HIT_POLL: Duration = Duration::from_millis(33);

/// Interactive regions `[x, y, w, h]` in the window's logical pixels, reported by
/// the page (the bird, the bubble, the menu). Everything else is click-through.
static HIT_RECTS: Mutex<Vec<[f64; 4]>> = Mutex::new(Vec::new());

pub fn set_hit_rects(rects: Vec<[f64; 4]>) {
    if let Ok(mut r) = HIT_RECTS.lock() {
        *r = rects;
    }
}

fn inside(rects: &[[f64; 4]], x: f64, y: f64) -> bool {
    rects
        .iter()
        .any(|[rx, ry, w, h]| x >= *rx && x < rx + w && y >= *ry && y < ry + h)
}

/// A transparent window still eats clicks over its empty area. Once ignoring
/// cursor events the page gets no hover events either, so the cursor is polled
/// here and the window only takes input while the cursor is over a drawn region.
fn spawn_hit_test(window: tauri::WebviewWindow) {
    std::thread::spawn(move || {
        let mut ignoring: Option<bool> = None;
        loop {
            std::thread::sleep(HIT_POLL);
            let (Ok(cursor), Ok(origin), Ok(scale)) = (
                window.cursor_position(),
                window.inner_position(),
                window.scale_factor(),
            ) else {
                continue;
            };
            let x = (cursor.x - f64::from(origin.x)) / scale;
            let y = (cursor.y - f64::from(origin.y)) / scale;
            let want_ignore = !HIT_RECTS.lock().is_ok_and(|r| inside(&r, x, y));
            if ignoring != Some(want_ignore) {
                if window.set_ignore_cursor_events(want_ignore).is_err() {
                    break; // window is gone
                }
                ignoring = Some(want_ignore);
            }
        }
    });
}

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
    spawn_hit_test(window.clone());
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

#[cfg(test)]
mod tests {
    use super::inside;

    #[test]
    fn hit_test_is_half_open_and_empty_means_click_through() {
        let rects = [[10.0, 20.0, 100.0, 50.0]];
        assert!(inside(&rects, 10.0, 20.0));
        assert!(inside(&rects, 109.9, 69.9));
        assert!(!inside(&rects, 110.0, 30.0));
        assert!(!inside(&rects, 5.0, 30.0));
        assert!(!inside(&[], 10.0, 20.0));
    }
}
