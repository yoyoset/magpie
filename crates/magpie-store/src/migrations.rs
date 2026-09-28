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
    // v2: todos and ideas
    r"
    CREATE TABLE items (
        id            TEXT PRIMARY KEY,
        kind          TEXT NOT NULL,       -- idea | todo
        status        TEXT NOT NULL,       -- open | done | dropped
        title         TEXT NOT NULL,
        why           TEXT NOT NULL,
        action        TEXT NOT NULL,
        source        TEXT NOT NULL,
        area          TEXT NOT NULL,
        day           TEXT,                -- local YYYY-MM-DD, sorts as text
        at            TEXT,                -- local HH:MM:SS
        deadline      TEXT,                -- local YYYY-MM-DD
        estimate_min  INTEGER,
        actual_min    INTEGER,
        blocking      INTEGER NOT NULL,
        deferrals     INTEGER NOT NULL,
        created_at    INTEGER NOT NULL,    -- UTC ms
        updated_at    INTEGER NOT NULL,
        done_at       INTEGER
    );
    CREATE INDEX items_by_status_day ON items (status, day);
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
    fn upgrades_existing_v1_database_without_losing_data() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(STEPS[0]).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO events (id, source, title, body, severity, tags, created_at)
             VALUES ('e1', 'cli', 'kept', '', 'normal', '[]', 0)",
            [],
        )
        .unwrap();

        apply(&mut conn).unwrap();

        let v: u32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 2);
        let title: String = conn
            .query_row("SELECT title FROM events WHERE id = 'e1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(title, "kept");
        let items: i64 = conn
            .query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(items, 0);
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
