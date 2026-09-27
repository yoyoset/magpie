//! Generic webhook: POST the message as JSON to a URL the user configured.

use std::sync::Arc;

use magpie_core::Reach;
use serde_json::json;

use crate::http::HttpClient;
use crate::text::truncate;
use crate::{classify, BoxFuture, Channel, ChannelError, Message};

pub const SECRET_HEADER: &str = "X-Magpie-Secret";

pub struct Webhook {
    http: Arc<dyn HttpClient>,
    id: String,
    url: String,
    /// Sent as `X-Magpie-Secret` so the receiver can reject forged calls.
    secret: Option<String>,
}

impl std::fmt::Debug for Webhook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Webhook")
            .field("id", &self.id)
            .field("url", &self.url)
            .field("secret", &self.secret.as_ref().map(|_| "<redacted>"))
            .finish_non_exhaustive()
    }
}

impl Webhook {
    /// `id` lets several webhooks coexist (`webhook:home-assistant`, …).
    pub fn new(http: Arc<dyn HttpClient>, id: String, url: String, secret: Option<String>) -> Self {
        Self {
            http,
            id,
            url,
            secret,
        }
    }
}

impl Channel for Webhook {
    fn id(&self) -> &str {
        &self.id
    }

    fn reach(&self) -> Reach {
        Reach::Remote
    }

    fn deliver<'a>(&'a self, msg: &'a Message) -> BoxFuture<'a, Result<(), ChannelError>> {
        Box::pin(async move {
            let body = json!({
                "event_id": msg.event_id,
                "source": msg.source,
                "title": msg.title,
                "body": msg.body,
                "severity": msg.severity,
                "tier": msg.tier,
                "attempt": msg.attempt,
            });
            let headers: Vec<(&str, String)> = self
                .secret
                .iter()
                .map(|s| (SECRET_HEADER, s.clone()))
                .collect();
            let res = self.http.post_json(&self.url, &headers, &body).await?;
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
    use magpie_core::{EventId, Severity, Tier};

    #[tokio::test]
    async fn posts_message_with_secret_header() {
        let http = Arc::new(FakeHttp::replying(204, ""));
        let ch = Webhook::new(
            Arc::clone(&http) as Arc<dyn HttpClient>,
            "webhook:ha".into(),
            "http://ha.local/hook".into(),
            Some("s3cret".into()),
        );
        let m = Message {
            event_id: EventId::new(),
            source: "cli".into(),
            title: "t".into(),
            body: "b".into(),
            severity: Severity::Urgent,
            tier: Tier::Phone,
            attempt: 0,
        };
        ch.deliver(&m).await.unwrap();
        let req = http.only_request();
        assert_eq!(req.url, "http://ha.local/hook");
        assert_eq!(req.body["severity"], "urgent");
        assert_eq!(req.body["tier"], "phone");
        assert_eq!(req.body["event_id"], m.event_id.0.to_string());
        assert_eq!(
            req.headers,
            vec![(SECRET_HEADER.to_owned(), "s3cret".to_owned())]
        );
        assert_eq!(ch.id(), "webhook:ha");
        assert!(!format!("{ch:?}").contains("s3cret"));
    }

    #[tokio::test]
    async fn network_failure_passes_through() {
        let http = Arc::new(FakeHttp::failing(ChannelError::Transient(
            "connection refused".into(),
        )));
        let ch = Webhook::new(
            Arc::clone(&http) as Arc<dyn HttpClient>,
            "w".into(),
            "http://x".into(),
            None,
        );
        let m = Message {
            event_id: EventId::new(),
            source: "cli".into(),
            title: "t".into(),
            body: String::new(),
            severity: Severity::Normal,
            tier: Tier::Phone,
            attempt: 0,
        };
        assert_eq!(
            ch.deliver(&m).await,
            Err(ChannelError::Transient("connection refused".into()))
        );
    }
}
