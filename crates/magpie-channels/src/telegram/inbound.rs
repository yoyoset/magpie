//! Interpreting what comes back from the phone. Pure: JSON in, intent out.

use magpie_core::EventId;
use serde_json::Value;

use super::{decode_callback, CallbackAction};

/// What the human meant by an update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inbound {
    /// Tapped one of our inline buttons.
    Button {
        callback_id: String,
        message_id: i64,
        event: EventId,
        action: CallbackAction,
    },
    /// A button whose data we can't decode (e.g. from an older build). Still
    /// needs answering, or the phone shows a spinner forever.
    UnknownButton { callback_id: String },
    /// Replied (Telegram "reply") to one of our messages; `event_ref` is the
    /// short id quoted from it, e.g. `6bbe86f0`.
    Reply { event_ref: String, text: String },
    /// Text that doesn't reply to any of our messages.
    Stray,
}

/// `#6bbe86f0`: printed in every outgoing message so a reply can be traced back.
pub fn short_ref(id: EventId) -> String {
    format!("#{}", &id.0.simple().to_string()[..8])
}

pub fn update_id(update: &Value) -> Option<i64> {
    update["update_id"].as_i64()
}

/// Maps an update to an intent, or `None` to ignore it. Updates from any chat
/// other than `allowed_chat` are ignored: the bot may be found and messaged by
/// strangers, and they must not be able to ack anything.
pub fn interpret(update: &Value, allowed_chat: i64) -> Option<Inbound> {
    if let Some(cb) = update.get("callback_query") {
        let chat = cb["message"]["chat"]["id"].as_i64()?;
        if chat != allowed_chat {
            return None;
        }
        let callback_id = cb["id"].as_str()?.to_owned();
        let message_id = cb["message"]["message_id"].as_i64().unwrap_or_default();
        return Some(match cb["data"].as_str().and_then(decode_callback) {
            Some((event, action)) => Inbound::Button {
                callback_id,
                message_id,
                event,
                action,
            },
            None => Inbound::UnknownButton { callback_id },
        });
    }

    let msg = update.get("message")?;
    if msg["chat"]["id"].as_i64()? != allowed_chat {
        return None;
    }
    let text = msg["text"].as_str()?.trim();
    if text.is_empty() || text.starts_with('/') {
        return None; // commands like /start are not answers
    }
    match msg["reply_to_message"]["text"].as_str().and_then(find_ref) {
        Some(event_ref) => Some(Inbound::Reply {
            event_ref,
            text: text.to_owned(),
        }),
        None => Some(Inbound::Stray),
    }
}

/// Finds the first `#` followed by exactly 8 lowercase hex chars.
fn find_ref(text: &str) -> Option<String> {
    text.match_indices('#').find_map(|(i, _)| {
        let rest = &text[i + 1..];
        let candidate: String = rest.chars().take(9).collect();
        let hex: String = candidate
            .chars()
            .take_while(char::is_ascii_hexdigit)
            .collect();
        (hex.len() == 8
            && candidate
                .chars()
                .nth(8)
                .is_none_or(|c| !c.is_ascii_hexdigit()))
        .then(|| hex.to_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CHAT: i64 = 42;

    fn event() -> EventId {
        EventId(uuid::Uuid::parse_str("6bbe86f0-1111-4222-8333-444455556666").unwrap())
    }

    #[test]
    fn short_ref_is_hash_plus_8_hex() {
        assert_eq!(short_ref(event()), "#6bbe86f0");
    }

    #[test]
    fn button_press_from_our_chat() {
        let u = json!({"update_id": 10, "callback_query": {
            "id": "cb1", "data": format!("s30:{}", event().0),
            "message": {"message_id": 5, "chat": {"id": CHAT}}
        }});
        assert_eq!(update_id(&u), Some(10));
        assert_eq!(
            interpret(&u, CHAT),
            Some(Inbound::Button {
                callback_id: "cb1".into(),
                message_id: 5,
                event: event(),
                action: CallbackAction::Snooze { minutes: 30 }
            })
        );
    }

    #[test]
    fn strangers_are_ignored() {
        let cb = json!({"callback_query": {"id": "x", "data": format!("a:{}", event().0),
            "message": {"message_id": 1, "chat": {"id": 999}}}});
        let msg = json!({"message": {"chat": {"id": 999}, "text": "发",
            "reply_to_message": {"text": "#6bbe86f0"}}});
        assert_eq!(interpret(&cb, CHAT), None);
        assert_eq!(interpret(&msg, CHAT), None);
    }

    #[test]
    fn garbled_button_still_needs_an_answer() {
        let u = json!({"callback_query": {"id": "cb2", "data": "zz",
            "message": {"message_id": 1, "chat": {"id": CHAT}}}});
        assert_eq!(
            interpret(&u, CHAT),
            Some(Inbound::UnknownButton {
                callback_id: "cb2".into()
            })
        );
    }

    #[test]
    fn reply_is_traced_through_the_quoted_short_id() {
        let u = json!({"message": {"chat": {"id": CHAT}, "text": " 发 ",
            "reply_to_message": {"text": "🟠 Claude 在等你\nclaude-code · #6bbe86f0\n\n正文"}}});
        assert_eq!(
            interpret(&u, CHAT),
            Some(Inbound::Reply {
                event_ref: "6bbe86f0".into(),
                text: "发".into()
            })
        );
    }

    #[test]
    fn plain_text_and_commands() {
        let stray = json!({"message": {"chat": {"id": CHAT}, "text": "在吗"}});
        assert_eq!(interpret(&stray, CHAT), Some(Inbound::Stray));
        let start = json!({"message": {"chat": {"id": CHAT}, "text": "/start"}});
        assert_eq!(interpret(&start, CHAT), None);
    }

    #[test]
    fn find_ref_requires_exactly_eight_hex() {
        assert_eq!(find_ref("x #6bbe86f0 y").as_deref(), Some("6bbe86f0"));
        assert_eq!(find_ref("#6BBE86F0").as_deref(), Some("6bbe86f0"));
        assert_eq!(find_ref("#6bbe86f"), None, "too short");
        assert_eq!(find_ref("#6bbe86f0a"), None, "too long");
        assert_eq!(
            find_ref("#话题 #6bbe86f0").as_deref(),
            Some("6bbe86f0"),
            "skips hashtags"
        );
    }
}
