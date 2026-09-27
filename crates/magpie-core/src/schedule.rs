//! When a reminder fires.
//!
//! All computations take the caller's time zone explicitly: cron patterns are
//! written in wall-clock time ("21:05 every Sunday"), but everything is stored
//! and compared in UTC.

use std::str::FromStr;

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use croner::Cron;
use serde::{Deserialize, Serialize};

/// Shortest allowed repeat. Anything faster is a bug or spam, not a reminder.
pub const MIN_INTERVAL_SECS: u64 = 60;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScheduleError {
    #[error("invalid cron pattern `{pattern}`: {reason}")]
    InvalidCron { pattern: String, reason: String },
    #[error("interval must be at least {MIN_INTERVAL_SECS}s, got {0}s")]
    IntervalTooShort(u64),
}

/// A validated 5-field cron pattern, evaluated in the caller's time zone.
#[derive(Debug, Clone)]
pub struct CronExpr {
    source: String,
    // Boxed: `Cron` is large and would bloat every `Schedule` value.
    cron: Box<Cron>,
}

impl CronExpr {
    pub fn parse(pattern: &str) -> Result<Self, ScheduleError> {
        let source = pattern.trim().to_owned();
        let cron = Cron::from_str(&source).map_err(|e| ScheduleError::InvalidCron {
            pattern: source.clone(),
            reason: e.to_string(),
        })?;
        Ok(Self {
            source,
            cron: Box::new(cron),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.source
    }

    fn next_after<Tz: TimeZone>(&self, after: DateTime<Utc>, tz: &Tz) -> Option<DateTime<Utc>> {
        let local = after.with_timezone(tz);
        self.cron
            .find_next_occurrence(&local, false)
            .ok()
            .map(|t| t.with_timezone(&Utc))
    }
}

impl PartialEq for CronExpr {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Eq for CronExpr {}

impl Serialize for CronExpr {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.source)
    }
}

impl<'de> Deserialize<'de> for CronExpr {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    Once {
        at: DateTime<Utc>,
    },
    /// Every `every_secs`, aligned to `anchor` (not to when the app happened to start).
    Interval {
        anchor: DateTime<Utc>,
        #[serde(deserialize_with = "de_interval")]
        every_secs: u64,
    },
    Cron {
        expr: CronExpr,
    },
}

fn de_interval<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let secs = u64::deserialize(d)?;
    check_interval(secs).map_err(serde::de::Error::custom)
}

fn check_interval(secs: u64) -> Result<u64, ScheduleError> {
    if secs < MIN_INTERVAL_SECS {
        return Err(ScheduleError::IntervalTooShort(secs));
    }
    Ok(secs)
}

impl Schedule {
    pub fn interval(anchor: DateTime<Utc>, every_secs: u64) -> Result<Self, ScheduleError> {
        Ok(Self::Interval {
            anchor,
            every_secs: check_interval(every_secs)?,
        })
    }

    pub fn cron(pattern: &str) -> Result<Self, ScheduleError> {
        Ok(Self::Cron {
            expr: CronExpr::parse(pattern)?,
        })
    }

    /// First fire time strictly after `after`, or `None` if the schedule is spent.
    pub fn next_fire<Tz: TimeZone>(&self, after: DateTime<Utc>, tz: &Tz) -> Option<DateTime<Utc>> {
        match self {
            Self::Once { at } => (*at > after).then_some(*at),
            Self::Interval { anchor, every_secs } => {
                Some(next_interval_tick(*anchor, *every_secs, after))
            }
            Self::Cron { expr } => expr.next_after(after, tz),
        }
    }

    /// What should fire at `now`, given the last time this schedule fired.
    ///
    /// Missed occurrences (machine asleep, app closed) are **coalesced into one**:
    /// a daily reminder after three days off fires once, not three times. The
    /// returned time is the most recent missed occurrence, so the caller can record
    /// it as `last_fired` and move on.
    pub fn due<Tz: TimeZone>(
        &self,
        last_fired: DateTime<Utc>,
        now: DateTime<Utc>,
        tz: &Tz,
    ) -> Option<DateTime<Utc>> {
        let first = self.next_fire(last_fired, tz)?;
        if first > now {
            return None;
        }
        let mut latest = first;
        // Walk forward to the latest occurrence <= now. Bounded: cron/interval
        // are >= 1 minute apart and outages are days, not years; the cap guards
        // against a pathological clock jump.
        for _ in 0..100_000 {
            match self.next_fire(latest, tz) {
                Some(t) if t <= now => latest = t,
                _ => break,
            }
        }
        Some(latest)
    }
}

fn next_interval_tick(
    anchor: DateTime<Utc>,
    every_secs: u64,
    after: DateTime<Utc>,
) -> DateTime<Utc> {
    if anchor > after {
        return anchor;
    }
    let step = i64::try_from(every_secs).unwrap_or(i64::MAX);
    let elapsed = (after - anchor).num_seconds();
    let ticks = elapsed / step + 1;
    anchor + TimeDelta::seconds(ticks.saturating_mul(step))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    fn cst() -> FixedOffset {
        FixedOffset::east_opt(8 * 3600).unwrap()
    }

    #[test]
    fn once_fires_only_in_the_future() {
        let s = Schedule::Once {
            at: utc(2026, 9, 28, 12, 0),
        };
        assert_eq!(
            s.next_fire(utc(2026, 9, 28, 11, 59), &Utc),
            Some(utc(2026, 9, 28, 12, 0))
        );
        assert_eq!(
            s.next_fire(utc(2026, 9, 28, 12, 0), &Utc),
            None,
            "strictly after"
        );
    }

    #[test]
    fn interval_aligns_to_anchor() {
        let s = Schedule::interval(utc(2026, 9, 28, 0, 0), 3600).unwrap();
        assert_eq!(
            s.next_fire(utc(2026, 9, 28, 5, 30), &Utc),
            Some(utc(2026, 9, 28, 6, 0))
        );
        assert_eq!(
            s.next_fire(utc(2026, 9, 28, 6, 0), &Utc),
            Some(utc(2026, 9, 28, 7, 0)),
            "exact tick is not 'after' itself"
        );
        assert_eq!(
            s.next_fire(utc(2026, 9, 27, 0, 0), &Utc),
            Some(utc(2026, 9, 28, 0, 0)),
            "before anchor -> anchor"
        );
    }

    #[test]
    fn interval_rejects_spam() {
        assert_eq!(
            Schedule::interval(utc(2026, 1, 1, 0, 0), 59),
            Err(ScheduleError::IntervalTooShort(59))
        );
        let json = r#"{"kind":"interval","anchor":"2026-01-01T00:00:00Z","every_secs":5}"#;
        assert!(serde_json::from_str::<Schedule>(json).is_err());
    }

    #[test]
    fn cron_uses_wall_clock_of_given_zone() {
        // 21:05 every Sunday, Beijing time. 2026-09-27 is a Sunday.
        let s = Schedule::cron("5 21 * * 0").unwrap();
        let next = s.next_fire(utc(2026, 9, 27, 0, 0), &cst()).unwrap();
        assert_eq!(next, utc(2026, 9, 27, 13, 5));
        let after = s.next_fire(next, &cst()).unwrap();
        assert_eq!(after, utc(2026, 10, 4, 13, 5), "one week later");
    }

    #[test]
    fn cron_crosses_month_end() {
        // 10:00 on the 1st of each month, Beijing time.
        let s = Schedule::cron("0 10 1 * *").unwrap();
        assert_eq!(
            s.next_fire(utc(2026, 9, 28, 0, 0), &cst()),
            Some(utc(2026, 10, 1, 2, 0))
        );
    }

    #[test]
    fn cron_rejects_garbage() {
        assert!(matches!(
            Schedule::cron("every sunday"),
            Err(ScheduleError::InvalidCron { .. })
        ));
        let json = r#"{"kind":"cron","expr":"61 * * * *"}"#;
        assert!(serde_json::from_str::<Schedule>(json).is_err());
    }

    #[test]
    fn cron_json_roundtrip_keeps_pattern_text() {
        let s = Schedule::cron("0 20 * * 1,3,5").unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, r#"{"kind":"cron","expr":"0 20 * * 1,3,5"}"#);
        assert_eq!(serde_json::from_str::<Schedule>(&json).unwrap(), s);
    }

    #[test]
    fn missed_occurrences_coalesce_to_latest() {
        // Daily 09:00 UTC; app was off from Monday 09:30 until Thursday 12:00.
        let s = Schedule::cron("0 9 * * *").unwrap();
        let last = utc(2026, 9, 28, 9, 0);
        let now = utc(2026, 10, 1, 12, 0);
        assert_eq!(s.due(last, now, &Utc), Some(utc(2026, 10, 1, 9, 0)));
    }

    #[test]
    fn nothing_due_before_next_fire() {
        let s = Schedule::cron("0 9 * * *").unwrap();
        let last = utc(2026, 9, 28, 9, 0);
        assert_eq!(s.due(last, utc(2026, 9, 29, 8, 59), &Utc), None);
        assert_eq!(
            s.due(last, utc(2026, 9, 29, 9, 0), &Utc),
            Some(utc(2026, 9, 29, 9, 0)),
            "exactly on time counts as due"
        );
    }

    #[test]
    fn spent_once_is_never_due_again() {
        let at = utc(2026, 9, 28, 12, 0);
        let s = Schedule::Once { at };
        assert_eq!(s.due(at, utc(2027, 1, 1, 0, 0), &Utc), None);
    }

    #[test]
    fn dst_spring_forward_shifts_nonexistent_time_forward() {
        // US spring-forward 2026-03-08: 02:00-03:00 local does not exist.
        // First assumed croner skips the day; measured: it fires at the first valid
        // instant, 03:00 EDT (07:00 UTC), the same day. The reminder is not lost.
        let ny = chrono_tz::America::New_York;
        let s = Schedule::cron("30 2 * * *").unwrap();
        let next = s.next_fire(utc(2026, 3, 8, 6, 0), &ny).unwrap();
        assert_eq!(next, utc(2026, 3, 8, 7, 0));
    }
}
