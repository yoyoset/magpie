//! `reminders` table.

use chrono::{DateTime, Utc};
use magpie_core::{Reminder, ReminderId};
use rusqlite::{params, Row};

use crate::codec::{from_json, from_ms, ms, opt_from_ms, to_json, uuid};
use crate::{Result, Store};

fn decode(row: &Row<'_>) -> rusqlite::Result<Result<Reminder>> {
    let id: String = row.get(0)?;
    let schedule: String = row.get(1)?;
    let template: String = row.get(2)?;
    let enabled: bool = row.get(3)?;
    let created_at: i64 = row.get(4)?;
    let last_fired: Option<i64> = row.get(5)?;
    Ok((|| {
        Ok(Reminder {
            id: ReminderId(uuid("reminder id", &id)?),
            schedule: from_json("schedule", &schedule)?,
            template: from_json("reminder template", &template)?,
            enabled,
            created_at: from_ms(created_at)?,
            last_fired: opt_from_ms(last_fired)?,
        })
    })())
}

impl Store {
    /// Insert or replace by id.
    pub fn save_reminder(&self, r: &Reminder) -> Result<()> {
        self.conn.execute(
            "INSERT INTO reminders (id, schedule, template, enabled, created_at, last_fired)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (id) DO UPDATE SET
                schedule = excluded.schedule,
                template = excluded.template,
                enabled = excluded.enabled,
                last_fired = excluded.last_fired",
            params![
                r.id.to_string(),
                to_json(&r.schedule),
                to_json(&r.template),
                r.enabled,
                ms(r.created_at),
                r.last_fired.map(ms),
            ],
        )?;
        Ok(())
    }

    pub fn reminders(&self) -> Result<Vec<Reminder>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, schedule, template, enabled, created_at, last_fired
             FROM reminders ORDER BY created_at",
        )?;
        let rows = stmt.query_map([], decode)?;
        rows.map(|r| r?).collect()
    }

    /// Returns whether a reminder was deleted.
    pub fn delete_reminder(&self, id: ReminderId) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM reminders WHERE id = ?1", [id.to_string()])?
            > 0)
    }

    pub fn mark_reminder_fired(&self, id: ReminderId, at: DateTime<Utc>) -> Result<bool> {
        Ok(self.conn.execute(
            "UPDATE reminders SET last_fired = ?2 WHERE id = ?1",
            params![id.to_string(), ms(at)],
        )? > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use magpie_core::{EventDraft, Schedule, Severity};

    fn reminder() -> Reminder {
        Reminder {
            id: ReminderId::new(),
            schedule: Schedule::cron("50 19 * * 1,3,5").unwrap(),
            template: EventDraft {
                source: "reminder".into(),
                title: "发小红书".into(),
                severity: Severity::Urgent,
                ..EventDraft::default()
            },
            enabled: true,
            created_at: Utc.with_ymd_and_hms(2026, 9, 28, 0, 0, 0).unwrap(),
            last_fired: None,
        }
    }

    #[test]
    fn save_list_update_delete() {
        let s = Store::open_in_memory().unwrap();
        let mut r = reminder();
        s.save_reminder(&r).unwrap();
        assert_eq!(s.reminders().unwrap(), vec![r.clone()]);

        r.enabled = false;
        s.save_reminder(&r).unwrap();
        assert_eq!(
            s.reminders().unwrap(),
            vec![r.clone()],
            "upsert, not duplicate"
        );

        let fired = Utc.with_ymd_and_hms(2026, 9, 28, 11, 50, 0).unwrap();
        assert!(s.mark_reminder_fired(r.id, fired).unwrap());
        assert_eq!(s.reminders().unwrap()[0].last_fired, Some(fired));

        assert!(s.delete_reminder(r.id).unwrap());
        assert!(!s.delete_reminder(r.id).unwrap());
        assert!(s.reminders().unwrap().is_empty());
    }

    #[test]
    fn corrupt_schedule_is_reported_not_panicked() {
        let s = Store::open_in_memory().unwrap();
        s.save_reminder(&reminder()).unwrap();
        s.conn
            .execute(
                "UPDATE reminders SET schedule = '{\"kind\":\"cron\",\"expr\":\"nope\"}'",
                [],
            )
            .unwrap();
        assert!(matches!(
            s.reminders(),
            Err(crate::StoreError::Corrupt {
                what: "schedule",
                ..
            })
        ));
    }
}
