//! `config.toml`: everything the user can tune, in one strictly-parsed file.
//!
//! Every table uses `deny_unknown_fields`: a misspelled key is a load error,
//! never a silently ignored setting. Secrets are not here; they live in the
//! credential store under the keys in [`secret_keys`].

use std::collections::BTreeSet;

use magpie_core::{EscalationPolicy, Route};
use serde::{Deserialize, Serialize};

use crate::paths::Paths;
use crate::{ConfigError, Result};

pub const DEFAULT_PORT: u16 = 47_615;

/// Credential-store keys for channel secrets.
pub mod secret_keys {
    pub const TELEGRAM_BOT_TOKEN: &str = "telegram-bot-token";
    pub const NTFY_TOKEN: &str = "ntfy-token";

    pub fn webhook_secret(id: &str) -> String {
        format!("webhook-secret:{id}")
    }
}

/// Built-in local channel ids, implemented by the app.
pub const TOAST: &str = "toast";
pub const PET: &str = "pet";
pub const TAKEOVER: &str = "takeover";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub api: Api,
    pub escalation: EscalationPolicy,
    pub desktop: Desktop,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telegram: Option<Telegram>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ntfy: Option<Ntfy>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub webhooks: Vec<Webhook>,
    /// First match wins; no match = all channels (see `magpie_core::routing`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<Route>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Api {
    /// 0 lets the OS pick a free port; clients find it through `endpoint.json`.
    pub port: u16,
}

impl Default for Api {
    fn default() -> Self {
        Self { port: DEFAULT_PORT }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Desktop {
    pub toast: bool,
    pub pet: bool,
    /// Full-screen alert with sound for critical events, until answered.
    pub takeover: bool,
    pub autostart: bool,
}

impl Default for Desktop {
    fn default() -> Self {
        Self {
            toast: true,
            pet: true,
            takeover: true,
            autostart: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Telegram {
    /// Your chat with the bot. Bot token: credential store, `telegram-bot-token`.
    pub chat_id: i64,
    /// Only for a self-hosted Bot API server (or tests); defaults to api.telegram.org.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ntfy {
    #[serde(default = "default_ntfy_server")]
    pub server: String,
    pub topic: String,
}

fn default_ntfy_server() -> String {
    "https://ntfy.sh".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Webhook {
    pub id: String,
    pub url: String,
}

impl Webhook {
    pub fn channel_id(&self) -> String {
        format!("webhook:{}", self.id)
    }
}

impl Settings {
    /// Channel ids this configuration enables, in delivery order.
    pub fn channel_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        if self.desktop.toast {
            ids.push(TOAST.to_owned());
        }
        if self.desktop.pet {
            ids.push(PET.to_owned());
        }
        if self.desktop.takeover {
            ids.push(TAKEOVER.to_owned());
        }
        if self.telegram.is_some() {
            ids.push("telegram".to_owned());
        }
        if self.ntfy.is_some() {
            ids.push("ntfy".to_owned());
        }
        ids.extend(self.webhooks.iter().map(Webhook::channel_id));
        ids
    }

    /// Checks cross-field rules serde cannot express. Collects every problem
    /// instead of stopping at the first, so one edit fixes them all.
    pub fn validate(&self) -> std::result::Result<(), Vec<String>> {
        let mut problems = Vec::new();
        let e = &self.escalation;
        if e.renotify_after_secs < 60 {
            problems.push(format!(
                "escalation.renotify_after_secs is {}, minimum is 60",
                e.renotify_after_secs
            ));
        }
        if e.max_attempts == 0 {
            problems.push("escalation.max_attempts must be at least 1".into());
        }
        if let Some(t) = &self.telegram {
            if t.chat_id == 0 {
                problems.push("telegram.chat_id must not be 0".into());
            }
            if let Some(base) = &t.api_base {
                if !is_http_url(base) {
                    problems.push(format!("telegram.api_base `{base}` is not an http(s) URL"));
                }
            }
        }
        if let Some(n) = &self.ntfy {
            if n.topic.trim().is_empty() {
                problems.push("ntfy.topic must not be empty".into());
            }
            if !is_http_url(&n.server) {
                problems.push(format!("ntfy.server `{}` is not an http(s) URL", n.server));
            }
        }
        let mut seen = BTreeSet::new();
        for w in &self.webhooks {
            if w.id.trim().is_empty() {
                problems.push("webhooks: id must not be empty".into());
            } else if !seen.insert(w.id.as_str()) {
                problems.push(format!("webhooks: duplicate id `{}`", w.id));
            }
            if !is_http_url(&w.url) {
                problems.push(format!(
                    "webhooks.{}: `{}` is not an http(s) URL",
                    w.id, w.url
                ));
            }
        }
        let known: BTreeSet<String> = self.channel_ids().into_iter().collect();
        for r in &self.routes {
            for c in &r.channels {
                if !known.contains(c) {
                    problems.push(format!(
                        "route `{}` uses channel `{c}`, which is not enabled (enabled: {})",
                        r.name,
                        known.iter().cloned().collect::<Vec<_>>().join(", ")
                    ));
                }
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems)
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        let settings: Self = toml::from_str(text).map_err(|e| ConfigError::Malformed {
            what: "config.toml".into(),
            detail: e.to_string(),
        })?;
        settings
            .validate()
            .map_err(|problems| ConfigError::Malformed {
                what: "config.toml".into(),
                detail: problems.join("; "),
            })?;
        Ok(settings)
    }

    /// Missing file = defaults. Anything else unreadable or invalid is an error:
    /// running on half a config is worse than refusing to start.
    pub fn load(paths: &Paths) -> Result<Self> {
        let file = paths.config_file();
        match std::fs::read_to_string(&file) {
            Ok(text) => Self::parse(text.strip_prefix('\u{feff}').unwrap_or(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Io {
                what: format!("read {}", file.display()),
                source,
            }),
        }
    }

    /// Writes the commented template on first run so there is something to edit.
    pub fn write_template_if_missing(paths: &Paths) -> Result<bool> {
        let file = paths.config_file();
        if file.exists() {
            return Ok(false);
        }
        paths.ensure_root()?;
        std::fs::write(&file, TEMPLATE).map_err(|source| ConfigError::Io {
            what: format!("write {}", file.display()),
            source,
        })?;
        Ok(true)
    }
}

fn is_http_url(s: &str) -> bool {
    let s = s.trim();
    (s.starts_with("http://") || s.starts_with("https://")) && s.len() > "https://".len()
}

/// First-run `config.toml`. Every value shown is the default.
pub const TEMPLATE: &str = r#"# Magpie 配置。改完重启 Magpie 生效。拼错的键会导致启动报错（故意的）。
# 密钥（Telegram bot token 等）不写这里，存在 Windows 凭据管理器。

[api]
port = 47615            # 0 = 让系统挑空闲端口

[escalation]
renotify_after_secs = 600   # urgent 没回应，隔多久再提醒
phone_from_attempt = 1      # 第几次（从 0 数）开始上手机
max_attempts = 6            # 最多提醒几轮，之后标记为「放弃」

[desktop]
toast = true
pet = true
takeover = true            # critical：全屏 + 提示音，直到回应（Alt+F4 或托盘可关）
autostart = true

# [telegram]
# chat_id = 123456789       # bot token 存凭据管理器 telegram-bot-token

# [ntfy]
# server = "https://ntfy.sh"
# topic = "magpie-换成只有你知道的名字"

# [[webhooks]]
# id = "ha"
# url = "http://homeassistant.local:8123/api/webhook/magpie"

# 路由：第一条匹配的生效；都不匹配 = 所有渠道。channels 为空 = 静音（只进收件箱）。
# [[routes]]
# name = "定时任务直接上手机"
# match = { source_prefix = "scheduled:" }
# channels = ["toast", "pet", "telegram"]
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parses_to_defaults() {
        assert_eq!(Settings::parse(TEMPLATE).unwrap(), Settings::default());
    }

    #[test]
    fn empty_file_is_defaults() {
        let s = Settings::parse("").unwrap();
        assert_eq!(s.api.port, DEFAULT_PORT);
        assert_eq!(s.escalation, EscalationPolicy::default());
        assert_eq!(s.channel_ids(), vec!["toast", "pet", "takeover"]);
    }

    #[test]
    fn misspelled_keys_are_errors_at_every_level() {
        for bad in [
            "[api]\nprot = 1",
            "[escalation]\nrenotify_after = 60",
            "[desktop]\ntoasts = false",
            "[telegram]\nchat = 1",
            "unknown_top_level = 1",
            "[[routes]]\nname = \"r\"\nchannels = []\nmatch = { sorce_prefix = \"x\" }",
        ] {
            let err = Settings::parse(bad).unwrap_err().to_string();
            assert!(err.contains("unknown field"), "{bad:?} -> {err}");
        }
    }

    #[test]
    fn partial_escalation_keeps_other_defaults() {
        let s = Settings::parse("[escalation]\nmax_attempts = 2").unwrap();
        assert_eq!(s.escalation.max_attempts, 2);
        assert_eq!(s.escalation.renotify_after_secs, 600);
    }

    #[test]
    fn full_config_enables_channels_in_order() {
        let s = Settings::parse(
            r#"
            [telegram]
            chat_id = 42
            [ntfy]
            topic = "magpie-x"
            [[webhooks]]
            id = "ha"
            url = "http://ha.local/hook"
            [[routes]]
            name = "scheduled"
            match = { source_prefix = "scheduled:" }
            channels = ["pet", "telegram", "webhook:ha"]
            "#,
        )
        .unwrap();
        assert_eq!(
            s.channel_ids(),
            vec!["toast", "pet", "takeover", "telegram", "ntfy", "webhook:ha"]
        );
        assert_eq!(s.ntfy.unwrap().server, "https://ntfy.sh");
        assert_eq!(
            s.routes[0].matcher.source_prefix.as_deref(),
            Some("scheduled:")
        );
    }

    #[test]
    fn validation_collects_all_problems() {
        let err = Settings::parse(
            r#"
            [escalation]
            renotify_after_secs = 5
            max_attempts = 0
            [[webhooks]]
            id = "a"
            url = "ftp://x"
            [[webhooks]]
            id = "a"
            url = "http://ok"
            [[routes]]
            name = "r"
            channels = ["telegram"]
            "#,
        )
        .unwrap_err()
        .to_string();
        for needle in [
            "renotify_after_secs is 5",
            "max_attempts must be at least 1",
            "`ftp://x` is not an http(s) URL",
            "duplicate id `a`",
            "route `r` uses channel `telegram`, which is not enabled",
        ] {
            assert!(err.contains(needle), "missing `{needle}` in: {err}");
        }
    }

    #[test]
    fn load_missing_is_default_and_bom_is_tolerated() {
        let dir = std::env::temp_dir().join(format!("magpie-settings-{}", uuid::Uuid::new_v4()));
        let paths = Paths::at(&dir);
        assert_eq!(Settings::load(&paths).unwrap(), Settings::default());

        assert!(Settings::write_template_if_missing(&paths).unwrap());
        assert!(
            !Settings::write_template_if_missing(&paths).unwrap(),
            "never overwrites"
        );
        assert_eq!(Settings::load(&paths).unwrap(), Settings::default());

        // Notepad on older Windows saves UTF-8 with a BOM.
        std::fs::write(paths.config_file(), "\u{feff}[api]\nport = 1234\n").unwrap();
        assert_eq!(Settings::load(&paths).unwrap().api.port, 1234);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_roundtrip() {
        let s = Settings {
            telegram: Some(Telegram {
                chat_id: 7,
                api_base: None,
            }),
            ..Settings::default()
        };
        let text = toml::to_string_pretty(&s).unwrap();
        assert_eq!(Settings::parse(&text).unwrap(), s);
    }
}
