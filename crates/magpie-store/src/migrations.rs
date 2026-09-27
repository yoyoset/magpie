//! Forward-only schema migrations tracked in `PRAGMA user_version`.
//!
//! Append new steps; never edit a released one.

use rusqlite::Connection;

use crate::{Result, StoreError};

const STEPS: &[&str] = &[
    // v1
    r"
    CREATE TABLE events (
        id          TEXT PRIMARY KEY,
        source      TEXT NOT NULL,
        title       TEXT NOT NULL,
        body        TEXT NOT NULL,
        severity    TEXT NOT NULL,
        tags        TEXT NOT NULL,      -- JSON array
        created_at  INTEGER NOT NULL,   -- UTC ms
        closed_at   INTEGER             -- set by the first final ack
    );
    CREATE INDEX events_by_state ON events (closed_at, created_at);

    CREATE TABLE deliveries (
        id        INTEGER PRIMARY KEY,
        event_id  TEXT NOT NULL REFERENCES events (id) ON DELETE CASCADE,
        channel   TEXT NOT NULL,
        tier      TEXT NOT NULL,
        at        INTEGER NOT NULL,
        outcome   TEXT NOT NULL         -- JSON DeliveryOutcome
    );
    CREATE INDEX deliveries_by_event ON deliveries (event_id, at);

    CREATE TABLE acks (
        id        INTEGER PRIMARY KEY,
        event_id  TEXT NOT NULL REFERENCES events (id) ON DELETE CASCADE,
        kind      TEXT NOT NULL,        -- JSON AckKind
        via       TEXT NOT NULL,
        at        INTEGER NOT NULL
    );
    CREATE INDEX acks_by_event ON acks (event_id, id);

    CREATE TABLE reminders (
        id          TEXT PRIMARY KEY,
        schedule    TEXT NOT NULL,      -- JSON Schedule
        template    TEXT NOT NULL,      -- JSON EventDraft
        enabled     INTEGER NOT NULL,
        created_at  INTEGER NOT NULL,
        last_fired  INTEGER
    );
    ",
];

pub fn latest() -> u32 {
    u32::try_from(STEPS.len()).expect("fewer than 2^32 migrations")
}

pub fn apply(conn: &mut Connection) -> Result<()> {
    let current: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current > latest() {
        return Err(StoreError::SchemaTooNew {
            found: current,
            supported: latest(),
        });
    }
    for (version, sql) in (1..).zip(STEPS).skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_all_steps_once() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        apply(&mut conn).unwrap(); // idempotent: no "table already exists"
        let v: u32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, latest());
    }

    #[test]
    fn refuses_newer_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", latest() + 1)
            .unwrap();
        assert!(matches!(
            apply(&mut conn),
            Err(StoreError::SchemaTooNew { .. })
        ));
    }
}
