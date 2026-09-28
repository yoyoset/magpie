//! `SQLite` persistence for Magpie.
//!
//! One [`Store`] owns one connection. It is deliberately synchronous and not
//! `Sync`; async callers wrap it (e.g. `Mutex` + `spawn_blocking`). Each table
//! lives in its own module as an `impl Store` block.
//!
//! Timestamps are stored as UTC milliseconds, so sub-millisecond precision is
//! dropped on the way in.

mod codec;
mod error;
mod events;
mod followups;
mod items;
mod migrations;
mod reminders;

use std::path::Path;

use rusqlite::Connection;

pub use error::StoreError;
pub use events::{EventQuery, EventRecord};
pub use items::{DayFilter, ItemQuery};

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug)]
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens (creating if needed) the database file and migrates it to the latest schema.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        // WAL lets the UI read while the runtime writes; busy_timeout covers the
        // short write overlaps instead of failing with SQLITE_BUSY.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrations::apply(&mut conn)?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<u32> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use magpie_core::{EventDraft, EventId};

    #[test]
    fn file_database_persists_across_reopen() {
        let dir = std::env::temp_dir().join(format!("magpie-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("magpie.db");

        let event = EventDraft {
            source: "test".into(),
            title: "persist me".into(),
            ..EventDraft::default()
        }
        .validate(
            EventId::new(),
            Utc.with_ymd_and_hms(2026, 9, 28, 0, 0, 0).unwrap(),
        )
        .unwrap();

        {
            let s = Store::open(&path).unwrap();
            s.insert_event(&event).unwrap();
            let mode: String = s
                .conn
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .unwrap();
            assert_eq!(mode, "wal");
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.schema_version().unwrap(), migrations::latest());
        assert_eq!(s.get_event(event.id).unwrap().unwrap().event, event);

        drop(s);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
