//! Reminders: events produced by a schedule instead of by a caller.

use std::fmt;

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::event::EventDraft;
use crate::schedule::Schedule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReminderId(pub Uuid);

impl ReminderId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ReminderId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ReminderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reminder {
    pub id: ReminderId,
    pub schedule: Schedule,
    /// Validated into a fresh `Event` every time the reminder fires.
    pub template: EventDraft,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    /// Last occurrence that produced an event. `None` until the first fire.
    pub last_fired: Option<DateTime<Utc>>,
}

impl Reminder {
    /// The occurrence to fire at `now`, if any. Missed ones are coalesced
    /// (see [`Schedule::due`]). Before the first fire, counting starts at creation.
    pub fn due<Tz: TimeZone>(&self, now: DateTime<Utc>, tz: &Tz) -> Option<DateTime<Utc>> {
        if !self.enabled {
            return None;
        }
        let since = self.last_fired.unwrap_or(self.created_at);
        self.schedule.due(since, now, tz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Severity;

    fn utc(d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, d, h, 0, 0).unwrap()
    }

    fn reminder(last_fired: Option<DateTime<Utc>>) -> Reminder {
        Reminder {
            id: ReminderId::new(),
            schedule: Schedule::cron("0 12 * * *").unwrap(),
            template: EventDraft {
                source: "reminder".into(),
                title: "发小红书".into(),
                severity: Severity::Urgent,
                ..EventDraft::default()
            },
            enabled: true,
            created_at: utc(28, 10),
            last_fired,
        }
    }

    #[test]
    fn first_fire_counts_from_creation() {
        let r = reminder(None);
        assert_eq!(r.due(utc(28, 11), &Utc), None);
        assert_eq!(r.due(utc(28, 12), &Utc), Some(utc(28, 12)));
    }

    #[test]
    fn creation_after_todays_slot_waits_for_tomorrow() {
        let r = Reminder {
            created_at: utc(28, 13),
            ..reminder(None)
        };
        assert_eq!(r.due(utc(28, 23), &Utc), None);
    }

    #[test]
    fn disabled_never_due() {
        let r = Reminder {
            enabled: false,
            ..reminder(None)
        };
        assert_eq!(r.due(utc(30, 12), &Utc), None);
    }

    #[test]
    fn after_fire_waits_for_next_occurrence() {
        let r = reminder(Some(utc(28, 12)));
        assert_eq!(r.due(utc(29, 11), &Utc), None);
        assert_eq!(r.due(utc(29, 12), &Utc), Some(utc(29, 12)));
    }
}
