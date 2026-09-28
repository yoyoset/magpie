//! `followups` table: dedup keys of follow-ups already sent.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use rusqlite::params;

use crate::codec::ms;
use crate::{Result, Store};

impl Store {
    /// Keys sent since `since`. Keys embed their day, so a short window is enough.
    pub fn followup_keys_since(&self, since: DateTime<Utc>) -> Result<HashSet<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT key FROM followups WHERE sent_at >= ?1")?;
        let rows = stmt.query_map([ms(since)], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Returns false if the key was already recorded (someone else sent it).
    pub fn mark_followup(&self, key: &str, at: DateTime<Utc>) -> Result<bool> {
        Ok(self.conn.execute(
            "INSERT OR IGNORE INTO followups (key, sent_at) VALUES (?1, ?2)",
            params![key, ms(at)],
        )? > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeDelta, TimeZone};

    #[test]
    fn keys_are_recorded_once_and_windowed() {
        let s = Store::open_in_memory().unwrap();
        let t = Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap();
        assert!(s.mark_followup("brief:2026-09-28", t).unwrap());
        assert!(
            !s.mark_followup("brief:2026-09-28", t).unwrap(),
            "second insert is a no-op"
        );
        assert!(s
            .mark_followup("brief:2026-09-25", t - TimeDelta::days(3))
            .unwrap());
        let recent = s.followup_keys_since(t - TimeDelta::days(1)).unwrap();
        assert_eq!(recent, HashSet::from(["brief:2026-09-28".to_owned()]));
    }
}
