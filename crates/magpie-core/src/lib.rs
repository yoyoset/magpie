//! Magpie domain core.
//!
//! Everything here is pure: no IO, no async, no reading the system clock.
//! Callers pass `now` (and the local time zone) explicitly so every decision is
//! reproducible in tests.

pub mod ack;
pub mod event;
pub mod schedule;

pub use ack::{Ack, AckKind};
pub use event::{Event, EventDraft, EventId, EventSource, Severity, ValidationError};
pub use schedule::{CronExpr, Schedule, ScheduleError};
