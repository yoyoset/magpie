//! How hard to push an unanswered event, and when.

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::ack::{Ack, AckKind};
use crate::event::Severity;

/// How far a delivery reaches. Ordered: each tier includes the ones below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Desktop toast and pet bubble.
    Desktop,
    /// Desktop plus remote channels (phone).
    Phone,
    /// Everything, plus the full-screen takeover.
    Takeover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
// Strict: a misspelled key in config.toml must be an error, not a silent default.
#[serde(default, deny_unknown_fields)]
pub struct EscalationPolicy {
    /// Wait this long without a response before notifying again.
    pub renotify_after_secs: u32,
    /// Urgent events move to the phone from this attempt on (0-based).
    /// 0 means "phone from the first attempt".
    pub phone_from_attempt: u32,
    /// Stop after this many attempts and surface the event as exhausted.
    pub max_attempts: u32,
}

impl Default for EscalationPolicy {
    fn default() -> Self {
        Self {
            renotify_after_secs: 10 * 60,
            phone_from_attempt: 1,
            max_attempts: 6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextAction {
    /// Deliver now at this tier.
    Deliver(Tier),
    /// Nothing to do before this moment.
    WaitUntil(DateTime<Utc>),
    /// Answered, or needs no (further) delivery.
    Done,
    /// Gave up without an answer. The UI must show this; it is not success.
    Exhausted,
}

impl EscalationPolicy {
    /// Decide the next step for one event.
    ///
    /// `attempts` are the times of previous deliveries, in any order.
    /// `latest_ack` is the most recent human response, if any.
    pub fn next_action(
        &self,
        severity: Severity,
        attempts: &[DateTime<Utc>],
        latest_ack: Option<&Ack>,
        now: DateTime<Utc>,
    ) -> NextAction {
        // A snooze restarts the count: only attempts after it wakes matter.
        let since = match latest_ack.map(|a| &a.kind) {
            Some(kind) if kind.is_final() => return NextAction::Done,
            Some(AckKind::Snoozed { until }) if now < *until => {
                return NextAction::WaitUntil(*until);
            }
            Some(AckKind::Snoozed { until }) => Some(*until),
            _ => None,
        };
        let relevant: Vec<DateTime<Utc>> = attempts
            .iter()
            .copied()
            .filter(|t| since.is_none_or(|s| *t >= s))
            .collect();
        let count = u32::try_from(relevant.len()).unwrap_or(u32::MAX);

        match severity {
            Severity::Normal if count == 0 => NextAction::Deliver(Tier::Desktop),
            Severity::Info | Severity::Normal => NextAction::Done,
            Severity::Urgent | Severity::Critical => {
                if count >= self.max_attempts.max(1) {
                    return NextAction::Exhausted;
                }
                if let Some(last) = relevant.iter().max() {
                    let due = *last + TimeDelta::seconds(i64::from(self.renotify_after_secs));
                    if now < due {
                        return NextAction::WaitUntil(due);
                    }
                }
                NextAction::Deliver(self.tier_for(severity, count))
            }
        }
    }

    fn tier_for(&self, severity: Severity, attempt: u32) -> Tier {
        match severity {
            Severity::Critical => Tier::Takeover,
            _ if attempt >= self.phone_from_attempt => Tier::Phone,
            _ => Tier::Desktop,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventId;
    use chrono::TimeZone;

    fn t(min: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 28, 20, 0, 0).unwrap() + TimeDelta::minutes(min)
    }

    fn ack(kind: AckKind) -> Ack {
        Ack {
            event_id: EventId::new(),
            kind,
            via: "test".into(),
            at: t(0),
        }
    }

    const P: EscalationPolicy = EscalationPolicy {
        renotify_after_secs: 600,
        phone_from_attempt: 1,
        max_attempts: 3,
    };

    #[test]
    fn info_is_never_delivered() {
        assert_eq!(
            P.next_action(Severity::Info, &[], None, t(0)),
            NextAction::Done
        );
    }

    #[test]
    fn normal_delivers_once() {
        assert_eq!(
            P.next_action(Severity::Normal, &[], None, t(0)),
            NextAction::Deliver(Tier::Desktop)
        );
        assert_eq!(
            P.next_action(Severity::Normal, &[t(0)], None, t(60)),
            NextAction::Done
        );
    }

    #[test]
    fn urgent_escalates_desktop_then_phone_then_gives_up() {
        let s = Severity::Urgent;
        assert_eq!(
            P.next_action(s, &[], None, t(0)),
            NextAction::Deliver(Tier::Desktop)
        );
        assert_eq!(
            P.next_action(s, &[t(0)], None, t(5)),
            NextAction::WaitUntil(t(10))
        );
        assert_eq!(
            P.next_action(s, &[t(0)], None, t(10)),
            NextAction::Deliver(Tier::Phone)
        );
        assert_eq!(
            P.next_action(s, &[t(0), t(10)], None, t(20)),
            NextAction::Deliver(Tier::Phone)
        );
        assert_eq!(
            P.next_action(s, &[t(0), t(10), t(20)], None, t(30)),
            NextAction::Exhausted
        );
    }

    #[test]
    fn attempt_order_does_not_matter() {
        assert_eq!(
            P.next_action(Severity::Urgent, &[t(10), t(0)], None, t(15)),
            NextAction::WaitUntil(t(20))
        );
    }

    #[test]
    fn critical_always_takes_over() {
        assert_eq!(
            P.next_action(Severity::Critical, &[], None, t(0)),
            NextAction::Deliver(Tier::Takeover)
        );
    }

    #[test]
    fn final_ack_stops_everything() {
        for kind in [
            AckKind::Acknowledged,
            AckKind::Dismissed,
            AckKind::Replied { text: "发".into() },
        ] {
            assert_eq!(
                P.next_action(Severity::Critical, &[t(0)], Some(&ack(kind)), t(60)),
                NextAction::Done
            );
        }
    }

    #[test]
    fn snooze_waits_then_restarts_the_count() {
        let snooze = ack(AckKind::Snoozed { until: t(60) });
        let attempts = [t(0), t(10)];
        assert_eq!(
            P.next_action(Severity::Urgent, &attempts, Some(&snooze), t(30)),
            NextAction::WaitUntil(t(60))
        );
        // After waking, earlier attempts are ignored: back to desktop, not exhausted.
        assert_eq!(
            P.next_action(Severity::Urgent, &attempts, Some(&snooze), t(60)),
            NextAction::Deliver(Tier::Desktop)
        );
    }

    #[test]
    fn zero_max_attempts_still_delivers_once() {
        let p = EscalationPolicy {
            max_attempts: 0,
            ..P
        };
        assert_eq!(
            p.next_action(Severity::Urgent, &[], None, t(0)),
            NextAction::Deliver(Tier::Desktop)
        );
    }

    #[test]
    fn tiers_are_ordered() {
        assert!(Tier::Desktop < Tier::Phone && Tier::Phone < Tier::Takeover);
    }
}
