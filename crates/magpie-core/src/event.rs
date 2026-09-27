//! Events: the unit that lands in the inbox.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_TITLE_CHARS: usize = 200;
pub const MAX_BODY_CHARS: usize = 20_000;
pub const MAX_SOURCE_CHARS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EventId(pub Uuid);

impl EventId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// How hard Magpie should try to get a human's attention.
///
/// Ordered: `Info < Normal < Urgent < Critical`, so routing rules can say
/// "at least Urgent".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Recorded in the inbox, no interruption.
    Info,
    /// One notification, no escalation.
    Normal,
    /// Re-notifies until acknowledged; may move to the phone.
    Urgent,
    /// Full-screen takeover and every channel until acknowledged.
    Critical,
}

impl Severity {
    /// Whether an unacknowledged event of this severity keeps nagging.
    pub fn escalates(self) -> bool {
        self >= Self::Urgent
    }
}

/// Who produced the event, e.g. `claude-code`, `scheduled:viiyd-weekly-social-prep`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventSource(String);

impl EventSource {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EventSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} is {actual} characters, limit is {limit}")]
    TooLong {
        field: &'static str,
        actual: usize,
        limit: usize,
    },
}

/// Untrusted input as it arrives from the API or CLI.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventDraft {
    pub source: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default = "default_severity")]
    pub severity: Severity,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_severity() -> Severity {
    Severity::Normal
}

impl Default for Severity {
    fn default() -> Self {
        default_severity()
    }
}

/// A validated event. Only constructible through [`EventDraft::validate`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub source: EventSource,
    pub title: String,
    pub body: String,
    pub severity: Severity,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

impl EventDraft {
    /// Trims, checks limits and stamps the event. `now` is injected so this stays pure.
    pub fn validate(self, id: EventId, now: DateTime<Utc>) -> Result<Event, ValidationError> {
        let source = required("source", &self.source, MAX_SOURCE_CHARS)?;
        let title = required("title", &self.title, MAX_TITLE_CHARS)?;
        let body = self.body.trim().to_owned();
        check_len("body", &body, MAX_BODY_CHARS)?;

        let mut tags: Vec<String> = self
            .tags
            .into_iter()
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        tags.sort();
        tags.dedup();

        Ok(Event {
            id,
            source: EventSource(source),
            title,
            body,
            severity: self.severity,
            tags,
            created_at: now,
        })
    }
}

fn required(field: &'static str, value: &str, limit: usize) -> Result<String, ValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::Empty { field });
    }
    check_len(field, trimmed, limit)?;
    Ok(trimmed.to_owned())
}

fn check_len(field: &'static str, value: &str, limit: usize) -> Result<(), ValidationError> {
    // Count chars, not bytes: a 200-char Chinese title is ~600 bytes.
    let actual = value.chars().count();
    if actual > limit {
        return Err(ValidationError::TooLong {
            field,
            actual,
            limit,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap()
    }

    fn draft() -> EventDraft {
        EventDraft {
            source: " claude-code ".into(),
            title: "  备货完成 ".into(),
            body: "5 条已推送\n".into(),
            severity: Severity::Urgent,
            tags: vec![
                "Social".into(),
                " social".into(),
                String::new(),
                "xhs".into(),
            ],
        }
    }

    #[test]
    fn validate_trims_and_normalizes() {
        let e = draft().validate(EventId::new(), now()).unwrap();
        assert_eq!(e.source.as_str(), "claude-code");
        assert_eq!(e.title, "备货完成");
        assert_eq!(e.body, "5 条已推送");
        assert_eq!(e.tags, vec!["social", "xhs"]);
        assert_eq!(e.created_at, now());
    }

    #[test]
    fn empty_title_rejected() {
        let d = EventDraft {
            title: "   ".into(),
            ..draft()
        };
        assert_eq!(
            d.validate(EventId::new(), now()),
            Err(ValidationError::Empty { field: "title" })
        );
    }

    #[test]
    fn empty_source_rejected() {
        let d = EventDraft {
            source: String::new(),
            ..draft()
        };
        assert_eq!(
            d.validate(EventId::new(), now()),
            Err(ValidationError::Empty { field: "source" })
        );
    }

    #[test]
    fn length_limit_counts_chars_not_bytes() {
        let at_limit = EventDraft {
            title: "鹊".repeat(MAX_TITLE_CHARS),
            ..draft()
        };
        assert!(at_limit.validate(EventId::new(), now()).is_ok());

        let over = EventDraft {
            title: "鹊".repeat(MAX_TITLE_CHARS + 1),
            ..draft()
        };
        assert_eq!(
            over.validate(EventId::new(), now()),
            Err(ValidationError::TooLong {
                field: "title",
                actual: MAX_TITLE_CHARS + 1,
                limit: MAX_TITLE_CHARS
            })
        );
    }

    #[test]
    fn severity_order_and_escalation() {
        assert!(Severity::Info < Severity::Normal);
        assert!(Severity::Urgent < Severity::Critical);
        assert!(!Severity::Normal.escalates());
        assert!(Severity::Urgent.escalates());
        assert!(Severity::Critical.escalates());
    }

    #[test]
    fn draft_json_defaults() {
        let d: EventDraft = serde_json::from_str(r#"{"source":"cli","title":"hi"}"#).unwrap();
        assert_eq!(d.severity, Severity::Normal);
        assert!(d.body.is_empty());
        assert!(d.tags.is_empty());
    }

    #[test]
    fn severity_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&Severity::Critical).unwrap(),
            r#""critical""#
        );
    }
}
