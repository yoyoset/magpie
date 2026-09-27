//! Magpie domain core.
//!
//! Everything here is pure: no IO, no async, no reading the system clock.
//! Callers pass `now` explicitly so every decision is reproducible in tests.

pub mod ack;
pub mod event;

pub use ack::{Ack, AckKind};
pub use event::{Event, EventDraft, EventId, EventSource, Severity, ValidationError};
