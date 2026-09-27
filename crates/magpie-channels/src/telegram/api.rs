//! Thin Telegram Bot API client: every method is a JSON POST that answers
//! `{"ok": true, "result": …}` or `{"ok": false, "description": …}`.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::http::HttpClient;
use crate::text::truncate;
use crate::{classify, ChannelError};

pub const DEFAULT_API_BASE: &str = "https://api.telegram.org";

#[derive(Clone)]
pub struct TelegramApi {
    http: Arc<dyn HttpClient>,
    api_base: String,
    token: String,
}

impl std::fmt::Debug for TelegramApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramApi")
            .field("api_base", &self.api_base)
            .field("token", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl TelegramApi {
    pub fn new(http: Arc<dyn HttpClient>, token: String) -> Self {
        Self::with_api_base(http, DEFAULT_API_BASE.into(), token)
    }

    pub fn with_api_base(http: Arc<dyn HttpClient>, api_base: String, token: String) -> Self {
        Self {
            http,
            api_base,
            token,
        }
    }

    /// Calls `method` and returns its `result`.
    pub async fn call(&self, method: &str, body: &Value) -> Result<Value, ChannelError> {
        let url = format!("{}/bot{}/{method}", self.api_base, self.token);
        let res = self.http.post_json(&url, &[], body).await?;
        let parsed: Option<Value> = serde_json::from_str(&res.body).ok();
        let ok = parsed.as_ref().and_then(|v| v["ok"].as_bool()) == Some(true);
        if (200..300).contains(&res.status) && ok {
            return Ok(parsed.map_or(Value::Null, |mut v| v["result"].take()));
        }
        let detail = parsed
            .as_ref()
            .and_then(|v| v["description"].as_str().map(str::to_owned))
            .unwrap_or_else(|| truncate(res.body.trim(), 200));
        // A 200 with ok:false is still a failure; treat it like a client error.
        let status = if (200..300).contains(&res.status) {
            400
        } else {
            res.status
        };
        Err(classify(status, &detail))
    }

    /// Long-polls for updates after `offset`, waiting up to `timeout_secs`.
    pub async fn get_updates(
        &self,
        offset: i64,
        timeout_secs: u32,
    ) -> Result<Vec<Value>, ChannelError> {
        let body = json!({
            "offset": offset,
            "timeout": timeout_secs,
            "allowed_updates": ["message", "callback_query"],
        });
        match self.call("getUpdates", &body).await? {
            Value::Array(updates) => Ok(updates),
            other => Err(ChannelError::Permanent(format!(
                "getUpdates: expected an array, got {other}"
            ))),
        }
    }

    /// Stops the spinner on a tapped button and shows `text` as a small toast.
    pub async fn answer_callback(&self, callback_id: &str, text: &str) -> Result<(), ChannelError> {
        self.call(
            "answerCallbackQuery",
            &json!({ "callback_query_id": callback_id, "text": text }),
        )
        .await
        .map(drop)
    }

    /// Removes the inline keyboard so a handled message can't be answered twice.
    pub async fn clear_buttons(&self, chat_id: i64, message_id: i64) -> Result<(), ChannelError> {
        self.call(
            "editMessageReplyMarkup",
            &json!({ "chat_id": chat_id, "message_id": message_id, "reply_markup": { "inline_keyboard": [] } }),
        )
        .await
        .map(drop)
    }

    pub async fn send_text(&self, chat_id: i64, text: &str) -> Result<(), ChannelError> {
        self.call("sendMessage", &json!({ "chat_id": chat_id, "text": text }))
            .await
            .map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;

    fn api(http: &Arc<FakeHttp>) -> TelegramApi {
        TelegramApi::with_api_base(
            Arc::clone(http) as Arc<dyn HttpClient>,
            "https://tg.test".into(),
            "123:SECRET".into(),
        )
    }

    #[tokio::test]
    async fn get_updates_returns_result_array() {
        let http = Arc::new(FakeHttp::replying(
            200,
            r#"{"ok":true,"result":[{"update_id":7}]}"#,
        ));
        let updates = api(&http).get_updates(5, 25).await.unwrap();
        assert_eq!(updates.len(), 1);
        let req = http.only_request();
        assert_eq!(req.url, "https://tg.test/bot123:SECRET/getUpdates");
        assert_eq!(req.body["offset"], 5);
        assert_eq!(req.body["timeout"], 25);
    }

    #[tokio::test]
    async fn ok_false_with_200_is_an_error() {
        let http = Arc::new(FakeHttp::replying(
            200,
            r#"{"ok":false,"description":"Bad Request: query is too old"}"#,
        ));
        assert_eq!(
            api(&http).answer_callback("x", "hi").await,
            Err(ChannelError::Permanent(
                "HTTP 400: Bad Request: query is too old".into()
            ))
        );
    }

    #[tokio::test]
    async fn clear_buttons_sends_empty_keyboard() {
        let http = Arc::new(FakeHttp::replying(200, r#"{"ok":true,"result":true}"#));
        api(&http).clear_buttons(42, 9).await.unwrap();
        let req = http.only_request();
        assert!(req.url.ends_with("/editMessageReplyMarkup"));
        assert_eq!(req.body["reply_markup"]["inline_keyboard"], json!([]));
    }

    #[test]
    fn debug_hides_token() {
        let http = Arc::new(FakeHttp::replying(200, ""));
        assert!(!format!("{:?}", api(&http)).contains("SECRET"));
    }
}
