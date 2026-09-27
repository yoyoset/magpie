//! A human's response to an event.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::event::EventId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AckKind {
    /// Seen and handled. Stops escalation.
    Acknowledged,
    /// Remind again at `until`. Pauses escalation until then.
    Snoozed { until: DateTime<Utc> },
    /// Free-text answer, relayed back to the source when it registered a callback.
    Replied { text: String },
    /// Not relevant. Stops escalation without implying it was handled.
    Dismissed,
}

impl AckKind {
    /// Whether this response ends the event's escalation for good.
    pub fn is_final(&self) -> bool {
        !matches!(self, Self::Snoozed { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ack {
    pub event_id: EventId,
    pub kind: AckKind,
    /// Where the response came from, e.g. `desktop`, `pet`, `telegram`.
    pub via: String,
    pub at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn only_snooze_is_not_final() {
        let t = Utc.with_ymd_and_hms(2026, 9, 28, 20, 0, 0).unwrap();
        assert!(AckKind::Acknowledged.is_final());
        assert!(AckKind::Dismissed.is_final());
        assert!(AckKind::Replied { text: "发".into() }.is_final());
        assert!(!AckKind::Snoozed { until: t }.is_final());
    }

    #[test]
    fn ack_kind_json_is_tagged() {
        let json = serde_json::to_string(&AckKind::Replied { text: "发".into() }).unwrap();
        assert_eq!(json, r#"{"kind":"replied","text":"发"}"#);
    }
}
