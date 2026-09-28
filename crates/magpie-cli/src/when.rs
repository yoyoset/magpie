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

/// A calendar day relative to `today`: today/今天, tomorrow/明天, 后天,
/// 周一..周日 (the next one after today), `MM-DD` (this year), `YYYY-MM-DD`.
pub fn day(s: &str, today: NaiveDate) -> anyhow::Result<NaiveDate> {
    use chrono::{Datelike, Weekday};
    let s = s.trim();
    let rel = match s.to_lowercase().as_str() {
        "today" | "今天" => Some(0),
        "tomorrow" | "明天" => Some(1),
        "后天" => Some(2),
        _ => None,
    };
    if let Some(n) = rel {
        return Ok(today + TimeDelta::days(n));
    }
    let weekday = match s {
        "周一" | "星期一" | "mon" => Some(Weekday::Mon),
        "周二" | "星期二" | "tue" => Some(Weekday::Tue),
        "周三" | "星期三" | "wed" => Some(Weekday::Wed),
        "周四" | "星期四" | "thu" => Some(Weekday::Thu),
        "周五" | "星期五" | "fri" => Some(Weekday::Fri),
        "周六" | "星期六" | "sat" => Some(Weekday::Sat),
        "周日" | "周天" | "星期日" | "星期天" | "sun" => Some(Weekday::Sun),
        _ => None,
    };
    if let Some(w) = weekday {
        let ahead = (7 + i64::from(w.num_days_from_monday())
            - i64::from(today.weekday().num_days_from_monday()))
            % 7;
        return Ok(today + TimeDelta::days(if ahead == 0 { 7 } else { ahead }));
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(d);
    }
    if let Ok(d) = NaiveDate::parse_from_str(&format!("{}-{s}", today.year()), "%Y-%m-%d") {
        return Ok(d);
    }
    anyhow::bail!("`{s}` is not a day like 今天 / 明天 / 周三 / 10-02 / 2026-10-02")
}

pub fn clock(s: &str) -> anyhow::Result<NaiveTime> {
    NaiveTime::parse_from_str(s.trim(), "%H:%M")
        .map_err(|_| anyhow::anyhow!("`{s}` is not a time like 19:50"))
}

/// Today in the machine's zone.
pub fn local_today() -> NaiveDate {
    Local::now().date_naive()
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
    fn relative_and_absolute_days() {
        let mon = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap(); // a Monday
        let d = |s| day(s, mon).unwrap();
        assert_eq!(d("今天"), mon);
        assert_eq!(d("tomorrow"), NaiveDate::from_ymd_opt(2026, 9, 29).unwrap());
        assert_eq!(d("后天"), NaiveDate::from_ymd_opt(2026, 9, 30).unwrap());
        assert_eq!(d("周三"), NaiveDate::from_ymd_opt(2026, 9, 30).unwrap());
        assert_eq!(
            d("周一"),
            NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
            "next Monday, not today"
        );
        assert_eq!(d("10-02"), NaiveDate::from_ymd_opt(2026, 10, 2).unwrap());
        assert_eq!(
            d("2027-01-05"),
            NaiveDate::from_ymd_opt(2027, 1, 5).unwrap()
        );
        assert!(day("someday", mon).is_err());
        assert!(clock("25:00").is_err());
    }

    #[test]
    fn past_or_garbage_rejected() {
        assert!(at("2026-09-01 10:00", now(), &cst()).is_err());
        assert!(at("tonight", now(), &cst()).is_err());
    }
}
