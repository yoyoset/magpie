//! Remote delivery channels.
//!
//! Every channel implements [`Channel`]; adding one means adding a module, not
//! touching the others. Local channels (toast, pet, takeover) live in the app
//! and implement the same trait. HTTP goes through [`http::HttpClient`] so tests
//! never touch the network.

pub mod http;
pub mod ntfy;
pub mod telegram;
mod text;
pub mod webhook;

use std::future::Future;
use std::pin::Pin;

use magpie_core::{Event, EventId, Reach, Severity, Tier};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What a channel is asked to deliver: an event at a given escalation step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub event_id: EventId,
    pub source: String,
    pub title: String,
    pub body: String,
    pub severity: Severity,
    pub tier: Tier,
    /// 0-based escalation round. Lets a channel say "2nd reminder".
    pub attempt: u32,
}

impl Message {
    pub fn new(event: &Event, tier: Tier, attempt: u32) -> Self {
        Self {
            event_id: event.id,
            source: event.source.as_str().to_owned(),
            title: event.title.clone(),
            body: event.body.clone(),
            severity: event.severity,
            tier,
            attempt,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChannelError {
    /// Worth retrying: network trouble, 5xx, rate limiting.
    #[error("temporary failure: {0}")]
    Transient(String),
    /// Retrying won't help until the configuration changes: bad token, unknown chat.
    #[error("{0}")]
    Permanent(String),
}

pub trait Channel: Send + Sync {
    /// Stable id used in routes and delivery records, e.g. `telegram`.
    fn id(&self) -> &str;
    fn reach(&self) -> Reach;
    fn deliver<'a>(&'a self, msg: &'a Message) -> BoxFuture<'a, Result<(), ChannelError>>;
}

/// Maps an HTTP status to the retry class shared by all HTTP channels.
pub(crate) fn classify(status: u16, detail: &str) -> ChannelError {
    if status == 429 || status >= 500 {
        ChannelError::Transient(format!("HTTP {status}: {detail}"))
    } else {
        ChannelError::Permanent(format!("HTTP {status}: {detail}"))
    }
}
