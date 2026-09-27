//! Windows toast notifications as a Magpie channel.

use magpie_channels::{BoxFuture, Channel, ChannelError, Message};
use magpie_config::settings::TOAST;
use magpie_core::Reach;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

pub struct Toast {
    app: AppHandle,
}

impl Toast {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl Channel for Toast {
    fn id(&self) -> &'static str {
        TOAST
    }

    fn reach(&self) -> Reach {
        Reach::Local
    }

    fn deliver<'a>(&'a self, msg: &'a Message) -> BoxFuture<'a, Result<(), ChannelError>> {
        Box::pin(async move {
            let title = match msg.attempt {
                0 => msg.title.clone(),
                n => format!("{}（第 {} 次提醒）", msg.title, n + 1),
            };
            let body = if msg.body.is_empty() {
                msg.source.clone()
            } else {
                msg.body.chars().take(300).collect()
            };
            self.app
                .notification()
                .builder()
                .title(title)
                .body(body)
                .show()
                .map_err(|e| ChannelError::Transient(format!("toast: {e}")))
        })
    }
}
