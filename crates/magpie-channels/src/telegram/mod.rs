//! Telegram: the outbound channel (`sendMessage` with 确认 / 稍后 / 忽略
//! buttons) plus the pieces the two-way poller needs.
//!
//! - [`api`]: the Bot API calls.
//! - [`inbound`]: turning updates from the phone into intents.
//! - Callback data is compact (≤ 64 bytes, Telegram's limit); every message
//!   also carries a short `#abcd1234` id so a text reply can be traced back.

pub mod api;
pub mod inbound;

use std::sync::Arc;

use magpie_core::{EventId, Reach};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::http::HttpClient;
use crate::text::{repeat_note, severity_badge, truncate};
use crate::{BoxFuture, Channel, ChannelError, Message};

pub use api::{TelegramApi, DEFAULT_API_BASE};
/// Telegram rejects texts over 4096 chars; leave room for title and markup.
const MAX_BODY_CHARS: usize = 3000;
pub const SNOOZE_MINUTES: u16 = 30;

pub struct Telegram {
    api: TelegramApi,
    chat_id: i64,
}

impl std::fmt::Debug for Telegram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Telegram")
            .field("chat_id", &self.chat_id)
            .field("token", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl Telegram {
    pub fn new(http: Arc<dyn HttpClient>, token: String, chat_id: i64) -> Self {
        Self::with_api_base(http, DEFAULT_API_BASE.into(), token, chat_id)
    }

    pub fn with_api_base(
        http: Arc<dyn HttpClient>,
        api_base: String,
        token: String,
        chat_id: i64,
    ) -> Self {
        Self {
            api: TelegramApi::with_api_base(http, api_base, token),
            chat_id,
        }
    }

    fn payload(&self, msg: &Message) -> Value {
        let mut meta = vec![escape(&msg.source)];
        meta.extend(repeat_note(msg.attempt));
        meta.push(inbound::short_ref(msg.event_id));
        let mut text = format!(
            "{} <b>{}</b>\n<i>{}</i>",
            severity_badge(msg.severity),
            escape(&msg.title),
            meta.join(" · ")
        );
        if !msg.body.is_empty() {
            text.push_str("\n\n");
            text.push_str(&escape(&truncate(&msg.body, MAX_BODY_CHARS)));
        }
        let id = msg.event_id;
        json!({
            "chat_id": self.chat_id,
            "text": text,
            "parse_mode": "HTML",
            "disable_web_page_preview": true,
            "reply_markup": { "inline_keyboard": [[
                { "text": "✅ 确认", "callback_data": encode_callback(id, CallbackAction::Ack) },
                { "text": format!("⏰ {SNOOZE_MINUTES} 分钟后"),
                  "callback_data": encode_callback(id, CallbackAction::Snooze { minutes: SNOOZE_MINUTES }) },
                { "text": "✖ 忽略", "callback_data": encode_callback(id, CallbackAction::Dismiss) },
            ]]}
        })
    }
}

impl Channel for Telegram {
    fn id(&self) -> &'static str {
        "telegram"
    }

    fn reach(&self) -> Reach {
        Reach::Remote
    }

    fn deliver<'a>(&'a self, msg: &'a Message) -> BoxFuture<'a, Result<(), ChannelError>> {
        Box::pin(async move {
            self.api
                .call("sendMessage", &self.payload(msg))
                .await
                .map(drop)
        })
    }
}

/// Minimal HTML escaping for Telegram's HTML parse mode.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackAction {
    Ack,
    Snooze { minutes: u16 },
    Dismiss,
}

/// `a:<uuid>`, `s30:<uuid>`, `d:<uuid>` — at most 43 bytes.
pub fn encode_callback(id: EventId, action: CallbackAction) -> String {
    match action {
        CallbackAction::Ack => format!("a:{id}"),
        CallbackAction::Snooze { minutes } => format!("s{minutes}:{id}"),
        CallbackAction::Dismiss => format!("d:{id}"),
    }
}

pub fn decode_callback(data: &str) -> Option<(EventId, CallbackAction)> {
    let (tag, id) = data.split_once(':')?;
    let id = EventId(Uuid::parse_str(id).ok()?);
    let action = match tag {
        "a" => CallbackAction::Ack,
        "d" => CallbackAction::Dismiss,
        t => CallbackAction::Snooze {
            minutes: t.strip_prefix('s')?.parse().ok().filter(|m| *m > 0)?,
        },
    };
    Some((id, action))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;
    use magpie_core::{Severity, Tier};

    fn msg(attempt: u32) -> Message {
        Message {
            event_id: EventId::new(),
            source: "claude-code".into(),
            title: "Claude 在等你 <proj>".into(),
            body: "needs permission & approval".into(),
            severity: Severity::Urgent,
            tier: Tier::Phone,
            attempt,
        }
    }

    fn channel(http: &Arc<FakeHttp>) -> Telegram {
        Telegram::with_api_base(
            Arc::clone(http) as Arc<dyn HttpClient>,
            "https://tg.test".into(),
            "123:SECRET".into(),
            42,
        )
    }

    #[tokio::test]
    async fn sends_escaped_html_with_buttons() {
        let http = Arc::new(FakeHttp::replying(200, r#"{"ok":true}"#));
        let m = msg(1);
        channel(&http).deliver(&m).await.unwrap();

        let req = http.only_request();
        assert_eq!(req.url, "https://tg.test/bot123:SECRET/sendMessage");
        assert_eq!(req.body["chat_id"], 42);
        assert_eq!(req.body["parse_mode"], "HTML");
        let text = req.body["text"].as_str().unwrap();
        assert!(text.contains("<b>Claude 在等你 &lt;proj&gt;</b>"), "{text}");
        assert!(text.contains("claude-code · 第 2 次提醒"), "{text}");
        assert!(text.contains("needs permission &amp; approval"), "{text}");

        let buttons = &req.body["reply_markup"]["inline_keyboard"][0];
        let data: Vec<&str> = buttons
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["callback_data"].as_str().unwrap())
            .collect();
        assert_eq!(
            decode_callback(data[0]),
            Some((m.event_id, CallbackAction::Ack))
        );
        assert_eq!(
            decode_callback(data[1]),
            Some((m.event_id, CallbackAction::Snooze { minutes: 30 }))
        );
        assert_eq!(
            decode_callback(data[2]),
            Some((m.event_id, CallbackAction::Dismiss))
        );
        assert!(
            data.iter().all(|d| d.len() <= 64),
            "Telegram callback_data limit"
        );
    }

    #[tokio::test]
    async fn api_errors_are_classified_with_description() {
        let http = Arc::new(FakeHttp::replying(
            400,
            r#"{"ok":false,"error_code":400,"description":"Bad Request: chat not found"}"#,
        ));
        assert_eq!(
            channel(&http).deliver(&msg(0)).await,
            Err(ChannelError::Permanent(
                "HTTP 400: Bad Request: chat not found".into()
            ))
        );

        let http = Arc::new(FakeHttp::replying(
            429,
            r#"{"ok":false,"description":"Too Many Requests: retry after 5"}"#,
        ));
        assert!(matches!(
            channel(&http).deliver(&msg(0)).await,
            Err(ChannelError::Transient(_))
        ));
    }

    #[test]
    fn debug_hides_token() {
        let http = Arc::new(FakeHttp::replying(200, ""));
        assert!(!format!("{:?}", channel(&http)).contains("SECRET"));
    }

    #[test]
    fn callback_rejects_garbage() {
        assert_eq!(decode_callback("x:nope"), None);
        assert_eq!(
            decode_callback("s0:00000000-0000-0000-0000-000000000000"),
            None
        );
        assert_eq!(
            decode_callback("sabc:00000000-0000-0000-0000-000000000000"),
            None
        );
        assert_eq!(decode_callback("a"), None);
    }
}
