//! Builds the channel list from settings + credential store.
//!
//! A configured channel whose secret is missing is left out and reported as a
//! warning, which `services` turns into an inbox event: a broken phone channel
//! must be visible, not silently absent.

use std::sync::Arc;
use std::time::Duration;

use magpie_channels::http::{HttpClient, ReqwestHttp};
use magpie_channels::ntfy::Ntfy;
use magpie_channels::telegram::{Telegram, TelegramApi, DEFAULT_API_BASE};
use magpie_channels::webhook::Webhook;
use magpie_channels::Channel;
use magpie_config::secrets::{OsKeyring, SecretStore};
use magpie_config::settings::{secret_keys, Settings};
use tauri::AppHandle;

use crate::pet::PetChannel;
use crate::toast::Toast;

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

pub struct Built {
    pub channels: Vec<Arc<dyn Channel>>,
    pub warnings: Vec<String>,
    /// Set when Telegram is fully configured: the poller for replies from the phone.
    pub telegram_inbox: Option<(TelegramApi, i64)>,
}

pub fn build(app: &AppHandle, settings: &Settings) -> Built {
    let mut channels: Vec<Arc<dyn Channel>> = Vec::new();
    let mut warnings = Vec::new();
    let mut telegram_inbox = None;
    let http: Arc<dyn HttpClient> = Arc::new(ReqwestHttp::new(HTTP_TIMEOUT));
    let secret = |key: &str, warnings: &mut Vec<String>| match OsKeyring.get(key) {
        Ok(v) => v.filter(|s| !s.is_empty()),
        Err(e) => {
            warnings.push(format!("读取凭据 `{key}` 失败：{e}"));
            None
        }
    };

    if settings.desktop.toast {
        channels.push(Arc::new(Toast::new(app.clone())));
    }
    if settings.desktop.pet {
        channels.push(Arc::new(PetChannel::new(app.clone())));
    }

    if let Some(t) = &settings.telegram {
        match secret(secret_keys::TELEGRAM_BOT_TOKEN, &mut warnings) {
            Some(token) => {
                // Long polls hold the connection open for POLL_SECS; give it room.
                let poll_http: Arc<dyn HttpClient> = Arc::new(ReqwestHttp::new(
                    Duration::from_secs(u64::from(crate::telegram_inbox::POLL_SECS) + 15),
                ));
                let base = t
                    .api_base
                    .clone()
                    .unwrap_or_else(|| DEFAULT_API_BASE.to_owned());
                telegram_inbox = Some((
                    TelegramApi::with_api_base(poll_http, base.clone(), token.clone()),
                    t.chat_id,
                ));
                channels.push(Arc::new(Telegram::with_api_base(
                    Arc::clone(&http),
                    base,
                    token,
                    t.chat_id,
                )));
            }
            None => warnings.push(format!(
                "配置了 [telegram] 但凭据管理器里没有 `{}`，Telegram 渠道未启用。",
                secret_keys::TELEGRAM_BOT_TOKEN
            )),
        }
    }
    if let Some(n) = &settings.ntfy {
        let token = secret(secret_keys::NTFY_TOKEN, &mut warnings);
        channels.push(Arc::new(Ntfy::new(
            Arc::clone(&http),
            &n.server,
            n.topic.clone(),
            token,
        )));
    }
    for w in &settings.webhooks {
        let s = secret(&secret_keys::webhook_secret(&w.id), &mut warnings);
        channels.push(Arc::new(Webhook::new(
            Arc::clone(&http),
            w.channel_id(),
            w.url.clone(),
            s,
        )));
    }
    Built {
        channels,
        warnings,
        telegram_inbox,
    }
}
