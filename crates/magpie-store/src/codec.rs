//! Conversions between domain values and `SQLite` columns.

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Serialize};

use crate::{Result, StoreError};

pub fn ms(t: DateTime<Utc>) -> i64 {
    t.timestamp_millis()
}

pub fn from_ms(v: i64) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp_millis(v).ok_or(StoreError::Corrupt {
        what: "timestamp",
        detail: v.to_string(),
    })
}

pub fn opt_from_ms(v: Option<i64>) -> Result<Option<DateTime<Utc>>> {
    v.map(from_ms).transpose()
}

pub fn to_json<T: Serialize>(v: &T) -> String {
    // Domain types serialize infallibly (no maps with non-string keys).
    serde_json::to_string(v).expect("domain value serializes to JSON")
}

pub fn from_json<T: DeserializeOwned>(what: &'static str, s: &str) -> Result<T> {
    serde_json::from_str(s).map_err(|e| StoreError::Corrupt {
        what,
        detail: e.to_string(),
    })
}

/// A unit enum as its bare serde name (`Severity::Urgent` -> `urgent`), so the
/// column is readable and indexable instead of holding a quoted JSON string.
pub fn enum_name<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        other => panic!("expected a unit enum, got {other:?}"),
    }
}

pub fn enum_from_name<T: DeserializeOwned>(what: &'static str, s: &str) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(s.to_owned())).map_err(|e| {
        StoreError::Corrupt {
            what,
            detail: e.to_string(),
        }
    })
}

pub fn uuid(what: &'static str, s: &str) -> Result<uuid::Uuid> {
    uuid::Uuid::parse_str(s).map_err(|e| StoreError::Corrupt {
        what,
        detail: e.to_string(),
    })
}
