//! Human time input → UTC. Pure: "now" and the zone are parameters.

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, TimeZone, Utc};

pub fn duration(s: &str) -> anyhow::Result<TimeDelta> {
    let d = humantime::parse_duration(s.trim())
        .map_err(|e| anyhow::anyhow!("`{s}` is not a duration like 30m, 2h, 1day: {e}"))?;
    TimeDelta::from_std(d).map_err(|_| anyhow::anyhow!("`{s}` is too long"))
}

/// `YYYY-MM-DD HH:MM` or `HH:MM` in `tz`. A bare time already past today means tomorrow.
pub fn at<Tz: TimeZone>(s: &str, now: DateTime<Utc>, tz: &Tz) -> anyhow::Result<DateTime<Utc>> {
    let s = s.trim();
    let naive = if let Ok(full) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M") {
        full
    } else if let Ok(time) = NaiveTime::parse_from_str(s, "%H:%M") {
        let today: NaiveDate = now.with_timezone(tz).date_naive();
        let candidate = today.and_time(time);
        if local_to_utc(candidate, tz)? <= now {
            candidate + TimeDelta::days(1)
        } else {
            candidate
        }
    } else {
        anyhow::bail!("`{s}` is not a time like `19:50` or `2026-09-28 19:50`");
    };
    let t = local_to_utc(naive, tz)?;
    anyhow::ensure!(t > now, "`{s}` is in the past");
    Ok(t)
}

fn local_to_utc<Tz: TimeZone>(naive: NaiveDateTime, tz: &Tz) -> anyhow::Result<DateTime<Utc>> {
    // `earliest` resolves DST overlaps; `None` means the time does not exist (DST gap).
    tz.from_local_datetime(&naive)
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .ok_or_else(|| anyhow::anyhow!("{naive} does not exist in the local time zone"))
}

/// Formats a UTC time for display in the machine's zone.
pub fn local_short(t: DateTime<Utc>) -> String {
    t.with_timezone(&Local).format("%m-%d %H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn cst() -> FixedOffset {
        FixedOffset::east_opt(8 * 3600).unwrap()
    }

    // 2026-09-28 20:00 Beijing.
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap()
    }

    #[test]
    fn durations() {
        assert_eq!(duration("30m").unwrap(), TimeDelta::minutes(30));
        assert_eq!(duration(" 2h ").unwrap(), TimeDelta::hours(2));
        assert!(duration("soon").is_err());
    }

    #[test]
    fn full_datetime_in_local_zone() {
        assert_eq!(
            at("2026-09-30 19:50", now(), &cst()).unwrap(),
            Utc.with_ymd_and_hms(2026, 9, 30, 11, 50, 0).unwrap()
        );
    }

    #[test]
    fn bare_time_later_today_or_tomorrow() {
        assert_eq!(
            at("21:05", now(), &cst()).unwrap(),
            Utc.with_ymd_and_hms(2026, 9, 28, 13, 5, 0).unwrap(),
            "still ahead today"
        );
        assert_eq!(
            at("19:50", now(), &cst()).unwrap(),
            Utc.with_ymd_and_hms(2026, 9, 29, 11, 50, 0).unwrap(),
            "already past -> tomorrow"
        );
    }

    #[test]
    fn past_or_garbage_rejected() {
        assert!(at("2026-09-01 10:00", now(), &cst()).is_err());
        assert!(at("tonight", now(), &cst()).is_err());
    }
}
