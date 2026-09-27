//! A record of one attempt to reach a human through one channel.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::escalation::Tier;
use crate::event::EventId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DeliveryOutcome {
    Sent,
    /// Kept, not swallowed: the inbox shows failed deliveries so a broken
    /// channel is never mistaken for "no news".
    Failed {
        error: String,
    },
}

/// One channel's delivery within one escalation round.
///
/// All channels delivered in the same round share the same `at`; the number of
/// distinct `at` values for an event is its attempt count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    pub event_id: EventId,
    pub channel: String,
    pub tier: Tier,
    pub at: DateTime<Utc>,
    pub outcome: DeliveryOutcome,
}
