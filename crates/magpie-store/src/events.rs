//! `events` table, plus the delivery and ack history that hangs off each event.

use chrono::{DateTime, Utc};
use magpie_core::{Ack, AckKind, Delivery, Event, EventId, EventSource, Severity};
use rusqlite::{params, OptionalExtension, Row};

use crate::codec::{enum_from_name, enum_name, from_json, from_ms, ms, opt_from_ms, to_json, uuid};
use crate::{Result, Store, StoreError};

/// An event as stored: the event itself plus its lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    pub event: Event,
    /// When the first final ack (acknowledged / replied / dismissed) arrived.
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventQuery {
    pub open_only: bool,
    /// Newest first; page by passing the last seen `created_at`.
    pub before: Option<DateTime<Utc>>,
    pub limit: u32,
}

impl Default for EventQuery {
    fn default() -> Self {
        Self {
            open_only: false,
            before: None,
            limit: 100,
        }
    }
}

const EVENT_COLS: &str = "id, source, title, body, severity, tags, created_at, closed_at";

fn decode_record(row: &Row<'_>) -> rusqlite::Result<Result<EventRecord>> {
    let id: String = row.get(0)?;
    let source: String = row.get(1)?;
    let title: String = row.get(2)?;
    let body: String = row.get(3)?;
    let severity: String = row.get(4)?;
    let tags: String = row.get(5)?;
    let created_at: i64 = row.get(6)?;
    let closed_at: Option<i64> = row.get(7)?;
    // Inner Result: decoding errors are ours, not SQLite's.
    Ok((|| {
        Ok(EventRecord {
            event: Event {
                id: EventId(uuid("event id", &id)?),
                source: EventSource::from_trusted(source),
                title,
                body,
                severity: enum_from_name::<Severity>("severity", &severity)?,
                tags: from_json("tags", &tags)?,
                created_at: from_ms(created_at)?,
            },
            closed_at: opt_from_ms(closed_at)?,
        })
    })())
}

impl Store {
    pub fn insert_event(&self, event: &Event) -> Result<()> {
        self.conn.execute(
            "INSERT INTO events (id, source, title, body, severity, tags, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.id.to_string(),
                event.source.as_str(),
                event.title,
                event.body,
                enum_name(&event.severity),
                to_json(&event.tags),
                ms(event.created_at),
            ],
        )?;
        Ok(())
    }

    pub fn get_event(&self, id: EventId) -> Result<Option<EventRecord>> {
        self.conn
            .query_row(
                &format!("SELECT {EVENT_COLS} FROM events WHERE id = ?1"),
                [id.to_string()],
                decode_record,
            )
            .optional()?
            .transpose()
    }

    pub fn list_events(&self, q: &EventQuery) -> Result<Vec<EventRecord>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {EVENT_COLS} FROM events
             WHERE (?1 = 0 OR closed_at IS NULL)
               AND (?2 IS NULL OR created_at < ?2)
             ORDER BY created_at DESC, id DESC
             LIMIT ?3"
        ))?;
        let rows = stmt.query_map(
            params![q.open_only, q.before.map(ms), q.limit],
            decode_record,
        )?;
        rows.map(|r| r?).collect()
    }

    /// Open events that may still need delivering, oldest first. Info-level
    /// events are never delivered, so they are left out.
    pub fn events_needing_attention(&self) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {EVENT_COLS} FROM events
             WHERE closed_at IS NULL AND severity != ?1
             ORDER BY created_at ASC"
        ))?;
        let rows = stmt.query_map([enum_name(&Severity::Info)], decode_record)?;
        rows.map(|r| r?.map(|rec| rec.event)).collect()
    }

    pub fn record_delivery(&self, d: &Delivery) -> Result<()> {
        self.conn.execute(
            "INSERT INTO deliveries (event_id, channel, tier, at, outcome)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                d.event_id.to_string(),
                d.channel,
                enum_name(&d.tier),
                ms(d.at),
                to_json(&d.outcome),
            ],
        )?;
        Ok(())
    }

    pub fn deliveries(&self, event_id: EventId) -> Result<Vec<Delivery>> {
        let mut stmt = self.conn.prepare(
            "SELECT channel, tier, at, outcome FROM deliveries
             WHERE event_id = ?1 ORDER BY at, id",
        )?;
        let rows = stmt.query_map([event_id.to_string()], |r| {
            let channel: String = r.get(0)?;
            let tier: String = r.get(1)?;
            let at: i64 = r.get(2)?;
            let outcome: String = r.get(3)?;
            Ok((|| {
                Ok(Delivery {
                    event_id,
                    channel,
                    tier: enum_from_name("tier", &tier)?,
                    at: from_ms(at)?,
                    outcome: from_json("delivery outcome", &outcome)?,
                })
            })())
        })?;
        rows.map(|r| r?).collect()
    }

    /// Distinct round times: the attempt history escalation works from.
    pub fn attempt_times(&self, event_id: EventId) -> Result<Vec<DateTime<Utc>>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT at FROM deliveries WHERE event_id = ?1 ORDER BY at")?;
        let rows = stmt.query_map([event_id.to_string()], |r| r.get::<_, i64>(0))?;
        rows.map(|r| from_ms(r?)).collect()
    }

    /// Stores the ack and, if it is final, closes the event (first final ack wins).
    pub fn record_ack(&mut self, ack: &Ack) -> Result<()> {
        let tx = self.conn.transaction()?;
        let id = ack.event_id.to_string();
        let exists: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM events WHERE id = ?1)",
            [&id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(StoreError::EventNotFound(ack.event_id));
        }
        tx.execute(
            "INSERT INTO acks (event_id, kind, via, at) VALUES (?1, ?2, ?3, ?4)",
            params![id, to_json(&ack.kind), ack.via, ms(ack.at)],
        )?;
        if ack.kind.is_final() {
            tx.execute(
                "UPDATE events SET closed_at = ?2 WHERE id = ?1 AND closed_at IS NULL",
                params![id, ms(ack.at)],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Acknowledges and closes every open event carrying `tag`; returns how many.
    /// Used when the thing an event nags about is settled somewhere else (the
    /// todo was ticked off), so the reminder does not stay open forever.
    pub fn close_events_tagged(
        &mut self,
        tag: &str,
        via: &str,
        at: DateTime<Utc>,
    ) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let ids: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT id FROM events
                 WHERE closed_at IS NULL
                   AND EXISTS (SELECT 1 FROM json_each(events.tags) WHERE value = ?1)",
            )?;
            let rows = stmt.query_map([tag], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for id in &ids {
            tx.execute(
                "INSERT INTO acks (event_id, kind, via, at) VALUES (?1, ?2, ?3, ?4)",
                params![id, to_json(&AckKind::Acknowledged), via, ms(at)],
            )?;
            tx.execute(
                "UPDATE events SET closed_at = ?2 WHERE id = ?1",
                params![id, ms(at)],
            )?;
        }
        tx.commit()?;
        Ok(ids.len())
    }

    pub fn latest_ack(&self, event_id: EventId) -> Result<Option<Ack>> {
        self.conn
            .query_row(
                "SELECT kind, via, at FROM acks WHERE event_id = ?1
                 ORDER BY id DESC LIMIT 1",
                [event_id.to_string()],
                |r| {
                    let kind: String = r.get(0)?;
                    let via: String = r.get(1)?;
                    let at: i64 = r.get(2)?;
                    Ok((|| {
                        Ok(Ack {
                            event_id,
                            kind: from_json("ack kind", &kind)?,
                            via,
                            at: from_ms(at)?,
                        })
                    })())
                },
            )
            .optional()?
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeDelta, TimeZone};
    use magpie_core::{DeliveryOutcome, EventDraft, Tier};

    fn t(min: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 28, 20, 0, 0).unwrap() + TimeDelta::minutes(min)
    }

    fn event(title: &str, severity: Severity, at: DateTime<Utc>) -> Event {
        EventDraft {
            source: "test".into(),
            title: title.into(),
            body: "正文".into(),
            severity,
            tags: vec!["social".into()],
        }
        .validate(EventId::new(), at)
        .unwrap()
    }

    fn ack(e: &Event, kind: AckKind, at: DateTime<Utc>) -> Ack {
        Ack {
            event_id: e.id,
            kind,
            via: "desktop".into(),
            at,
        }
    }

    #[test]
    fn event_roundtrip() {
        let s = Store::open_in_memory().unwrap();
        let e = event("发小红书", Severity::Urgent, t(0));
        s.insert_event(&e).unwrap();
        let got = s.get_event(e.id).unwrap().unwrap();
        assert_eq!(got.event, e);
        assert_eq!(got.closed_at, None);
        assert!(s.get_event(EventId::new()).unwrap().is_none());
    }

    #[test]
    fn closing_by_tag_acks_only_open_events_with_that_exact_tag() {
        let mut s = Store::open_in_memory().unwrap();
        let tagged = |title: &str, tag: &str| {
            let mut e = event(title, Severity::Urgent, t(0));
            e.tags = vec!["planner".into(), tag.into()];
            e
        };
        let (a, b, other) = (
            tagged("a", "item:1"),
            tagged("b", "item:1"),
            tagged("c", "item:12"),
        );
        for e in [&a, &b, &other] {
            s.insert_event(e).unwrap();
        }
        s.record_ack(&ack(&a, AckKind::Dismissed, t(1))).unwrap();

        assert_eq!(s.close_events_tagged("item:1", "item", t(5)).unwrap(), 1);
        // Already closed: first final ack still wins.
        assert_eq!(s.get_event(a.id).unwrap().unwrap().closed_at, Some(t(1)));
        assert_eq!(s.get_event(b.id).unwrap().unwrap().closed_at, Some(t(5)));
        let latest = s.latest_ack(b.id).unwrap().unwrap();
        assert_eq!(
            (latest.kind, latest.via.as_str()),
            (AckKind::Acknowledged, "item")
        );
        assert_eq!(s.get_event(other.id).unwrap().unwrap().closed_at, None);
        assert_eq!(s.close_events_tagged("item:1", "item", t(6)).unwrap(), 0);
    }

    #[test]
    fn duplicate_id_is_an_error() {
        let s = Store::open_in_memory().unwrap();
        let e = event("a", Severity::Normal, t(0));
        s.insert_event(&e).unwrap();
        assert!(s.insert_event(&e).is_err());
    }

    #[test]
    fn list_is_newest_first_and_pages() {
        let s = Store::open_in_memory().unwrap();
        for i in 0..5 {
            s.insert_event(&event(&format!("e{i}"), Severity::Normal, t(i)))
                .unwrap();
        }
        let page1 = s
            .list_events(&EventQuery {
                limit: 2,
                ..EventQuery::default()
            })
            .unwrap();
        let titles: Vec<_> = page1.iter().map(|r| r.event.title.as_str()).collect();
        assert_eq!(titles, ["e4", "e3"]);

        let page2 = s
            .list_events(&EventQuery {
                limit: 2,
                before: Some(page1[1].event.created_at),
                ..EventQuery::default()
            })
            .unwrap();
        let titles: Vec<_> = page2.iter().map(|r| r.event.title.as_str()).collect();
        assert_eq!(titles, ["e2", "e1"]);
    }

    #[test]
    fn final_ack_closes_snooze_does_not() {
        let mut s = Store::open_in_memory().unwrap();
        let e = event("a", Severity::Urgent, t(0));
        s.insert_event(&e).unwrap();

        s.record_ack(&ack(&e, AckKind::Snoozed { until: t(30) }, t(1)))
            .unwrap();
        assert_eq!(s.get_event(e.id).unwrap().unwrap().closed_at, None);
        assert_eq!(s.events_needing_attention().unwrap().len(), 1);

        s.record_ack(&ack(&e, AckKind::Acknowledged, t(2))).unwrap();
        s.record_ack(&ack(&e, AckKind::Dismissed, t(3))).unwrap();
        let rec = s.get_event(e.id).unwrap().unwrap();
        assert_eq!(rec.closed_at, Some(t(2)), "first final ack wins");
        assert_eq!(
            s.latest_ack(e.id).unwrap().unwrap().kind,
            AckKind::Dismissed
        );
        assert!(s.events_needing_attention().unwrap().is_empty());
        assert!(s
            .list_events(&EventQuery {
                open_only: true,
                ..EventQuery::default()
            })
            .unwrap()
            .is_empty());
    }

    #[test]
    fn ack_for_unknown_event_is_rejected() {
        let mut s = Store::open_in_memory().unwrap();
        let ghost = event("ghost", Severity::Normal, t(0));
        assert!(matches!(
            s.record_ack(&ack(&ghost, AckKind::Acknowledged, t(0))),
            Err(StoreError::EventNotFound(_))
        ));
    }

    #[test]
    fn info_events_never_need_attention() {
        let s = Store::open_in_memory().unwrap();
        s.insert_event(&event("log", Severity::Info, t(0))).unwrap();
        s.insert_event(&event("ping", Severity::Normal, t(1)))
            .unwrap();
        let open = s.events_needing_attention().unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].title, "ping");
    }

    #[test]
    fn attempts_count_rounds_not_channels() {
        let s = Store::open_in_memory().unwrap();
        let e = event("a", Severity::Urgent, t(0));
        s.insert_event(&e).unwrap();
        let d = |channel: &str, at, outcome| Delivery {
            event_id: e.id,
            channel: channel.into(),
            tier: Tier::Phone,
            at,
            outcome,
        };
        s.record_delivery(&d("toast", t(0), DeliveryOutcome::Sent))
            .unwrap();
        s.record_delivery(&d("pet", t(0), DeliveryOutcome::Sent))
            .unwrap();
        s.record_delivery(&d(
            "telegram",
            t(10),
            DeliveryOutcome::Failed {
                error: "timeout".into(),
            },
        ))
        .unwrap();

        assert_eq!(s.attempt_times(e.id).unwrap(), vec![t(0), t(10)]);
        let all = s.deliveries(e.id).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(
            all[2].outcome,
            DeliveryOutcome::Failed {
                error: "timeout".into()
            }
        );
    }
}
