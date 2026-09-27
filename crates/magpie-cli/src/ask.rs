//! `magpie ask`: post a question and block until a human answers.
//!
//! Pull, not push: the caller (typically an AI agent in a terminal) just runs a
//! command and reads stdout. No callback URL, no inbound port on the caller,
//! and Magpie never has to POST to an address supplied by event content.

use chrono::{DateTime, Utc};
use magpie_core::{Ack, AckKind};
use serde::Deserialize;

/// The subset of `GET /v1/events/{id}` that `ask` needs.
#[derive(Debug, Deserialize)]
pub struct Status {
    pub closed_at: Option<DateTime<Utc>>,
    pub latest_ack: Option<Ack>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The human wrote something back. Printed verbatim.
    Replied(String),
    Acknowledged,
    Dismissed,
}

impl Answer {
    /// Exit code contract for scripts: 0 = go ahead, 4 = declined.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Replied(_) | Self::Acknowledged => 0,
            Self::Dismissed => 4,
        }
    }

    pub fn stdout(&self) -> &str {
        match self {
            Self::Replied(text) => text,
            Self::Acknowledged => "acknowledged",
            Self::Dismissed => "dismissed",
        }
    }
}

/// Timed out waiting. The event stays open: the human may still answer later.
pub const TIMEOUT_EXIT: u8 = 5;

/// `Some` once the event is closed. While it is open (including snoozed) the
/// caller keeps waiting.
pub fn answer(status: &Status) -> Option<Answer> {
    status.closed_at?;
    Some(match status.latest_ack.as_ref().map(|a| &a.kind) {
        Some(AckKind::Replied { text }) => Answer::Replied(text.clone()),
        Some(AckKind::Dismissed) => Answer::Dismissed,
        // Closed is what counts. A snooze recorded after closing changes nothing.
        Some(AckKind::Acknowledged | AckKind::Snoozed { .. }) | None => Answer::Acknowledged,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use magpie_core::EventId;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 28, 20, 0, 0).unwrap()
    }

    fn status(closed: bool, kind: Option<AckKind>) -> Status {
        Status {
            closed_at: closed.then(at),
            latest_ack: kind.map(|kind| Ack {
                event_id: EventId::new(),
                kind,
                via: "telegram".into(),
                at: at(),
            }),
        }
    }

    #[test]
    fn open_or_snoozed_means_keep_waiting() {
        assert_eq!(answer(&status(false, None)), None);
        assert_eq!(
            answer(&status(false, Some(AckKind::Snoozed { until: at() }))),
            None
        );
    }

    #[test]
    fn final_answers() {
        let replied = answer(&status(true, Some(AckKind::Replied { text: "发".into() }))).unwrap();
        assert_eq!((replied.stdout(), replied.exit_code()), ("发", 0));
        let ok = answer(&status(true, Some(AckKind::Acknowledged))).unwrap();
        assert_eq!((ok.stdout(), ok.exit_code()), ("acknowledged", 0));
        let no = answer(&status(true, Some(AckKind::Dismissed))).unwrap();
        assert_eq!((no.stdout(), no.exit_code()), ("dismissed", 4));
    }

    #[test]
    fn closed_is_final_even_if_a_late_snooze_came_in() {
        assert_eq!(
            answer(&status(true, Some(AckKind::Snoozed { until: at() }))),
            Some(Answer::Acknowledged)
        );
    }
}
