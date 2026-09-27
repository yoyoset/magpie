//! ntfy (<https://ntfy.sh> or self-hosted): JSON publishing.
//!
//! JSON, not the header API: HTTP header values must be ASCII, so a Chinese
//! title in a `Title:` header would need RFC 2047 encoding. The JSON body has
//! no such limit.

use std::sync::Arc;

use magpie_core::{Reach, Severity};
use serde_json::json;

use crate::http::HttpClient;
use crate::text::{repeat_note, truncate};
use crate::{classify, BoxFuture, Channel, ChannelError, Message};

pub const DEFAULT_SERVER: &str = "https://ntfy.sh";
const MAX_BODY_CHARS: usize = 3000;

pub struct Ntfy {
    http: Arc<dyn HttpClient>,
    server: String,
    topic: String,
    /// Access token for protected topics.
    token: Option<String>,
}

impl std::fmt::Debug for Ntfy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ntfy")
            .field("server", &self.server)
            .field("topic", &self.topic)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .finish_non_exhaustive()
    }
}

impl Ntfy {
    pub fn new(
        http: Arc<dyn HttpClient>,
        server: &str,
        topic: String,
        token: Option<String>,
    ) -> Self {
        Self {
            http,
            server: server.trim_end_matches('/').to_owned(),
            topic,
            token,
        }
    }
}

/// ntfy priorities: 1 min … 5 max. 5 overrides the phone's Do Not Disturb.
fn priority(s: Severity) -> u8 {
    match s {
        Severity::Info => 2,
        Severity::Normal => 3,
        Severity::Urgent => 4,
        Severity::Critical => 5,
    }
}

impl Channel for Ntfy {
    fn id(&self) -> &'static str {
        "ntfy"
    }

    fn reach(&self) -> Reach {
        Reach::Remote
    }

    fn deliver<'a>(&'a self, msg: &'a Message) -> BoxFuture<'a, Result<(), ChannelError>> {
        Box::pin(async move {
            let mut message = truncate(&msg.body, MAX_BODY_CHARS);
            if message.is_empty() {
                // ntfy shows "triggered" for an empty message; the source is more useful.
                message.clone_from(&msg.source);
            }
            let title = match repeat_note(msg.attempt) {
                Some(n) => format!("{}（{n}）", msg.title),
                None => msg.title.clone(),
            };
            let body = json!({
                "topic": self.topic,
                "title": title,
                "message": message,
                "priority": priority(msg.severity),
                "tags": [msg.source],
            });
            let headers: Vec<(&str, String)> = self
                .token
                .iter()
                .map(|t| ("Authorization", format!("Bearer {t}")))
                .collect();
            let res = self.http.post_json(&self.server, &headers, &body).await?;
            if (200..300).contains(&res.status) {
                Ok(())
            } else {
                Err(classify(res.status, &truncate(res.body.trim(), 200)))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;
    use magpie_core::{EventId, Tier};

    fn msg(severity: Severity, body: &str, attempt: u32) -> Message {
        Message {
            event_id: EventId::new(),
            source: "scheduled:viiyd".into(),
            title: "发小红书".into(),
            body: body.into(),
            severity,
            tier: Tier::Phone,
            attempt,
        }
    }

    #[tokio::test]
    async fn publishes_json_to_server_root() {
        let http = Arc::new(FakeHttp::replying(200, "{}"));
        let ch = Ntfy::new(
            Arc::clone(&http) as Arc<dyn HttpClient>,
            "https://ntfy.example/",
            "magpie-abc".into(),
            Some("tk_secret".into()),
        );
        ch.deliver(&msg(Severity::Critical, "今晚 20:00", 2))
            .await
            .unwrap();

        let req = http.only_request();
        assert_eq!(
            req.url, "https://ntfy.example",
            "JSON publish goes to the root"
        );
        assert_eq!(req.body["topic"], "magpie-abc");
        assert_eq!(req.body["title"], "发小红书（第 3 次提醒）");
        assert_eq!(req.body["message"], "今晚 20:00");
        assert_eq!(req.body["priority"], 5);
        assert_eq!(
            req.headers,
            vec![("Authorization".to_owned(), "Bearer tk_secret".to_owned())]
        );
        assert!(!format!("{ch:?}").contains("tk_secret"));
    }

    #[tokio::test]
    async fn empty_body_falls_back_to_source_and_no_token_no_header() {
        let http = Arc::new(FakeHttp::replying(200, "{}"));
        let ch = Ntfy::new(
            Arc::clone(&http) as Arc<dyn HttpClient>,
            DEFAULT_SERVER,
            "t".into(),
            None,
        );
        ch.deliver(&msg(Severity::Normal, "", 0)).await.unwrap();
        let req = http.only_request();
        assert_eq!(req.body["message"], "scheduled:viiyd");
        assert_eq!(req.body["title"], "发小红书");
        assert!(req.headers.is_empty());
    }

    #[tokio::test]
    async fn server_errors_are_transient() {
        let http = Arc::new(FakeHttp::replying(502, "bad gateway"));
        let ch = Ntfy::new(
            Arc::clone(&http) as Arc<dyn HttpClient>,
            DEFAULT_SERVER,
            "t".into(),
            None,
        );
        assert_eq!(
            ch.deliver(&msg(Severity::Normal, "x", 0)).await,
            Err(ChannelError::Transient("HTTP 502: bad gateway".into()))
        );
    }
}
